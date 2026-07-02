use anyhow::{anyhow, bail, Context, Result};
use rand_core::OsRng;
use rsa_risc0::{BigUint, Oaep, RsaPrivateKey, RsaPublicKey};
use sha2_risc0::Sha256;

use crate::models::{NULLIFIER_BYTES, RSA_MODULUS_BYTES, SECRET_BYTES};
use crate::proofs::note::RuntimeNote;

pub const ENCRYPTED_NOTE_MAGIC: &[u8; 4] = b"SMN1";
pub const ENCRYPTED_NOTE_MAX_PLAINTEXT_BYTES: usize = 190;
pub const ENCRYPTED_NOTE_MAX_ACCOUNT_HINT_BYTES: usize = 36;
pub const ENCRYPTED_NOTE_MAX_MEMO_BYTES: usize = 64;

const RSA_E_BYTES: [u8; 3] = [0x01, 0x00, 0x01];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EncryptedNoteKind {
    DepositSelf = 1,
    WithdrawChange = 2,
    TransferReceived = 3,
    TransferChange = 4,
}

impl EncryptedNoteKind {
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::DepositSelf),
            2 => Ok(Self::WithdrawChange),
            3 => Ok(Self::TransferReceived),
            4 => Ok(Self::TransferChange),
            other => bail!("unknown encrypted note kind: {other}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecodedEncryptedNotePayload {
    pub kind: EncryptedNoteKind,
    pub value: u128,
    pub nullifier: [u8; NULLIFIER_BYTES],
    pub secret: [u8; SECRET_BYTES],
    pub account_hint: Option<String>,
    pub memo: Option<String>,
}

impl DecodedEncryptedNotePayload {
    pub fn to_runtime_note(&self, owner_modulus: [u8; RSA_MODULUS_BYTES]) -> RuntimeNote {
        RuntimeNote {
            value: self.value,
            owner_modulus,
            nullifier: self.nullifier,
            secret: self.secret,
        }
    }
}

pub fn encrypt_runtime_note_for_modulus(
    note: &RuntimeNote,
    recipient_modulus: &[u8; RSA_MODULUS_BYTES],
    kind: EncryptedNoteKind,
    account_hint: Option<&str>,
    memo: Option<&str>,
) -> Result<Vec<u8>> {
    let plaintext = encode_payload(note, kind, account_hint, memo)?;
    let public_key = public_key_from_modulus(recipient_modulus)?;

    let ciphertext = public_key
        .encrypt(&mut OsRng, Oaep::new::<Sha256>(), &plaintext)
        .map_err(|error| anyhow!("failed to RSA-OAEP encrypt note payload: {error}"))?;

    if ciphertext.len() != RSA_MODULUS_BYTES {
        bail!(
            "unexpected encrypted note ciphertext length: expected {}, got {}",
            RSA_MODULUS_BYTES,
            ciphertext.len()
        );
    }

    Ok(ciphertext)
}

pub fn decrypt_encrypted_note(
    rsa_private_key: &RsaPrivateKey,
    ciphertext: &[u8],
) -> Result<DecodedEncryptedNotePayload> {
    if ciphertext.len() != RSA_MODULUS_BYTES {
        bail!(
            "encrypted note ciphertext must be {} bytes, got {}",
            RSA_MODULUS_BYTES,
            ciphertext.len()
        );
    }

    let plaintext = rsa_private_key
        .decrypt(Oaep::new::<Sha256>(), ciphertext)
        .map_err(|error| anyhow!("encrypted note is not for this Mixer Identity: {error}"))?;

    decode_payload(&plaintext)
}

