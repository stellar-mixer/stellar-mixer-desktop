mod commands;
mod config;
mod error;
mod mixer;
mod models;
mod proofs;
mod security;
mod state;
mod stellar;
mod storage;

use state::AppState;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::setup_vault,
            commands::unlock_vault,
            commands::import_mixer_backup,
            commands::export_mixer_backup,
            commands::export_identity_recovery_phrase,
            commands::setup_vault_from_recovery_phrase,
            commands::lock_vault,
            commands::create_stellar_account,
            commands::list_accounts,
            commands::export_stellar_secret_key,
            commands::copy_stellar_secret_key,
            commands::identity_address,
            commands::notes_summary,
            commands::private_balance,
            commands::treepir_status,
            commands::sync_archive,
            commands::deposit,
            commands::withdraw,
            commands::transfer
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
