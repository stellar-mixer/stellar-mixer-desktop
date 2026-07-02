use anyhow::{bail, Context, Result};
use parity_scale_codec::Decode;
use risc0_ethereum_contracts::encode_seal;
use risc0_zkvm::{default_prover, ExecutorEnv, ExecutorEnvBuilder, ProverOpts, Receipt};
use rsa_risc0::pkcs1v15::{SigningKey, VerifyingKey};
use rsa_risc0::signature::{SignatureEncoding, Signer, Verifier};
use rsa_risc0::traits::PublicKeyParts;
use rsa_risc0::{RsaPrivateKey, RsaPublicKey};
use sha2_risc0::{Digest, Sha256};
use std::io::Read;
use std::time::{Duration, Instant};
use treepir_client::{TreePirClientConfig, TreePirRemoteClient};

use crate::config::{
    DEPTH, GROTH16_WRAP_URL, TREEPIR_URL, WITHDRAW_GUEST_ELF_PATH, WITHDRAW_GUEST_ID,
};
use crate::models::{NULLIFIER_BYTES, RSA_MODULUS_BYTES};
use crate::proofs::note::{fixed_be_array, note_leaf, RuntimeNote};

const SIGNATURE_BYTES: usize = 256;
const RSA_E_BYTES: [u8; 3] = [0x01, 0x00, 0x01];
const MAX_INPUT_NOTES: usize = 8;

#[derive(Debug, Decode)]
pub struct WithdrawJournal {
    pub root: [u8; 32],
    pub nullifiers: Vec<[u8; 32]>,
    pub withdraw_amount: u128,
    pub output_leaf: [u8; 32],
}

pub struct WithdrawProofResult {
    pub seal: Vec<u8>,
    pub journal: Vec<u8>,
    pub output_leaf: [u8; 32],
    pub root: [u8; 32],
    pub nullifiers: Vec<[u8; 32]>,
}

