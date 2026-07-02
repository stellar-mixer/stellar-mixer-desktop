use anyhow::{anyhow, bail, Result};
use keyring::Entry;
use schnorrkel::Keypair;
use std::fs;
use std::path::PathBuf;

use crate::config::{APP_SERVICE, VAULT_KEY};
use crate::models::{ArchiveSyncState, VaultPayload};
use crate::security::crypto::{decrypt_json, encrypt_json};
use crate::security::identity::{new_identity_rsa_keypair, new_mixer_identity};

pub fn vault_exists() -> bool {
    read_encrypted_vault().is_ok()
}

pub fn create_vault(password: &str) -> Result<VaultPayload> {
    if vault_exists() {
        bail!("vault already exists");
    }

    let vault = VaultPayload {
        version: 1,
        identity: new_mixer_identity()?,
        accounts: Vec::new(),
        notes: Vec::new(),
        archive: ArchiveSyncState::default(),
    };

    save_vault(password, &vault)?;

    Ok(vault)
}

pub fn unlock_vault(password: &str) -> Result<VaultPayload> {
    let encrypted = read_encrypted_vault()?;
    let plaintext = decrypt_json(password, &encrypted)?;
    let mut vault: VaultPayload = serde_json::from_slice(&plaintext)?;

    if ensure_identity_rsa(&mut vault)? {
        save_vault(password, &vault)?;
    }

    Ok(vault)
}

pub fn save_vault(password: &str, vault: &VaultPayload) -> Result<()> {
    validate_identity(vault)?;

    let plaintext = serde_json::to_vec_pretty(vault)?;
    let encrypted = encrypt_json(password, &plaintext)?;

    let mut keyring_error: Option<String> = None;

    match vault_entry() {
        Ok(entry) => {
            if let Err(error) = entry.set_password(&encrypted) {
                keyring_error = Some(error.to_string());
            }
        }
        Err(error) => {
            keyring_error = Some(error.to_string());
        }
    }

    let path = vault_file_path()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&path, encrypted)?;

    if let Some(error) = keyring_error {
        eprintln!("warning: failed to write vault to OS keyring, encrypted file fallback was written: {error}");
    }

    Ok(())
}

fn read_encrypted_vault() -> Result<String> {
    if let Ok(entry) = vault_entry() {
        if let Ok(value) = entry.get_password() {
            return Ok(value);
        }
    }

    let path = vault_file_path()?;

    fs::read_to_string(&path).map_err(|error| {
        anyhow!(
            "vault not found in OS keyring or encrypted file {}: {error}",
            path.display()
        )
    })
}

fn vault_entry() -> Result<Entry> {
    Entry::new(APP_SERVICE, VAULT_KEY)
        .map_err(|error| anyhow!("failed to open OS keyring entry: {error}"))
}

fn vault_file_path() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| anyhow!("HOME env var is not set; cannot locate encrypted vault fallback"))?;

    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Stellar Mixer")
        .join("vault.enc.json"))
}

fn validate_identity(vault: &VaultPayload) -> Result<()> {
    let keypair_bytes = hex::decode(&vault.identity.schnorrkel_keypair_hex)?;
    let _keypair = Keypair::from_bytes(&keypair_bytes)
        .map_err(|error| anyhow!("invalid schnorrkel identity stored in vault: {error}"))?;

    Ok(())
}

fn ensure_identity_rsa(vault: &mut VaultPayload) -> Result<bool> {
    if vault.identity.rsa_private_key_pkcs1_der_hex.is_some()
        && vault.identity.rsa_public_modulus_hex.is_some()
    {
        return Ok(false);
    }

    let (private_hex, modulus_hex) = new_identity_rsa_keypair()?;

    vault.identity.rsa_private_key_pkcs1_der_hex = Some(private_hex);
    vault.identity.rsa_public_modulus_hex = Some(modulus_hex);

    Ok(true)
}
