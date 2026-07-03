use std::collections::HashSet;
use std::sync::Mutex;

use anyhow::{bail, Result};

use crate::models::VaultPayload;

#[derive(Default)]
pub struct AppState {
    pub session: Mutex<Option<Session>>,
    pub spend_active: Mutex<Option<String>>,
    #[allow(dead_code)]
    pub account_operations: Mutex<HashSet<String>>,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub password: String,
    pub vault: VaultPayload,
}

pub struct SpendOperationGuard<'a> {
    state: &'a AppState,
}

#[allow(dead_code)]
pub struct AccountOperationGuard<'a> {
    state: &'a AppState,
    account_id: String,
}

impl AppState {
    pub fn start_spend_operation(
        &self,
        label: impl Into<String>,
    ) -> Result<SpendOperationGuard<'_>> {
        let label = label.into();
        let mut active = self.spend_active.lock().expect("spend lock mutex poisoned");

        if let Some(existing) = active.as_ref() {
            bail!("another spend operation is already running: {existing}; concurrent withdraw/transfer is disabled to avoid note-selection races");
        }

        *active = Some(label);

        Ok(SpendOperationGuard { state: self })
    }

    pub fn start_withdraw(&self) -> Result<SpendOperationGuard<'_>> {
        self.start_spend_operation("withdraw")
    }

    pub fn start_transfer(&self) -> Result<SpendOperationGuard<'_>> {
        self.start_spend_operation("transfer")
    }

    #[allow(dead_code)]
    pub fn start_account_operation(
        &self,
        account_id: impl Into<String>,
        label: &str,
    ) -> Result<AccountOperationGuard<'_>> {
        let account_id = account_id.into();

        let mut active = self
            .account_operations
            .lock()
            .expect("account operations mutex poisoned");

        if active.contains(&account_id) {
            bail!("account already has an active operation; wait until the current account operation finishes before starting {label}");
        }

        active.insert(account_id.clone());

        Ok(AccountOperationGuard {
            state: self,
            account_id,
        })
    }
}

impl Drop for SpendOperationGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.state.spend_active.lock() {
            *active = None;
        }
    }
}

impl Drop for AccountOperationGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.state.account_operations.lock() {
            active.remove(&self.account_id);
        }
    }
}