pub async fn build_withdraw_proof(
    input_notes: Vec<(u64, RuntimeNote)>,
    withdraw_amount: u128,
    change_note: RuntimeNote,
    rsa_private_key: RsaPrivateKey,
    progress: impl Fn(&str, &str) + Send + Sync,
) -> Result<WithdrawProofResult> {
    if input_notes.is_empty() {
        bail!("withdraw requires at least one input note");
    }

    if input_notes.len() > MAX_INPUT_NOTES {
        bail!("withdraw supports max {MAX_INPUT_NOTES} input notes");
    }

    let input_sum: u128 = input_notes.iter().map(|(_, note)| note.value).sum();

    if withdraw_amount == 0 || withdraw_amount > input_sum {
        bail!("withdraw amount must be > 0 and <= selected input sum");
    }

    if input_sum != withdraw_amount.saturating_add(change_note.value) {
        bail!(
            "withdraw value mismatch: input_sum={input_sum}, withdraw_amount={withdraw_amount}, change={}",
            change_note.value
        );
    }

    progress(
        "treepir",
        "opening TreePIR session and requesting private Merkle paths",
    );

    let treepir_start = Instant::now();

    let opened = TreePirRemoteClient::open_or_register(
        TreePirClientConfig::new(TREEPIR_URL.to_string()),
        None,
    )
    .await?;

    let mut root: Option<[u8; 32]> = None;
    let mut merkle_paths = Vec::<[[u8; 32]; DEPTH]>::with_capacity(input_notes.len());
    let mut input_leaves = Vec::<[u8; 32]>::with_capacity(input_notes.len());
    let mut nullifiers = Vec::<[u8; 32]>::with_capacity(input_notes.len());

    for (position, (leaf_index, note)) in input_notes.iter().enumerate() {
        let extracted = opened
            .client
            .path_for_leaf::<DEPTH>(*leaf_index as usize)
            .await?;

        progress(
            "treepir",
            &format!(
                "TreePIR path {}/{} ready in {:?}",
                position + 1,
                input_notes.len(),
                treepir_start.elapsed()
            ),
        );

        let input_leaf = note_leaf(note);

        if !extracted.verify(input_leaf) {
            bail!("TreePIR path does not verify for selected withdraw note at index {leaf_index}");
        }

        let extracted_root = extracted.root();

        if let Some(existing_root) = root {
            if existing_root != extracted_root {
                bail!("selected withdraw notes resolve to different Merkle roots");
            }
        } else {
            root = Some(extracted_root);
        }

        let siblings = extracted.siblings();

        if siblings.len() != DEPTH {
            bail!(
                "unexpected path depth: expected {DEPTH}, got {}",
                siblings.len()
            );
        }

        let mut path = [[0u8; 32]; DEPTH];
        for (dst, src) in path.iter_mut().zip(siblings.iter()) {
            *dst = *src;
        }

        merkle_paths.push(path);
        input_leaves.push(input_leaf);
        nullifiers.push(note.nullifier);
    }

    let root = root.context("withdraw proof has no root")?;
    let output_leaf = note_leaf(&change_note);

    let action_hash = hash_withdraw_action(
        &root,
        &input_leaves,
        &nullifiers,
        withdraw_amount,
        &output_leaf,
    );

    progress(
        "witness",
        "signing withdraw action with Mixer Identity RSA key",
    );

    let public_key = RsaPublicKey::from(&rsa_private_key);
    validate_rsa_public_key(&public_key)?;
    let signature = sign_action(&rsa_private_key, &public_key, &action_hash)?;

    progress(
        "stark",
        "creating local RISC0 STARK/composite withdraw proof",
    );

    let withdraw_elf = std::fs::read(WITHDRAW_GUEST_ELF_PATH).with_context(|| {
        format!("failed to read withdraw guest ELF at {WITHDRAW_GUEST_ELF_PATH}")
    })?;

    let mut builder = ExecutorEnv::builder();

    write_withdraw_guest_input(
        &mut builder,
        &root,
        &input_notes,
        &merkle_paths,
        withdraw_amount,
        &change_note,
        &signature,
    )?;

    let env = builder.build()?;

    let stark_start = Instant::now();
    let stark_receipt = default_prover()
        .prove_with_opts(env, &withdraw_elf, &ProverOpts::composite())?
        .receipt;

    stark_receipt.verify(WITHDRAW_GUEST_ID)?;

    progress(
        "stark",
        &format!("withdraw STARK proof ready in {:?}", stark_start.elapsed()),
    );

    let stark_journal = stark_receipt.journal.bytes.clone();
    let decoded = WithdrawJournal::decode(&mut &stark_journal[..])?;

    if decoded.root != root {
        bail!("withdraw journal root differs from TreePIR root");
    }

    if decoded.nullifiers != nullifiers {
        bail!("withdraw journal nullifiers differ from selected input notes");
    }

    if decoded.withdraw_amount != withdraw_amount {
        bail!("withdraw journal amount differs from requested amount");
    }

    if decoded.output_leaf != output_leaf {
        bail!("withdraw journal output leaf differs from change note leaf");
    }

    progress(
        "groth16",
        "sending withdraw STARK receipt to remote Groth16 wrap server",
    );

    let groth16_start = Instant::now();
    let groth16_receipt = wrap_receipt_remote(&stark_receipt)?;
    groth16_receipt.verify(WITHDRAW_GUEST_ID)?;

    if groth16_receipt.journal.bytes != stark_journal {
        bail!("Groth16 receipt journal differs from STARK journal");
    }

    let seal = encode_seal(&groth16_receipt)?;

    progress(
        "groth16",
        &format!("Groth16 wrap completed in {:?}", groth16_start.elapsed()),
    );

    progress(
        "groth16",
        &format!("Groth16 encoded router seal ready: {} bytes", seal.len()),
    );

    Ok(WithdrawProofResult {
        seal,
        journal: stark_journal,
        output_leaf,
        root,
        nullifiers,
    })
}

