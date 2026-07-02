use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use reqwest::Client;
use rsa_risc0::pkcs1::DecodeRsaPrivateKey;
use rsa_risc0::RsaPrivateKey;
use serde::Deserialize;
use std::collections::HashSet;

use crate::config::MIXER_ARCHIVE_URL;
use crate::models::{ArchiveSyncReport, NoteSecret, VaultPayload};
use crate::proofs::encrypted_note::decrypt_encrypted_note;
use crate::proofs::note::{fixed_be_array, note_leaf};

#[derive(Debug)]
pub struct ArchiveSyncDelta {
    pub archive_contract_id: String,
    pub encrypted_note_cursor: u64,
    pub nullifier_cursor: u64,
    pub scanned_encrypted_notes: u64,
    pub scanned_nullifiers: u64,
    pub recovered_notes: Vec<NoteSecret>,
    pub spent_nullifiers: HashSet<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveStateResponse {
    #[serde(alias = "contract_id")]
    contract_id: String,

    #[serde(alias = "encrypted_note_count")]
    encrypted_note_count: u64,

    #[serde(alias = "nullifier_count")]
    nullifier_count: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EncryptedNotesResponse {
    #[serde(alias = "next_index")]
    next_index: u64,

    #[serde(alias = "has_more")]
    has_more: bool,

    items: Vec<ArchiveEncryptedNote>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NullifiersResponse {
    #[serde(alias = "next_index")]
    next_index: u64,

    #[serde(alias = "has_more")]
    has_more: bool,

    items: Vec<ArchiveNullifier>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveEncryptedNote {
    index: u64,

    #[serde(alias = "leaf_hex")]
    leaf_hex: String,

    #[serde(alias = "encrypted_note_base64")]
    encrypted_note_base64: String,

    #[serde(alias = "event_id")]
    event_id: String,

    ledger: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveNullifier {
    #[serde(alias = "nullifier_hex")]
    nullifier_hex: String,
}

pub async fn sync_archive_vault(
    vault: &VaultPayload,
    progress: impl Fn(&str, &str) + Send + Sync,
) -> Result<ArchiveSyncDelta> {
    let client = Client::new();
    let state = fetch_archive_state(&client).await?;

    progress(
        "archive",
        &format!(
            "archive state: encrypted_notes={}, nullifiers={}",
            state.encrypted_note_count, state.nullifier_count
        ),
    );

    let same_contract = vault.archive.contract_id.as_deref() == Some(state.contract_id.as_str());

    let encrypted_start = if same_contract {
        vault
            .archive
            .encrypted_note_cursor
            .min(state.encrypted_note_count)
    } else {
        0
    };

    let nullifier_start = if same_contract {
        vault.archive.nullifier_cursor.min(state.nullifier_count)
    } else {
        0
    };

    let private_key = identity_private_key(vault)?;
    let owner_modulus = identity_owner_modulus(vault)?;
    let owner_modulus_hex = hex::encode(owner_modulus);

    let mut known_leafs: HashSet<String> = vault
        .notes
        .iter()
        .map(|note| note.leaf_hex.to_ascii_lowercase())
        .collect();

    let mut known_nullifiers: HashSet<String> = vault
        .notes
        .iter()
        .map(|note| note.nullifier_hex.to_ascii_lowercase())
        .collect();

    let mut recovered_notes = Vec::new();
    let mut encrypted_cursor = encrypted_start;
    let mut scanned_encrypted_notes = 0u64;

    loop {
        if encrypted_cursor >= state.encrypted_note_count {
            break;
        }

        let batch = fetch_encrypted_notes(&client, encrypted_cursor).await?;

        if batch.items.is_empty() {
            encrypted_cursor = batch.next_index;
            break;
        }

        progress(
            "archive",
            &format!(
                "processing encrypted notes {}..{}",
                encrypted_cursor, batch.next_index
            ),
        );

        for item in batch.items {
            scanned_encrypted_notes = scanned_encrypted_notes.saturating_add(1);

            let ciphertext = match B64.decode(item.encrypted_note_base64.as_bytes()) {
                Ok(value) => value,
                Err(_) => continue,
            };

            let decoded = match decrypt_encrypted_note(&private_key, &ciphertext) {
                Ok(value) => value,
                Err(_) => continue,
            };

            if decoded.value == 0 {
                continue;
            }

            let runtime_note = decoded.to_runtime_note(owner_modulus);
            let computed_leaf = note_leaf(&runtime_note);
            let computed_leaf_hex = hex::encode(computed_leaf);
            let event_leaf_hex = item.leaf_hex.to_ascii_lowercase();

            if computed_leaf_hex != event_leaf_hex {
                bail!(
                    "archive encrypted note decrypted for this identity but leaf mismatch at index {}",
                    item.index
                );
            }

            let nullifier_hex = hex::encode(runtime_note.nullifier);

            if known_leafs.contains(&computed_leaf_hex) || known_nullifiers.contains(&nullifier_hex)
            {
                continue;
            }

            let account_id = decoded
                .account_hint
                .as_ref()
                .filter(|hint| vault.accounts.iter().any(|account| account.id == **hint))
                .cloned()
                .unwrap_or_else(|| "archive-recovered".to_string());

            let note = NoteSecret {
                id: uuid::Uuid::new_v4().to_string(),
                account_id,
                value: decoded.value,
                owner_modulus_hex: owner_modulus_hex.clone(),
                nullifier_hex: nullifier_hex.clone(),
                secret_hex: hex::encode(runtime_note.secret),
                leaf_hex: computed_leaf_hex.clone(),
                leaf_index: Some(item.index),
                spent: false,
                created_at: chrono::Utc::now().timestamp_millis(),
                spent_at: None,
                source_event_id: Some(item.event_id),
                source_ledger: Some(item.ledger),
                source_kind: Some(format!("{:?}", decoded.kind)),
                message: decoded.memo,
            };

            known_leafs.insert(computed_leaf_hex);
            known_nullifiers.insert(nullifier_hex);
            recovered_notes.push(note);
        }

        encrypted_cursor = batch.next_index;

        if !batch.has_more {
            break;
        }
    }

    let mut spent_nullifiers = HashSet::new();
    let mut nullifier_cursor = nullifier_start;
    let mut scanned_nullifiers = 0u64;

    loop {
        if nullifier_cursor >= state.nullifier_count {
            break;
        }

        let batch = fetch_nullifiers(&client, nullifier_cursor).await?;

        if batch.items.is_empty() {
            nullifier_cursor = batch.next_index;
            break;
        }

        progress(
            "archive",
            &format!(
                "processing nullifiers {}..{}",
                nullifier_cursor, batch.next_index
            ),
        );

        for item in batch.items {
            scanned_nullifiers = scanned_nullifiers.saturating_add(1);
            spent_nullifiers.insert(item.nullifier_hex.to_ascii_lowercase());
        }

        nullifier_cursor = batch.next_index;

        if !batch.has_more {
            break;
        }
    }

    Ok(ArchiveSyncDelta {
        archive_contract_id: state.contract_id,
        encrypted_note_cursor: encrypted_cursor,
        nullifier_cursor,
        scanned_encrypted_notes,
        scanned_nullifiers,
        recovered_notes,
        spent_nullifiers,
    })
}

pub fn empty_report(vault: &VaultPayload) -> ArchiveSyncReport {
    ArchiveSyncReport {
        archive_contract_id: vault.archive.contract_id.clone(),
        encrypted_note_cursor: vault.archive.encrypted_note_cursor,
        nullifier_cursor: vault.archive.nullifier_cursor,
        scanned_encrypted_notes: 0,
        scanned_nullifiers: 0,
        imported_note_count: 0,
        spent_note_count: 0,
    }
}

async fn fetch_archive_state(client: &Client) -> Result<ArchiveStateResponse> {
    let url = format!("{}/v1/state", MIXER_ARCHIVE_URL.trim_end_matches('/'));

    client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("failed to request mixer archive state from {url}"))?
        .error_for_status()
        .with_context(|| format!("mixer archive state request failed for {url}"))?
        .json::<ArchiveStateResponse>()
        .await
        .context("failed to decode mixer archive state")
}

async fn fetch_encrypted_notes(client: &Client, index: u64) -> Result<EncryptedNotesResponse> {
    let url = format!(
        "{}/v1/encrypted-notes?index={}",
        MIXER_ARCHIVE_URL.trim_end_matches('/'),
        index
    );

    client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("failed to request encrypted notes from {url}"))?
        .error_for_status()
        .with_context(|| format!("encrypted notes request failed for {url}"))?
        .json::<EncryptedNotesResponse>()
        .await
        .context("failed to decode encrypted notes response")
}

async fn fetch_nullifiers(client: &Client, index: u64) -> Result<NullifiersResponse> {
    let url = format!(
        "{}/v1/nullifiers?index={}",
        MIXER_ARCHIVE_URL.trim_end_matches('/'),
        index
    );

    client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("failed to request nullifiers from {url}"))?
        .error_for_status()
        .with_context(|| format!("nullifiers request failed for {url}"))?
        .json::<NullifiersResponse>()
        .await
        .context("failed to decode nullifiers response")
}

fn identity_private_key(vault: &VaultPayload) -> Result<RsaPrivateKey> {
    let private_hex = vault
        .identity
        .rsa_private_key_pkcs1_der_hex
        .as_ref()
        .ok_or_else(|| anyhow!("Mixer Identity RSA private key is missing"))?;

    let der = hex::decode(private_hex).context("invalid Mixer Identity RSA private key hex")?;

    RsaPrivateKey::from_pkcs1_der(&der).context("failed to decode Mixer Identity RSA private key")
}

fn identity_owner_modulus(vault: &VaultPayload) -> Result<[u8; 256]> {
    let modulus_hex = vault
        .identity
        .rsa_public_modulus_hex
        .as_ref()
        .ok_or_else(|| anyhow!("Mixer Identity RSA public modulus is missing"))?;

    fixed_be_array(&hex::decode(modulus_hex)?)
}