fn encode_payload(
    note: &RuntimeNote,
    kind: EncryptedNoteKind,
    account_hint: Option<&str>,
    memo: Option<&str>,
) -> Result<Vec<u8>> {
    let account_hint_bytes = account_hint.unwrap_or("").as_bytes();
    let memo_bytes = memo.unwrap_or("").as_bytes();

    if account_hint_bytes.len() > ENCRYPTED_NOTE_MAX_ACCOUNT_HINT_BYTES {
        bail!(
            "encrypted note account hint too large: {} > {} bytes",
            account_hint_bytes.len(),
            ENCRYPTED_NOTE_MAX_ACCOUNT_HINT_BYTES
        );
    }

    if memo_bytes.len() > ENCRYPTED_NOTE_MAX_MEMO_BYTES {
        bail!(
            "encrypted note memo too large: {} > {} bytes",
            memo_bytes.len(),
            ENCRYPTED_NOTE_MAX_MEMO_BYTES
        );
    }

    let mut out = Vec::with_capacity(ENCRYPTED_NOTE_MAX_PLAINTEXT_BYTES);

    out.extend_from_slice(ENCRYPTED_NOTE_MAGIC);
    out.push(kind as u8);
    out.push(0u8);
    out.extend_from_slice(&note.value.to_be_bytes());
    out.extend_from_slice(&note.nullifier);
    out.extend_from_slice(&note.secret);

    out.push(account_hint_bytes.len() as u8);
    out.extend_from_slice(account_hint_bytes);

    out.push(memo_bytes.len() as u8);
    out.extend_from_slice(memo_bytes);

    if out.len() > ENCRYPTED_NOTE_MAX_PLAINTEXT_BYTES {
        bail!(
            "encrypted note payload too large: {} > {} bytes",
            out.len(),
            ENCRYPTED_NOTE_MAX_PLAINTEXT_BYTES
        );
    }

    Ok(out)
}

fn decode_payload(payload: &[u8]) -> Result<DecodedEncryptedNotePayload> {
    let mut offset = 0usize;

    let magic = take(payload, &mut offset, 4)?;
    if magic != ENCRYPTED_NOTE_MAGIC {
        bail!("invalid encrypted note payload magic");
    }

    let kind = EncryptedNoteKind::from_u8(take(payload, &mut offset, 1)?[0])?;
    let _flags = take(payload, &mut offset, 1)?[0];

    let value = {
        let bytes: [u8; 16] = take(payload, &mut offset, 16)?
            .try_into()
            .expect("fixed 16 bytes");
        u128::from_be_bytes(bytes)
    };

    let nullifier: [u8; NULLIFIER_BYTES] = take(payload, &mut offset, NULLIFIER_BYTES)?
        .try_into()
        .expect("fixed nullifier bytes");

    let secret: [u8; SECRET_BYTES] = take(payload, &mut offset, SECRET_BYTES)?
        .try_into()
        .expect("fixed secret bytes");

    let account_hint_len = take(payload, &mut offset, 1)?[0] as usize;
    if account_hint_len > ENCRYPTED_NOTE_MAX_ACCOUNT_HINT_BYTES {
        bail!("encrypted note account hint length is too large");
    }

    let account_hint = if account_hint_len == 0 {
        None
    } else {
        Some(
            String::from_utf8(take(payload, &mut offset, account_hint_len)?.to_vec())
                .context("encrypted note account hint is not utf8")?,
        )
    };

    let memo_len = take(payload, &mut offset, 1)?[0] as usize;
    if memo_len > ENCRYPTED_NOTE_MAX_MEMO_BYTES {
        bail!("encrypted note memo length is too large");
    }

    let memo = if memo_len == 0 {
        None
    } else {
        Some(
            String::from_utf8(take(payload, &mut offset, memo_len)?.to_vec())
                .context("encrypted note memo is not utf8")?,
        )
    };

    Ok(DecodedEncryptedNotePayload {
        kind,
        value,
        nullifier,
        secret,
        account_hint,
        memo,
    })
}

fn take<'a>(payload: &'a [u8], offset: &mut usize, len: usize) -> Result<&'a [u8]> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| anyhow!("encrypted note payload offset overflow"))?;

    if end > payload.len() {
        bail!("truncated encrypted note payload");
    }

    let out = &payload[*offset..end];
    *offset = end;
    Ok(out)
}

fn public_key_from_modulus(modulus: &[u8; RSA_MODULUS_BYTES]) -> Result<RsaPublicKey> {
    let n = BigUint::from_bytes_be(modulus);
    let e = BigUint::from_bytes_be(&RSA_E_BYTES);

    RsaPublicKey::new(n, e).context("failed to build RSA public key from Mixer Identity modulus")
}
