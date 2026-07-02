use anyhow::{anyhow, bail, Context, Result};
use rsa_risc0::pkcs1::DecodeRsaPrivateKey;
use rsa_risc0::RsaPrivateKey;
use serde::Deserialize;

use crate::config::TREEPIR_URL;
use crate::mixer::coin_selection::{
    format_xlm, select_notes_for_transfer, select_notes_for_withdraw,
};
use crate::models::{
    AccountSecret, DepositResult, MixerIdentitySecret, NoteSecret, TransferResult, VaultPayload,
    WithdrawResult,
};
use crate::proofs::encrypted_note::{
    encrypt_runtime_note_for_modulus, EncryptedNoteKind, ENCRYPTED_NOTE_MAX_MEMO_BYTES,
};
use crate::proofs::note::{
    fixed_be_array, note_leaf, random_runtime_note, runtime_note_from_secret, to_note_secret,
};
use crate::proofs::transfer::build_transfer_proof;
use crate::proofs::withdraw::build_withdraw_proof;
use crate::stellar::contract::{
    ensure_source_account_exists, invoke_deposit, invoke_transfer, invoke_withdraw,
};

#[derive(Debug, Deserialize)]
struct TreePirReadyResponse {
    leaf_count: usize,
}

pub async fn deposit_for_account(
    account: &AccountSecret,
    identity: &MixerIdentitySecret,
    amount: u128,
) -> Result<(DepositResult, NoteSecret)> {
    if amount == 0 {
        bail!("deposit amount must be > 0 XLM");
    }

    ensure_source_account_exists(&account.stellar_secret_key, "deposit").await?;

    let owner_modulus = identity_owner_modulus(identity)?;
    let runtime_note = random_runtime_note(amount, owner_modulus);
    let leaf = note_leaf(&runtime_note);

    let encrypted_note = encrypt_runtime_note_for_modulus(
        &runtime_note,
        &owner_modulus,
        EncryptedNoteKind::DepositSelf,
        Some(&account.id),
        None,
    )?;

    let (tx_hash, leaf_index) = invoke_deposit(
        &account.stellar_secret_key,
        amount as i128,
        leaf,
        &encrypted_note,
    )
    .await?;

    let note = to_note_secret(account.id.clone(), amount, &runtime_note, Some(leaf_index));

    Ok((
        DepositResult {
            tx_hash,
            leaf_index,
            note_id: note.id.clone(),
            leaf_hex: hex::encode(leaf),
            amount: amount.to_string(),
        },
        note,
    ))
}

