use serde::{Deserialize, Serialize};

pub const RSA_MODULUS_BYTES: usize = 256;
pub const NULLIFIER_BYTES: usize = 32;
pub const SECRET_BYTES: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub exists: bool,
    pub unlocked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockResult {
    pub identity_public_key: String,
    pub accounts: Vec<PublicAccount>,
    pub note_count: usize,

    #[serde(default)]
    pub recovery_phrase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicAccount {
    pub id: String,
    pub name: String,
    pub public_key: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixerStats {
    pub anonymity_set: usize,
    pub root_hex: Option<String>,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateBalance {
    pub total_stroops: String,
    pub total_xlm: String,
    pub spendable_stroops: String,
    pub spendable_xlm: String,
    pub pending_stroops: String,
    pub pending_xlm: String,
    pub unspent_note_count: usize,
    pub spendable_note_count: usize,
    pub max_inputs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSyncReport {
    pub archive_contract_id: Option<String>,
    pub encrypted_note_cursor: u64,
    pub nullifier_cursor: u64,
    pub scanned_encrypted_notes: u64,
    pub scanned_nullifiers: u64,
    pub imported_note_count: usize,
    pub spent_note_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupExportResult {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupImportResult {
    pub identity_public_key: String,
    pub accounts: Vec<PublicAccount>,
    pub note_count: usize,
    pub ui_state_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositResult {
    pub tx_hash: String,
    pub leaf_index: u64,
    pub note_id: String,
    pub leaf_hex: String,
    pub amount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawResult {
    pub tx_hash: String,
    pub spent_note_ids: Vec<String>,
    pub created_note_id: Option<String>,
    pub amount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteView {
    pub id: String,
    pub deposited_by_account_id: String,
    pub deposited_by_account_name: String,
    pub deposited_by_public_key: String,
    pub amount_stroops: String,
    pub amount_xlm: String,
    pub leaf_index: Option<u64>,
    pub status: String,
    pub created_at: i64,
    pub spent_at: Option<i64>,

    #[serde(default)]
    pub source_kind: Option<String>,

    #[serde(default)]
    pub source_ledger: Option<u64>,

    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferResult {
    pub tx_hash: String,
    pub spent_note_ids: Vec<String>,
    pub created_note_ids: Vec<String>,
    pub amount: String,
    pub recipient_identity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub op_id: String,
    pub account_id: Option<String>,
    pub step: String,
    pub message: String,
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultPayload {
    pub version: u32,
    pub identity: MixerIdentitySecret,
    pub accounts: Vec<AccountSecret>,
    pub notes: Vec<NoteSecret>,

    #[serde(default)]
    pub archive: ArchiveSyncState,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSyncState {
    #[serde(default)]
    pub contract_id: Option<String>,

    #[serde(default)]
    pub encrypted_note_cursor: u64,

    #[serde(default)]
    pub nullifier_cursor: u64,

    #[serde(default)]
    pub last_synced_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixerIdentitySecret {
    pub schnorrkel_keypair_hex: String,
    pub schnorrkel_public_hex: String,
    pub treepir_identity_bincode_hex: Option<String>,

    #[serde(default)]
    pub recovery_kind: Option<String>,

    #[serde(default)]
    pub recovery_phrase: Option<String>,

    #[serde(default)]
    pub rsa_private_key_pkcs1_der_hex: Option<String>,

    #[serde(default)]
    pub rsa_public_modulus_hex: Option<String>,

    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSecret {
    pub id: String,
    pub name: String,
    pub stellar_secret_key: String,
    pub stellar_public_key: String,

    pub rsa_private_key_pkcs1_der_hex: String,
    pub rsa_public_modulus_hex: String,

    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSecret {
    pub id: String,

    pub account_id: String,

    pub value: u128,
    pub owner_modulus_hex: String,
    pub nullifier_hex: String,
    pub secret_hex: String,
    pub leaf_hex: String,
    pub leaf_index: Option<u64>,
    pub spent: bool,
    pub created_at: i64,
    pub spent_at: Option<i64>,

    #[serde(default)]
    pub source_event_id: Option<String>,

    #[serde(default)]
    pub source_ledger: Option<u64>,

    #[serde(default)]
    pub source_kind: Option<String>,

    #[serde(default)]
    pub message: Option<String>,
}

impl VaultPayload {
    pub fn public_accounts(&self) -> Vec<PublicAccount> {
        self.accounts
            .iter()
            .map(|account| PublicAccount {
                id: account.id.clone(),
                name: account.name.clone(),
                public_key: account.stellar_public_key.clone(),
                created_at: account.created_at,
            })
            .collect()
    }
}
