use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rsa_risc0::pkcs1::DecodeRsaPrivateKey;
use rsa_risc0::traits::PublicKeyParts;
use rsa_risc0::{RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

use crate::config::{MIXER_ARCHIVE_URL, TREEPIR_URL};
use crate::error::CommandResult;
use crate::mixer::archive::{empty_report, sync_archive_vault, ArchiveSyncDelta};
use crate::mixer::coin_selection::{
    format_xlm, spendable_note_count, total_spendable_balance, total_unspent_balance,
    unspent_note_count, MAX_WITHDRAW_INPUTS,
};
use crate::mixer::flow::{deposit_for_account, transfer_for_identity, withdraw_for_identity};
use crate::models::{
    ArchiveSyncReport, ArchiveSyncState, BackupExportResult, BackupImportResult, DepositResult,
    MixerStats, NoteSecret, NoteView, PrivateBalance, ProgressEvent, PublicAccount, TransferResult,
    UnlockResult, VaultPayload, VaultStatus, WithdrawResult,
};
use crate::proofs::note::fixed_be_array;
use crate::security::crypto::{decrypt_json, encrypt_json};
use crate::security::identity::{
    format_bip39_phrase_for_display, mixer_identity_from_bip39_phrase, new_mixer_identity,
    RECOVERY_KIND_LEGACY_SMXREC1,
};
use crate::security::vault::{
    create_vault, save_vault, unlock_vault as unlock_vault_store, vault_exists,
};
use crate::state::{AppState, Session};
use crate::stellar::account::create_stellar_account_secret;

#[derive(Debug, Deserialize)]
struct TreePirReadyResponse {
    ready: bool,
    leaf_count: usize,
    root_hex: Option<String>,
}

#[tauri::command]
pub async fn vault_status(state: State<'_, AppState>) -> CommandResult<VaultStatus> {
    let unlocked = state
        .session
        .lock()
        .expect("session mutex poisoned")
        .is_some();

    Ok(VaultStatus {
        exists: vault_exists(),
        unlocked,
    })
}

#[tauri::command]
pub async fn setup_vault(
    password: String,
    state: State<'_, AppState>,
) -> CommandResult<UnlockResult> {
    let mut vault = create_vault(&password)?;

    if let Err(error) = initialize_new_identity_archive_cursors(&mut vault).await {
        eprintln!("warning: failed to initialize archive cursors for new identity: {error:#}");
    } else {
        save_vault(&password, &vault)?;
    }

    let mut result = unlock_result(&vault);
    result.recovery_phrase = vault
        .identity
        .recovery_phrase
        .as_deref()
        .map(format_bip39_phrase_for_display);

    *state.session.lock().expect("session mutex poisoned") = Some(Session { password, vault });

    Ok(result)
}

#[tauri::command]
pub async fn unlock_vault(
    password: String,
    state: State<'_, AppState>,
) -> CommandResult<UnlockResult> {
    let vault = unlock_vault_store(&password)?;
    let result = unlock_result(&vault);

    *state.session.lock().expect("session mutex poisoned") = Some(Session { password, vault });

    Ok(result)
}

#[tauri::command]
pub async fn setup_vault_from_recovery_phrase(
    password: String,
    recovery_phrase: String,
    state: State<'_, AppState>,
) -> CommandResult<UnlockResult> {
    if vault_exists() {
        return Err(anyhow!(
            "active Mixer Identity already exists; delete/reset the local vault before restoring from recovery phrase"
        )
        .into());
    }

    if password.len() < 8 {
        return Err(anyhow!("password must be at least 8 characters").into());
    }

    let identity = if recovery_phrase
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .starts_with("smxrec1")
    {
        let payload = parse_identity_recovery_phrase(&recovery_phrase)?;
        validate_identity_recovery_payload(&payload)?;

        let mut identity = new_mixer_identity()?;
        identity.rsa_private_key_pkcs1_der_hex = Some(payload.rsa_private_key_pkcs1_der_hex);
        identity.rsa_public_modulus_hex = Some(payload.rsa_public_modulus_hex);
        identity.recovery_kind = Some(RECOVERY_KIND_LEGACY_SMXREC1.to_string());
        identity.recovery_phrase = None;
        identity.created_at = payload
            .created_at
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

        identity
    } else {
        mixer_identity_from_bip39_phrase(&recovery_phrase)?
    };

    let vault = VaultPayload {
        version: 1,
        identity,
        accounts: Vec::new(),
        notes: Vec::new(),
        archive: ArchiveSyncState::default(),
    };

    save_vault(&password, &vault)?;

    let result = unlock_result(&vault);

    *state.session.lock().expect("session mutex poisoned") = Some(Session { password, vault });

    Ok(result)
}

#[tauri::command]
pub async fn export_identity_recovery_phrase(password: String) -> CommandResult<String> {
    let vault = unlock_vault_store(&password)?;
    identity_recovery_phrase_from_vault(&vault).map_err(Into::into)
}

#[tauri::command]
pub async fn export_mixer_backup(
    password: String,
    ui_state_json: String,
) -> CommandResult<BackupExportResult> {
    let vault = unlock_vault_store(&password)?;

    let payload = MixerBackupPayload {
        version: 1,
        exported_at: chrono::Utc::now().timestamp_millis(),
        vault,
        ui_state_json: if ui_state_json.trim().is_empty() {
            None
        } else {
            Some(ui_state_json)
        },
    };

    let payload_json =
        serde_json::to_vec_pretty(&payload).context("failed to encode mixer backup payload")?;
    let encrypted_payload = encrypt_json(&password, &payload_json)?;

    let envelope = MixerBackupEnvelope {
        format: "stellar-mixer-backup-v1".to_string(),
        exported_at: chrono::Utc::now().timestamp_millis(),
        encrypted_payload,
    };

    let path = downloads_backup_path()?;
    let envelope_json =
        serde_json::to_vec_pretty(&envelope).context("failed to encode mixer backup envelope")?;

    fs::write(&path, envelope_json)
        .with_context(|| format!("failed to write mixer backup to {}", path.display()))?;

    Ok(BackupExportResult {
        path: path.display().to_string(),
    })
}

#[tauri::command]
pub async fn import_mixer_backup(
    password: String,
    backup_json: String,
    state: State<'_, AppState>,
) -> CommandResult<BackupImportResult> {
    if vault_exists() {
        return Err(anyhow!(
            "active Mixer Identity already exists; import is available only when there is no local vault"
        )
        .into());
    }

    let envelope: MixerBackupEnvelope =
        serde_json::from_str(&backup_json).context("failed to parse Stellar Mixer backup file")?;

    if envelope.format != "stellar-mixer-backup-v1" {
        return Err(anyhow!("unsupported backup format: {}", envelope.format).into());
    }

    let payload_plaintext = decrypt_json(&password, &envelope.encrypted_payload)
        .context("failed to decrypt backup; wrong password or corrupted backup")?;

    let payload: MixerBackupPayload = serde_json::from_slice(&payload_plaintext)
        .context("failed to decode decrypted backup payload")?;

    if payload.version != 1 {
        return Err(anyhow!("unsupported backup payload version: {}", payload.version).into());
    }

    save_vault(&password, &payload.vault)?;

    let result = unlock_result(&payload.vault);

    *state.session.lock().expect("session mutex poisoned") = Some(Session {
        password,
        vault: payload.vault,
    });

    Ok(BackupImportResult {
        identity_public_key: result.identity_public_key,
        accounts: result.accounts,
        note_count: result.note_count,
        ui_state_json: payload.ui_state_json,
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MixerBackupEnvelope {
    format: String,
    exported_at: i64,
    encrypted_payload: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MixerBackupPayload {
    version: u32,
    exported_at: i64,
    vault: VaultPayload,
    ui_state_json: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityRecoveryPhrasePayload {
    version: u32,
    rsa_private_key_pkcs1_der_hex: String,
    rsa_public_modulus_hex: String,
    created_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveStateForSetup {
    #[serde(alias = "contract_id")]
    contract_id: String,

    #[serde(alias = "encrypted_note_count")]
    encrypted_note_count: u64,

    #[serde(alias = "nullifier_count")]
    nullifier_count: u64,
}

async fn initialize_new_identity_archive_cursors(vault: &mut VaultPayload) -> Result<()> {
    let url = format!("{}/v1/state", MIXER_ARCHIVE_URL.trim_end_matches('/'));

    let state = reqwest::get(&url)
        .await
        .with_context(|| format!("failed to request mixer archive state from {url}"))?
        .error_for_status()
        .with_context(|| format!("mixer archive state request failed for {url}"))?
        .json::<ArchiveStateForSetup>()
        .await
        .context("failed to decode mixer archive state")?;

    vault.archive.contract_id = Some(state.contract_id);
    vault.archive.encrypted_note_cursor = state.encrypted_note_count;
    vault.archive.nullifier_cursor = state.nullifier_count;
    vault.archive.last_synced_at = Some(chrono::Utc::now().timestamp_millis());

    Ok(())
}

fn identity_recovery_phrase_from_vault(vault: &VaultPayload) -> Result<String> {
    if let Some(phrase) = vault.identity.recovery_phrase.as_deref() {
        return Ok(format_bip39_phrase_for_display(phrase));
    }

    let rsa_private_key_pkcs1_der_hex = vault
        .identity
        .rsa_private_key_pkcs1_der_hex
        .clone()
        .ok_or_else(|| anyhow!("Mixer Identity RSA private key is missing"))?;

    let rsa_public_modulus_hex = vault
        .identity
        .rsa_public_modulus_hex
        .clone()
        .ok_or_else(|| anyhow!("Mixer Identity RSA public modulus is missing"))?;

    let payload = IdentityRecoveryPhrasePayload {
        version: 1,
        rsa_private_key_pkcs1_der_hex,
        rsa_public_modulus_hex,
        created_at: Some(vault.identity.created_at),
    };

    let bytes = serde_json::to_vec(&payload)?;
    let encoded = URL_SAFE_NO_PAD.encode(bytes);

    Ok(format_pretty_recovery_phrase(&encoded))
}

fn format_pretty_recovery_phrase(encoded: &str) -> String {
    let mut out = String::from("smxrec1:\n");

    for (line_index, line) in encoded.as_bytes().chunks(64).enumerate() {
        if line_index > 0 {
            out.push('\n');
        }

        for (group_index, group) in line.chunks(16).enumerate() {
            if group_index > 0 {
                out.push(' ');
            }

            out.push_str(std::str::from_utf8(group).expect("base64url is utf8"));
        }
    }

    out
}

fn parse_identity_recovery_phrase(input: &str) -> Result<IdentityRecoveryPhrasePayload> {
    let compact: String = input.chars().filter(|c| !c.is_whitespace()).collect();

    let raw = compact
        .strip_prefix("smxrec1:")
        .or_else(|| compact.strip_prefix("smxrec1"))
        .ok_or_else(|| anyhow!("recovery phrase must start with smxrec1:"))?;

    let bytes = URL_SAFE_NO_PAD
        .decode(raw.as_bytes())
        .context("failed to decode recovery phrase")?;

    let payload: IdentityRecoveryPhrasePayload =
        serde_json::from_slice(&bytes).context("failed to parse recovery phrase payload")?;

    if payload.version != 1 {
        bail!("unsupported recovery phrase version: {}", payload.version);
    }

    Ok(payload)
}

fn validate_identity_recovery_payload(payload: &IdentityRecoveryPhrasePayload) -> Result<()> {
    if payload.rsa_public_modulus_hex.len() != 512 {
        bail!("invalid RSA public modulus length in recovery phrase");
    }

    if !payload
        .rsa_public_modulus_hex
        .chars()
        .all(|c| c.is_ascii_hexdigit())
    {
        bail!("RSA public modulus in recovery phrase is not hex");
    }

    let der = hex::decode(&payload.rsa_private_key_pkcs1_der_hex)
        .context("invalid RSA private key hex in recovery phrase")?;

    let private_key = RsaPrivateKey::from_pkcs1_der(&der)
        .context("failed to decode RSA private key from recovery phrase")?;

    let public_key = RsaPublicKey::from(&private_key);
    let modulus: [u8; 256] = fixed_be_array(&public_key.n().to_bytes_be())?;
    let derived_modulus_hex = hex::encode(modulus);

    if !derived_modulus_hex.eq_ignore_ascii_case(&payload.rsa_public_modulus_hex) {
        bail!("recovery phrase RSA private key does not match public modulus");
    }

    Ok(())
}

fn downloads_backup_path() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| anyhow!("HOME env var is not set; cannot locate Downloads folder"))?;

    let filename = format!(
        "stellar-mixer-backup-{}.json",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    );

    Ok(PathBuf::from(home).join("Downloads").join(filename))
}

#[tauri::command]
pub async fn lock_vault(state: State<'_, AppState>) -> CommandResult<()> {
    *state.session.lock().expect("session mutex poisoned") = None;
    Ok(())
}

#[tauri::command]
pub async fn create_stellar_account(
    name: String,
    state: State<'_, AppState>,
) -> CommandResult<PublicAccount> {
    let mut guard = state.session.lock().expect("session mutex poisoned");
    let session = guard.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;

    let account = create_stellar_account_secret(name)?;
    let public = PublicAccount::from(&account);

    session.vault.accounts.push(account);
    save_vault(&session.password, &session.vault)?;

    Ok(public)
}

#[tauri::command]
pub async fn copy_stellar_secret_key(
    account_id: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let secret_key = {
        let guard = state.session.lock().expect("session mutex poisoned");
        let session = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;

        session
            .vault
            .accounts
            .iter()
            .find(|account| account.id == account_id)
            .map(|account| account.stellar_secret_key.clone())
            .ok_or_else(|| anyhow!("account not found"))?
    };

    let mut child = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| anyhow!("failed to start pbcopy: {error}"))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("failed to open pbcopy stdin"))?;

        std::io::Write::write_all(stdin, secret_key.as_bytes())
            .map_err(|error| anyhow!("failed to write secret key to clipboard: {error}"))?;
    }

    let status = child
        .wait()
        .map_err(|error| anyhow!("failed to wait for pbcopy: {error}"))?;

    if !status.success() {
        return Err(anyhow!("pbcopy failed with status: {status}").into());
    }

    Ok(())
}

#[tauri::command]
pub async fn export_stellar_secret_key(
    account_id: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let guard = state.session.lock().expect("session mutex poisoned");
    let session = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;

    let account = session
        .vault
        .accounts
        .iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| anyhow!("account not found"))?;

    Ok(account.stellar_secret_key.clone())
}

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> CommandResult<Vec<PublicAccount>> {
    let guard = state.session.lock().expect("session mutex poisoned");
    let session = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;

    Ok(session.vault.public_accounts())
}

#[tauri::command]
pub async fn identity_address(state: State<'_, AppState>) -> CommandResult<String> {
    let guard = state.session.lock().expect("session mutex poisoned");
    let session = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;

    let modulus = session
        .vault
        .identity
        .rsa_public_modulus_hex
        .as_ref()
        .ok_or_else(|| {
            anyhow!("Mixer Identity RSA public modulus is missing; lock/unlock vault to migrate it")
        })?;

    Ok(format!("smxid1:{modulus}"))
}

#[tauri::command]
pub async fn private_balance(state: State<'_, AppState>) -> CommandResult<PrivateBalance> {
    let guard = state.session.lock().expect("session mutex poisoned");
    let session = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;

    Ok(private_balance_from_notes(&session.vault.notes))
}

#[tauri::command]
pub async fn notes_summary(state: State<'_, AppState>) -> CommandResult<Vec<NoteView>> {
    let guard = state.session.lock().expect("session mutex poisoned");
    let session = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;

    let mut notes: Vec<NoteView> = session
        .vault
        .notes
        .iter()
        .map(|note| {
            let account = session
                .vault
                .accounts
                .iter()
                .find(|account| account.id == note.account_id);

            NoteView {
                id: note.id.clone(),
                deposited_by_account_id: note.account_id.clone(),
                deposited_by_account_name: account
                    .map(|account| account.name.clone())
                    .unwrap_or_else(|| {
                        if note.account_id == "archive-recovered" {
                            "Recovered from Mixer Archive".to_string()
                        } else {
                            "Unknown account".to_string()
                        }
                    }),
                deposited_by_public_key: account
                    .map(|account| account.stellar_public_key.clone())
                    .unwrap_or_else(|| "unknown".to_string()),
                amount_stroops: note.value.to_string(),
                amount_xlm: format_xlm(note.value),
                leaf_index: note.leaf_index,
                status: if note.spent {
                    "spent".to_string()
                } else if note.leaf_index.is_some() {
                    "spendable".to_string()
                } else {
                    "pending-index".to_string()
                },
                created_at: note.created_at,
                spent_at: note.spent_at,
                source_kind: note.source_kind.clone(),
                source_ledger: note.source_ledger,
                message: note.message.clone(),
            }
        })
        .collect();

    notes.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(notes)
}

fn note_view_from_secret(vault: &VaultPayload, note: &NoteSecret) -> NoteView {
    let account = vault
        .accounts
        .iter()
        .find(|account| account.id == note.account_id);

    NoteView {
        id: note.id.clone(),
        deposited_by_account_id: note.account_id.clone(),
        deposited_by_account_name: account
            .map(|account| account.name.clone())
            .unwrap_or_else(|| {
                if note.account_id == "archive-recovered" {
                    "Recovered from Mixer Archive".to_string()
                } else {
                    "Unknown account".to_string()
                }
            }),
        deposited_by_public_key: account
            .map(|account| account.stellar_public_key.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        amount_stroops: note.value.to_string(),
        amount_xlm: format_xlm(note.value),
        leaf_index: note.leaf_index,
        status: if note.spent {
            "spent".to_string()
        } else if note.leaf_index.is_some() {
            "spendable".to_string()
        } else {
            "pending-index".to_string()
        },
        created_at: note.created_at,
        spent_at: note.spent_at,
        source_kind: note.source_kind.clone(),
        source_ledger: note.source_ledger,
        message: note.message.clone(),
    }
}

#[tauri::command]
pub async fn treepir_status() -> CommandResult<MixerStats> {
    let url = format!("{}/ready", TREEPIR_URL.trim_end_matches('/'));

    let ready = reqwest::get(&url)
        .await
        .with_context(|| format!("failed to request TreePIR status from {url}"))?
        .error_for_status()
        .with_context(|| format!("TreePIR status request failed for {url}"))?
        .json::<TreePirReadyResponse>()
        .await
        .context("failed to decode TreePIR /ready response")?;

    Ok(MixerStats {
        anonymity_set: ready.leaf_count,
        root_hex: ready.root_hex,
        ready: ready.ready,
    })
}

#[tauri::command]
pub async fn sync_archive(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<ArchiveSyncReport> {
    let op_id = uuid::Uuid::new_v4().to_string();

    emit_progress(&app, &op_id, None, "archive", "starting mixer archive sync")?;

    let vault_snapshot = {
        let guard = state.session.lock().expect("session mutex poisoned");
        let session = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;
        session.vault.clone()
    };

    let app_for_progress = app.clone();
    let op_for_progress = op_id.clone();

    let delta = match sync_archive_vault(&vault_snapshot, move |step, message| {
        let _ = emit_progress(&app_for_progress, &op_for_progress, None, step, message);
    })
    .await
    {
        Ok(delta) => delta,
        Err(error) => {
            emit_progress(
                &app,
                &op_id,
                None,
                "archive",
                &format!("mixer archive sync skipped/failed: {error:#}"),
            )?;

            return Ok(empty_report(&vault_snapshot));
        }
    };

    let report = {
        let mut guard = state.session.lock().expect("session mutex poisoned");
        let session = guard.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;

        let report = merge_archive_delta(&mut session.vault, delta);
        save_vault(&session.password, &session.vault)?;

        report
    };

    emit_progress(
        &app,
        &op_id,
        None,
        "archive",
        &format!(
            "archive sync complete: imported {}, marked spent {}",
            report.imported_note_count, report.spent_note_count
        ),
    )?;

    Ok(report)
}

fn merge_archive_delta(
    vault: &mut crate::models::VaultPayload,
    delta: ArchiveSyncDelta,
) -> ArchiveSyncReport {
    let mut imported_note_count = 0usize;
    let mut received_transfer_notes = Vec::new();

    for mut incoming in delta.recovered_notes {
        if let Some(existing) = vault.notes.iter_mut().find(|note| {
            note.leaf_hex.eq_ignore_ascii_case(&incoming.leaf_hex)
                || note
                    .nullifier_hex
                    .eq_ignore_ascii_case(&incoming.nullifier_hex)
        }) {
            if existing.leaf_index.is_none() {
                existing.leaf_index = incoming.leaf_index;
            }

            if existing.source_event_id.is_none() {
                existing.source_event_id = incoming.source_event_id.take();
            }

            if existing.source_ledger.is_none() {
                existing.source_ledger = incoming.source_ledger;
            }

            if existing.source_kind.is_none() {
                existing.source_kind = incoming.source_kind.take();
            }

            if existing.message.is_none() {
                existing.message = incoming.message.take();
            }

            continue;
        }

        if incoming.source_kind.as_deref() == Some("TransferReceived") {
            received_transfer_notes.push(note_view_from_secret(vault, &incoming));
        }

        vault.notes.push(incoming);
        imported_note_count += 1;
    }

    let now = chrono::Utc::now().timestamp_millis();
    let mut spent_note_count = 0usize;

    for note in vault.notes.iter_mut() {
        if !note.spent
            && delta
                .spent_nullifiers
                .contains(&note.nullifier_hex.to_ascii_lowercase())
        {
            note.spent = true;
            note.spent_at = Some(now);
            spent_note_count += 1;
        }
    }

    vault.archive.contract_id = Some(delta.archive_contract_id.clone());
    vault.archive.encrypted_note_cursor = vault
        .archive
        .encrypted_note_cursor
        .max(delta.encrypted_note_cursor);
    vault.archive.nullifier_cursor = vault.archive.nullifier_cursor.max(delta.nullifier_cursor);
    vault.archive.last_synced_at = Some(now);

    ArchiveSyncReport {
        archive_contract_id: Some(delta.archive_contract_id),
        encrypted_note_cursor: vault.archive.encrypted_note_cursor,
        nullifier_cursor: vault.archive.nullifier_cursor,
        scanned_encrypted_notes: delta.scanned_encrypted_notes,
        scanned_nullifiers: delta.scanned_nullifiers,
        imported_note_count,
        spent_note_count,
        received_transfer_notes,
    }
}

#[tauri::command]
pub async fn deposit(
    app: AppHandle,
    account_id: String,
    amount: String,
    state: State<'_, AppState>,
) -> CommandResult<DepositResult> {
    let amount = parse_xlm_amount_to_stroops(&amount)?;

    let op_id = uuid::Uuid::new_v4().to_string();

    emit_progress(
        &app,
        &op_id,
        Some(&account_id),
        "deposit",
        "checking source account and creating identity-wide private note",
    )?;

    let (account, identity, password) = {
        let guard = state.session.lock().expect("session mutex poisoned");
        let session = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;

        let account = session
            .vault
            .accounts
            .iter()
            .find(|account| account.id == account_id)
            .cloned()
            .ok_or_else(|| anyhow!("account not found"))?;

        (
            account,
            session.vault.identity.clone(),
            session.password.clone(),
        )
    };

    let (result, note) = deposit_for_account(&account, &identity, amount).await?;

    emit_progress(
        &app,
        &op_id,
        Some(&account_id),
        "deposit",
        "deposit tx succeeded; storing note in Mixer Identity encrypted vault",
    )?;

    let mut guard = state.session.lock().expect("session mutex poisoned");
    let session = guard.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;

    session.vault.notes.push(note);
    save_vault(&password, &session.vault)?;

    emit_progress(
        &app,
        &op_id,
        Some(&account_id),
        "deposit",
        "identity-wide private note saved",
    )?;

    Ok(result)
}

#[tauri::command]
pub async fn withdraw(
    app: AppHandle,
    account_id: String,
    amount: String,
    fee_payer_account_id: Option<String>,
    recipient_address: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<WithdrawResult> {
    let amount = parse_xlm_amount_to_stroops(&amount)?;
    let _withdraw_operation_guard = state.start_withdraw()?;

    let fee_payer_id = fee_payer_account_id
        .clone()
        .unwrap_or_else(|| account_id.clone());

    let op_id = uuid::Uuid::new_v4().to_string();

    emit_progress(
        &app,
        &op_id,
        Some(&account_id),
        "withdraw",
        "selected account is recipient; fee payer can be selected separately",
    )?;

    let (recipient_account, fee_payer_account, vault, password) = {
        let guard = state.session.lock().expect("session mutex poisoned");
        let session = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;

        let recipient = session
            .vault
            .accounts
            .iter()
            .find(|account| account.id == account_id)
            .cloned()
            .ok_or_else(|| anyhow!("recipient account not found"))?;

        let fee_payer = session
            .vault
            .accounts
            .iter()
            .find(|account| account.id == fee_payer_id)
            .cloned()
            .ok_or_else(|| anyhow!("fee payer account not found"))?;

        (
            recipient,
            fee_payer,
            session.vault.clone(),
            session.password.clone(),
        )
    };

    let app_for_progress = app.clone();
    let op_for_progress = op_id.clone();
    let account_for_progress = account_id.clone();

    let (result, updated_notes) = withdraw_for_identity(
        &recipient_account,
        &fee_payer_account,
        &vault,
        amount,
        recipient_address.clone(),
        move |step, message| {
            let _ = emit_progress(
                &app_for_progress,
                &op_for_progress,
                Some(&account_for_progress),
                step,
                message,
            );
        },
    )
    .await?;

    emit_progress(
        &app,
        &op_id,
        Some(&account_id),
        "withdraw",
        "withdraw tx succeeded; updating identity-wide encrypted notes",
    )?;

    let mut guard = state.session.lock().expect("session mutex poisoned");
    let session = guard.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;

    for updated in updated_notes {
        if let Some(existing) = session
            .vault
            .notes
            .iter_mut()
            .find(|note| note.id == updated.id)
        {
            *existing = updated;
        } else {
            session.vault.notes.push(updated);
        }
    }

    save_vault(&password, &session.vault)?;

    emit_progress(
        &app,
        &op_id,
        Some(&account_id),
        "withdraw",
        "identity-wide private note pool updated",
    )?;

    Ok(result)
}

#[tauri::command]
pub async fn transfer(
    app: AppHandle,
    account_id: String,
    amount: String,
    recipient_identity: String,
    message: Option<String>,
    fee_payer_account_id: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<TransferResult> {
    let amount = parse_xlm_amount_to_stroops(&amount)?;
    let _spend_guard = state.start_transfer()?;

    let fee_payer_id = fee_payer_account_id
        .clone()
        .unwrap_or_else(|| account_id.clone());

    let op_id = uuid::Uuid::new_v4().to_string();

    emit_progress(
        &app,
        &op_id,
        Some(&account_id),
        "transfer",
        "starting private transfer to recipient Mixer Identity",
    )?;

    let (fee_payer, vault, password) = {
        let guard = state.session.lock().expect("session mutex poisoned");
        let session = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;

        let fee_payer = session
            .vault
            .accounts
            .iter()
            .find(|account| account.id == fee_payer_id)
            .cloned()
            .ok_or_else(|| anyhow!("fee payer account not found"))?;

        (fee_payer, session.vault.clone(), session.password.clone())
    };

    let app_for_progress = app.clone();
    let op_for_progress = op_id.clone();
    let account_for_progress = account_id.clone();

    let (result, updated_notes) = transfer_for_identity(
        &fee_payer,
        &vault,
        amount,
        &recipient_identity,
        message,
        move |step, message| {
            let _ = emit_progress(
                &app_for_progress,
                &op_for_progress,
                Some(&account_for_progress),
                step,
                message,
            );
        },
    )
    .await?;

    emit_progress(
        &app,
        &op_id,
        Some(&account_id),
        "transfer",
        "transfer tx succeeded; updating identity-wide encrypted notes",
    )?;

    let mut guard = state.session.lock().expect("session mutex poisoned");
    let session = guard.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;

    for updated in updated_notes {
        if let Some(existing) = session
            .vault
            .notes
            .iter_mut()
            .find(|note| note.id == updated.id)
        {
            *existing = updated;
        } else {
            session.vault.notes.push(updated);
        }
    }

    save_vault(&password, &session.vault)?;

    emit_progress(
        &app,
        &op_id,
        Some(&account_id),
        "transfer",
        "identity-wide private note pool updated",
    )?;

    Ok(result)
}

fn unlock_result(vault: &crate::models::VaultPayload) -> UnlockResult {
    UnlockResult {
        identity_public_key: vault.identity.schnorrkel_public_hex.clone(),
        accounts: vault.public_accounts(),
        note_count: vault.notes.len(),
        recovery_phrase: None,
    }
}

fn private_balance_from_notes(notes: &[crate::models::NoteSecret]) -> PrivateBalance {
    let total = total_unspent_balance(notes);
    let spendable = total_spendable_balance(notes);
    let pending = total.saturating_sub(spendable);

    PrivateBalance {
        total_stroops: total.to_string(),
        total_xlm: format_xlm(total),
        spendable_stroops: spendable.to_string(),
        spendable_xlm: format_xlm(spendable),
        pending_stroops: pending.to_string(),
        pending_xlm: format_xlm(pending),
        unspent_note_count: unspent_note_count(notes),
        spendable_note_count: spendable_note_count(notes),
        max_inputs: MAX_WITHDRAW_INPUTS,
    }
}

fn parse_xlm_amount_to_stroops(input: &str) -> Result<u128> {
    let value = input.trim().replace(',', ".");

    if value.is_empty() {
        bail!("amount is empty");
    }

    if value.starts_with('-') {
        bail!("amount must be positive");
    }

    let parts: Vec<&str> = value.split('.').collect();

    if parts.len() > 2 {
        bail!("invalid XLM amount format");
    }

    let whole = if parts[0].is_empty() {
        0u128
    } else {
        parts[0].parse::<u128>().context("invalid XLM amount")?
    };

    let frac_raw = parts.get(1).copied().unwrap_or("");

    if frac_raw.len() > 7 {
        bail!("XLM amount supports max 7 decimal places");
    }

    if !frac_raw.chars().all(|c| c.is_ascii_digit()) {
        bail!("invalid XLM amount");
    }

    let mut frac = frac_raw.to_string();

    while frac.len() < 7 {
        frac.push('0');
    }

    let frac_stroops = if frac.is_empty() {
        0u128
    } else {
        frac.parse::<u128>()
            .context("invalid XLM fractional amount")?
    };

    let stroops = whole
        .checked_mul(10_000_000)
        .and_then(|value| value.checked_add(frac_stroops))
        .ok_or_else(|| anyhow!("XLM amount is too large"))?;

    if stroops == 0 {
        bail!("amount must be greater than 0 XLM");
    }

    Ok(stroops)
}

fn emit_progress(
    app: &AppHandle,
    op_id: &str,
    account_id: Option<&str>,
    step: &str,
    message: &str,
) -> Result<()> {
    app.emit(
        "mixer-progress",
        ProgressEvent {
            op_id: op_id.to_string(),
            account_id: account_id.map(ToString::to_string),
            step: step.to_string(),
            message: message.to_string(),
            at: chrono::Utc::now().timestamp_millis(),
        },
    )?;

    Ok(())
}

fn parse_mixer_identity_modulus(input: &str) -> Result<String> {
    let trimmed = input.trim();
    let raw = trimmed.strip_prefix("smxid1:").unwrap_or(trimmed);

    if raw.len() != 512 {
        bail!("recipient Mixer Identity must be smxid1:<512 hex chars RSA modulus>");
    }

    if !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("recipient Mixer Identity contains non-hex characters");
    }

    Ok(raw.to_ascii_lowercase())
}
