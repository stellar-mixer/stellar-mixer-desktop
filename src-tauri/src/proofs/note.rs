use anyhow::{bail, Result};
use rand_core::{OsRng, RngCore};
use rsa_risc0::traits::PublicKeyParts;
use rsa_risc0::{RsaPrivateKey, RsaPublicKey};
use sha2_risc0::{Digest, Sha256};

use crate::models::{NoteSecret, NULLIFIER_BYTES, RSA_MODULUS_BYTES, SECRET_BYTES};

#[derive(Clone)]
pub struct RuntimeNote {
    pub value: u128,
    pub owner_modulus: [u8; RSA_MODULUS_BYTES],
    pub nullifier: [u8; NULLIFIER_BYTES],
    pub secret: [u8; SECRET_BYTES],
}

#[allow(dead_code)]
pub fn create_rsa_owner_key() -> Result<(RsaPrivateKey, [u8; RSA_MODULUS_BYTES])> {
    let mut rng = OsRng;
    let private_key = RsaPrivateKey::new(&mut rng, 2048)?;
    let public_key = RsaPublicKey::from(&private_key);
    let modulus = fixed_be_array(&public_key.n().to_bytes_be())?;
    Ok((private_key, modulus))
}

pub fn random_runtime_note(value: u128, owner_modulus: [u8; RSA_MODULUS_BYTES]) -> RuntimeNote {
    let mut rng = OsRng;
    let mut nullifier = [0u8; NULLIFIER_BYTES];
    let mut secret = [0u8; SECRET_BYTES];
    rng.fill_bytes(&mut nullifier);
    rng.fill_bytes(&mut secret);

    RuntimeNote {
        value,
        owner_modulus,
        nullifier,
        secret,
    }
}

pub fn runtime_note_from_secret(note: &NoteSecret) -> Result<RuntimeNote> {
    Ok(RuntimeNote {
        value: note.value,
        owner_modulus: fixed_be_array(&hex::decode(&note.owner_modulus_hex)?)?,
        nullifier: fixed_be_array(&hex::decode(&note.nullifier_hex)?)?,
        secret: fixed_be_array(&hex::decode(&note.secret_hex)?)?,
    })
}

pub fn note_leaf(note: &RuntimeNote) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"stellar-mixer-note-leaf-v1");
    h.update(note.value.to_be_bytes());
    h.update(note.owner_modulus);
    h.update(note.nullifier);
    h.update(note.secret);
    h.finalize().into()
}

pub fn to_note_secret(
    account_id: String,
    value: u128,
    note: &RuntimeNote,
    leaf_index: Option<u64>,
) -> NoteSecret {
    let leaf = note_leaf(note);
    NoteSecret {
        id: uuid::Uuid::new_v4().to_string(),
        account_id,
        value,
        owner_modulus_hex: hex::encode(note.owner_modulus),
        nullifier_hex: hex::encode(note.nullifier),
        secret_hex: hex::encode(note.secret),
        leaf_hex: hex::encode(leaf),
        leaf_index,
        spent: false,
        created_at: chrono::Utc::now().timestamp_millis(),
        spent_at: None,
        source_event_id: None,
        source_ledger: None,
        source_kind: None,
        message: None,
    }
}

pub fn fixed_be_array<const N: usize>(bytes: &[u8]) -> Result<[u8; N]> {
    if bytes.len() > N {
        bail!("byte array too large: {} > {}", bytes.len(), N);
    }

    let mut out = [0u8; N];
    let start = N - bytes.len();
    out[start..].copy_from_slice(bytes);
    Ok(out)
}
