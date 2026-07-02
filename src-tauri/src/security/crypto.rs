use anyhow::{anyhow, bail, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    Key, XChaCha20Poly1305, XNonce,
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};

const VAULT_VERSION: u32 = 1;
const SALT_BYTES: usize = 16;
const NONCE_BYTES: usize = 24;
const KEY_BYTES: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedVault {
    pub version: u32,
    pub salt_hex: String,
    pub nonce_hex: String,
    pub ciphertext_b64: String,
}

pub fn encrypt_json(password: &str, plaintext_json: &[u8]) -> Result<String> {
    if password.len() < 8 {
        bail!("password must be at least 8 characters");
    }

    let mut salt = [0u8; SALT_BYTES];
    let mut nonce = [0u8; NONCE_BYTES];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);

    let key = derive_key(password, &salt)?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));

    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext_json)
        .map_err(|_| anyhow!("vault encryption failed"))?;

    let envelope = EncryptedVault {
        version: VAULT_VERSION,
        salt_hex: hex::encode(salt),
        nonce_hex: hex::encode(nonce),
        ciphertext_b64: B64.encode(ciphertext),
    };

    Ok(serde_json::to_string(&envelope)?)
}

pub fn decrypt_json(password: &str, encrypted_json: &str) -> Result<Vec<u8>> {
    let envelope: EncryptedVault = serde_json::from_str(encrypted_json)?;

    if envelope.version != VAULT_VERSION {
        bail!("unsupported vault version {}", envelope.version);
    }

    let salt = hex::decode(&envelope.salt_hex).context("invalid vault salt hex")?;
    let nonce = hex::decode(&envelope.nonce_hex).context("invalid vault nonce hex")?;
    let ciphertext = B64.decode(envelope.ciphertext_b64.as_bytes())?;

    if salt.len() != SALT_BYTES {
        bail!("invalid vault salt length: {}", salt.len());
    }

    if nonce.len() != NONCE_BYTES {
        bail!("invalid vault nonce length: {}", nonce.len());
    }

    let key = derive_key(password, &salt)?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));

    cipher
        .decrypt(XNonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| anyhow!("wrong password or corrupted vault"))
}

fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; KEY_BYTES]> {
    let params = Params::new(64 * 1024, 3, 1, Some(KEY_BYTES))
        .map_err(|error| anyhow!("invalid argon2 params: {error}"))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; KEY_BYTES];

    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|error| anyhow!("password key derivation failed: {error}"))?;

    Ok(key)
}
