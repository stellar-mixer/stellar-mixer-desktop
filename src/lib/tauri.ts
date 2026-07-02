import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  ArchiveSyncReport,
  BackupExportResult,
  BackupImportResult,
  DepositResult,
  MixerStats,
  NoteView,
  PrivateBalance,
  ProgressEvent,
  PublicAccount,
  UnlockResult,
  VaultStatus,
  WithdrawResult,
  TransferResult,
} from './types';

export const backend = {
  vaultStatus: () => invoke<VaultStatus>('vault_status'),
  setupVault: (password: string) => invoke<UnlockResult>('setup_vault', { password }),
  setupVaultFromRecoveryPhrase: (password: string, recoveryPhrase: string) =>
    invoke<UnlockResult>('setup_vault_from_recovery_phrase', { password, recoveryPhrase }),
  unlockVault: (password: string) => invoke<UnlockResult>('unlock_vault', { password }),
  exportIdentityRecoveryPhrase: (password: string) =>
    invoke<string>('export_identity_recovery_phrase', { password }),
  exportMixerBackup: (password: string, uiStateJson: string) =>
    invoke<BackupExportResult>('export_mixer_backup', { password, uiStateJson }),
  importMixerBackup: (password: string, backupJson: string) =>
    invoke<BackupImportResult>('import_mixer_backup', { password, backupJson }),
  lockVault: () => invoke<void>('lock_vault'),
  createAccount: (name: string) => invoke<PublicAccount>('create_stellar_account', { name }),
  listAccounts: () => invoke<PublicAccount[]>('list_accounts'),
  exportStellarSecretKey: (accountId: string) => invoke<string>('export_stellar_secret_key', { accountId }),
  copyStellarSecretKey: (accountId: string) => invoke<void>('copy_stellar_secret_key', { accountId }),
  notesSummary: () => invoke<NoteView[]>('notes_summary'),
  privateBalance: () => invoke<PrivateBalance>('private_balance'),
  treePirStatus: () => invoke<MixerStats>('treepir_status'),
  syncArchive: () => invoke<ArchiveSyncReport>('sync_archive'),
  deposit: (accountId: string, amount: string) => invoke<DepositResult>('deposit', { accountId, amount }),
  withdraw: (accountId: string, amount: string, feePayerAccountId?: string, recipientAddress?: string) =>
    invoke<WithdrawResult>('withdraw', { accountId, amount, feePayerAccountId, recipientAddress }),
  transfer: (
    accountId: string,
    amount: string,
    recipientIdentity: string,
    message?: string,
    feePayerAccountId?: string,
  ) =>
    invoke<TransferResult>('transfer', {
      accountId,
      amount,
      recipientIdentity,
      message,
      feePayerAccountId,
    }),
  identityAddress: () => invoke<string>('identity_address'),
};

export const listenProgress = (handler: (event: ProgressEvent) => void) => {
  return listen<ProgressEvent>('mixer-progress', (event) => handler(event.payload));
};


export async function openExternalUrl(url: string) {
  try {
    await invoke('plugin:opener|open_url', { url });
    return;
  } catch (error) {
    console.warn('Tauri opener failed, falling back to window.open', error);
  }

  const opened = window.open(url, '_blank', 'noopener,noreferrer');

  if (!opened) {
    await navigator.clipboard?.writeText(url);
    throw new Error(`failed to open external URL; copied to clipboard instead: ${url}`);
  }
}