fn write_withdraw_guest_input(
    builder: &mut ExecutorEnvBuilder<'_>,
    root: &[u8; 32],
    input_notes: &[(u64, RuntimeNote)],
    merkle_paths: &[[[u8; 32]; DEPTH]],
    withdraw_amount: u128,
    change_note: &RuntimeNote,
    signature: &[u8; SIGNATURE_BYTES],
) -> Result<()> {
    builder.write_slice(root);
    builder.write(&(input_notes.len() as u32))?;

    for (idx, (leaf_index, note)) in input_notes.iter().enumerate() {
        builder.write(leaf_index)?;
        write_note(builder, note)?;

        for sibling in merkle_paths[idx].iter() {
            builder.write_slice(sibling);
        }
    }

    builder.write(&withdraw_amount)?;
    write_note(builder, change_note)?;
    builder.write_slice(signature);

    Ok(())
}

fn write_note(builder: &mut ExecutorEnvBuilder<'_>, note: &RuntimeNote) -> Result<()> {
    builder.write(&note.value)?;
    builder.write_slice(&note.owner_modulus);
    builder.write_slice(&note.nullifier);
    builder.write_slice(&note.secret);
    Ok(())
}

fn sign_action(
    private_key: &RsaPrivateKey,
    public_key: &RsaPublicKey,
    action_hash: &[u8; 32],
) -> Result<[u8; SIGNATURE_BYTES]> {
    let signing_key = SigningKey::<Sha256>::new(private_key.clone());
    let verifying_key = VerifyingKey::<Sha256>::new(public_key.clone());
    let signature = signing_key.sign(action_hash);
    verifying_key.verify(action_hash, &signature)?;
    fixed_be_array(signature.to_bytes().as_ref())
}

fn validate_rsa_public_key(public_key: &RsaPublicKey) -> Result<()> {
    if public_key.n().to_bytes_be().len() > RSA_MODULUS_BYTES {
        bail!("RSA modulus too large");
    }

    if public_key.e().to_bytes_be().as_slice() != RSA_E_BYTES.as_slice() {
        bail!("unexpected RSA exponent");
    }

    Ok(())
}

fn hash_withdraw_action(
    root: &[u8; 32],
    input_leaves: &[[u8; 32]],
    nullifiers: &[[u8; NULLIFIER_BYTES]],
    withdraw_amount: u128,
    output_leaf: &[u8; 32],
) -> [u8; 32] {
    let mut h = Sha256::new();

    h.update(b"stellar-mixer-withdraw-action-v1");
    h.update(root);

    h.update((input_leaves.len() as u32).to_be_bytes());
    for leaf in input_leaves {
        h.update(leaf);
    }

    h.update((nullifiers.len() as u32).to_be_bytes());
    for nullifier in nullifiers {
        h.update(nullifier);
    }

    h.update(withdraw_amount.to_be_bytes());
    h.update(output_leaf);

    h.finalize().into()
}

fn wrap_receipt_remote(stark_receipt: &Receipt) -> Result<Receipt> {
    let body = bincode::serialize(stark_receipt)?;

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(900))
        .timeout_write(Duration::from_secs(900))
        .build();

    let response = match agent
        .post(GROTH16_WRAP_URL)
        .set("content-type", "application/octet-stream")
        .set("connection", "close")
        .send_bytes(&body)
    {
        Ok(response) => response,
        Err(ureq::Error::Status(status, response)) => {
            let mut text = String::new();
            let _ = response.into_reader().read_to_string(&mut text);
            bail!("Groth16 wrap server returned HTTP {status}: {text}");
        }
        Err(error) => bail!("Groth16 wrap request failed: {error}"),
    };

    let mut bytes = Vec::new();
    response.into_reader().read_to_end(&mut bytes)?;
    Ok(bincode::deserialize(&bytes)?)
}