pub async fn withdraw_for_identity(
    recipient_account: &AccountSecret,
    fee_payer_account: &AccountSecret,
    vault: &VaultPayload,
    amount: u128,
    recipient_address: Option<String>,
    progress: impl Fn(&str, &str) + Send + Sync,
) -> Result<(WithdrawResult, Vec<NoteSecret>)> {
    if amount == 0 {
        bail!("withdraw amount must be > 0 XLM");
    }

    let withdraw_recipient_public_key = normalize_withdraw_recipient(
        recipient_address.as_deref(),
        &recipient_account.stellar_public_key,
    )?;

    progress(
        "preflight",
        &format!(
            "checking fee payer {} before proof generation",
            fee_payer_account.stellar_public_key
        ),
    );

    ensure_source_account_exists(&fee_payer_account.stellar_secret_key, "withdraw fee payer")
        .await?;

    progress(
        "coin-selection",
        "selecting up to 8 notes from the whole Mixer Identity private note pool",
    );

    let identity_modulus = vault
        .identity
        .rsa_public_modulus_hex
        .clone()
        .ok_or_else(|| anyhow!("Mixer Identity RSA public modulus is missing"))?;

    let identity_notes: Vec<NoteSecret> = vault
        .notes
        .iter()
        .filter(|note| {
            note.owner_modulus_hex
                .eq_ignore_ascii_case(&identity_modulus)
        })
        .cloned()
        .collect();

    let selection = select_notes_for_withdraw(&identity_notes, amount)?;

    let input_notes = selection
        .notes
        .iter()
        .map(|note| {
            Ok((
                note.leaf_index
                    .ok_or_else(|| anyhow!("selected withdraw note is not indexed"))?,
                runtime_note_from_secret(note)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    let change_value = selection.change;
    let change_owner_modulus = identity_owner_modulus(&vault.identity)?;
    let change_runtime_note = random_runtime_note(change_value, change_owner_modulus);

    let encrypted_change_note = encrypt_runtime_note_for_modulus(
        &change_runtime_note,
        &change_owner_modulus,
        EncryptedNoteKind::WithdrawChange,
        Some(&recipient_account.id),
        None,
    )?;

    let rsa_private = identity_signing_key(&vault.identity)?;

    progress(
        "coin-selection",
        &format!(
            "selected {} identity note(s): input {}, withdraw {}, change {}",
            selection.notes.len(),
            format_xlm(selection.input_sum),
            format_xlm(amount),
            format_xlm(change_value)
        ),
    );

    let proof = build_withdraw_proof(
        input_notes,
        amount,
        change_runtime_note.clone(),
        rsa_private,
        &progress,
    )
    .await?;

    let expected_change_leaf_index = if change_value > 0 {
        fetch_treepir_leaf_count()
            .await
            .ok()
            .map(|count| count as u64)
    } else {
        None
    };

    progress(
        "stellar",
        &format!(
            "submitting withdraw tx: fee payer {}, recipient {}",
            fee_payer_account.stellar_public_key, withdraw_recipient_public_key
        ),
    );

    let tx_hash = invoke_withdraw(
        &fee_payer_account.stellar_secret_key,
        &proof.seal,
        &proof.journal,
        Some(&withdraw_recipient_public_key),
        &encrypted_change_note,
    )
    .await?;

    let mut updated = Vec::new();

    for input in selection.notes.iter() {
        let mut spent_input = input.clone();
        spent_input.spent = true;
        spent_input.spent_at = Some(chrono::Utc::now().timestamp_millis());
        updated.push(spent_input);
    }

    let created_note_id = if change_value > 0 {
        let change_note = to_note_secret(
            recipient_account.id.clone(),
            change_value,
            &change_runtime_note,
            expected_change_leaf_index,
        );

        let id = change_note.id.clone();
        updated.push(change_note);
        Some(id)
    } else {
        None
    };

    Ok((
        WithdrawResult {
            tx_hash,
            spent_note_ids: selection.notes.iter().map(|note| note.id.clone()).collect(),
            created_note_id,
            amount: amount.to_string(),
        },
        updated,
    ))
}

pub async fn transfer_for_identity(
    fee_payer_account: &AccountSecret,
    vault: &VaultPayload,
    amount: u128,
    recipient_identity: &str,
    message: Option<String>,
    progress: impl Fn(&str, &str) + Send + Sync,
) -> Result<(TransferResult, Vec<NoteSecret>)> {
    if amount == 0 {
        bail!("transfer amount must be > 0 XLM");
    }

    let message = normalize_transfer_message(message)?;

    progress(
        "preflight",
        &format!(
            "checking transfer fee payer {} before proof generation",
            fee_payer_account.stellar_public_key
        ),
    );

    ensure_source_account_exists(&fee_payer_account.stellar_secret_key, "transfer fee payer")
        .await?;

    let recipient_modulus = parse_mixer_identity_modulus(recipient_identity)?;
    let self_modulus_hex = vault
        .identity
        .rsa_public_modulus_hex
        .clone()
        .ok_or_else(|| anyhow!("Mixer Identity RSA public modulus is missing"))?;

    let identity_notes: Vec<NoteSecret> = vault
        .notes
        .iter()
        .filter(|note| {
            note.owner_modulus_hex
                .eq_ignore_ascii_case(&self_modulus_hex)
        })
        .cloned()
        .collect();

    progress(
        "coin-selection",
        "selecting up to 8 input notes from Mixer Identity private note pool",
    );

    let selection = select_notes_for_transfer(&identity_notes, amount)?;

    if selection.notes.len() > 8 {
        bail!("transfer selected more than 8 input notes");
    }

    let input_notes = selection
        .notes
        .iter()
        .map(|note| {
            Ok((
                note.leaf_index
                    .ok_or_else(|| anyhow!("selected transfer note is not indexed"))?,
                runtime_note_from_secret(note)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    let recipient_note = random_runtime_note(amount, recipient_modulus);

    let mut output_notes = vec![recipient_note.clone()];
    let mut encrypted_notes = vec![encrypt_runtime_note_for_modulus(
        &recipient_note,
        &recipient_modulus,
        EncryptedNoteKind::TransferReceived,
        None,
        message.as_deref(),
    )?];

    let change_note_runtime = if selection.change > 0 {
        let self_modulus = identity_owner_modulus(&vault.identity)?;
        let change = random_runtime_note(selection.change, self_modulus);

        encrypted_notes.push(encrypt_runtime_note_for_modulus(
            &change,
            &self_modulus,
            EncryptedNoteKind::TransferChange,
            Some(&fee_payer_account.id),
            None,
        )?);

        output_notes.push(change.clone());
        Some(change)
    } else {
        None
    };

    progress(
        "coin-selection",
        &format!(
            "transfer selected {} input note(s), input {}, amount {}, change {}",
            selection.notes.len(),
            format_xlm(selection.input_sum),
            format_xlm(amount),
            format_xlm(selection.change)
        ),
    );

    let rsa_private = identity_signing_key(&vault.identity)?;

    let proof = build_transfer_proof(input_notes, output_notes, rsa_private, &progress).await?;

    let expected_start_index = fetch_treepir_leaf_count()
        .await
        .ok()
        .map(|count| count as u64);

    progress("stellar", "submitting transfer tx to mixer contract");

    let tx_hash = invoke_transfer(
        &fee_payer_account.stellar_secret_key,
        &proof.seal,
        &proof.journal,
        &encrypted_notes,
    )
    .await?;

    let mut updated = Vec::new();

    for input in selection.notes.iter() {
        let mut spent = input.clone();
        spent.spent = true;
        spent.spent_at = Some(chrono::Utc::now().timestamp_millis());
        updated.push(spent);
    }

    let mut created_note_ids = Vec::new();

    if let Some(change_runtime) = change_note_runtime {
        let change_leaf_index = expected_start_index.map(|start| start + 1);

        let change_note = to_note_secret(
            fee_payer_account.id.clone(),
            selection.change,
            &change_runtime,
            change_leaf_index,
        );

        created_note_ids.push(change_note.id.clone());
        updated.push(change_note);
    }

    Ok((
        TransferResult {
            tx_hash,
            spent_note_ids: selection.notes.iter().map(|note| note.id.clone()).collect(),
            created_note_ids,
            amount: amount.to_string(),
            recipient_identity: recipient_identity.to_string(),
        },
        updated,
    ))
}

fn normalize_withdraw_recipient(input: Option<&str>, fallback: &str) -> Result<String> {
    let recipient = input
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback);

    if recipient.len() != 56 || !recipient.starts_with('G') {
        bail!("withdraw recipient address must be a Stellar public address starting with G");
    }

    if !recipient
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        bail!("withdraw recipient address contains invalid characters");
    }

    Ok(recipient.to_string())
}

fn normalize_transfer_message(message: Option<String>) -> Result<Option<String>> {
    let Some(message) = message else {
        return Ok(None);
    };

    let trimmed = message.trim().to_string();

    if trimmed.is_empty() {
        return Ok(None);
    }

    if trimmed.as_bytes().len() > ENCRYPTED_NOTE_MAX_MEMO_BYTES {
        bail!(
            "transfer message is too large: {} > {} bytes",
            trimmed.as_bytes().len(),
            ENCRYPTED_NOTE_MAX_MEMO_BYTES
        );
    }

    Ok(Some(trimmed))
}

fn parse_mixer_identity_modulus(input: &str) -> Result<[u8; 256]> {
    let trimmed = input.trim();
    let raw = trimmed.strip_prefix("smxid1:").unwrap_or(trimmed);

    if raw.len() != 512 {
        bail!("recipient Mixer Identity must be smxid1:<512 hex chars RSA modulus>");
    }

    if !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("recipient Mixer Identity contains non-hex characters");
    }

    fixed_be_array(&hex::decode(raw)?)
}

fn identity_owner_modulus(identity: &MixerIdentitySecret) -> Result<[u8; 256]> {
    let modulus_hex = identity.rsa_public_modulus_hex.as_ref().ok_or_else(|| {
        anyhow!("Mixer Identity RSA public modulus is missing; lock/unlock vault to migrate it")
    })?;

    fixed_be_array(&hex::decode(modulus_hex)?)
}

fn identity_signing_key(identity: &MixerIdentitySecret) -> Result<RsaPrivateKey> {
    let private_hex = identity
        .rsa_private_key_pkcs1_der_hex
        .as_ref()
        .ok_or_else(|| {
            anyhow!("Mixer Identity RSA private key is missing; lock/unlock vault to migrate it")
        })?;

    let der = hex::decode(private_hex)?;

    Ok(RsaPrivateKey::from_pkcs1_der(&der)
        .context("failed to decode Mixer Identity RSA private key")?)
}

async fn fetch_treepir_leaf_count() -> Result<usize> {
    let url = format!("{}/ready", TREEPIR_URL.trim_end_matches('/'));

    let ready = reqwest::get(&url)
        .await
        .with_context(|| format!("failed to request TreePIR status from {url}"))?
        .error_for_status()
        .with_context(|| format!("TreePIR status request failed for {url}"))?
        .json::<TreePirReadyResponse>()
        .await
        .context("failed to decode TreePIR /ready response")?;

    Ok(ready.leaf_count)
}
