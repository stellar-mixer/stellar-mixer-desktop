import { useEffect, useRef, useState } from 'react';
import { backend } from '../lib/tauri';
import { db, upsertAccounts } from '../lib/db';
import type { PublicAccount, UnlockResult } from '../lib/types';

type SetupMode = 'create' | 'recover' | 'import';

export function SetupUnlock({
  exists,
  onUnlocked,
}: {
  exists: boolean;
  onUnlocked: (result: { identityPublicKey: string; accounts: PublicAccount[] }) => void;
}) {
  const [setupMode, setSetupMode] = useState<SetupMode>('create');
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [recoveryPhrase, setRecoveryPhrase] = useState('');
  const [backupJson, setBackupJson] = useState('');
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [recoveryModal, setRecoveryModal] = useState<string>();
  const [exportPath, setExportPath] = useState<string>();
  const [pendingUnlockResult, setPendingUnlockResult] = useState<UnlockResult>();
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    setError(undefined);
    setExportPath(undefined);
    setRecoveryModal(undefined);

    if (exists) {
      setConfirm('');
      setRecoveryPhrase('');
      setBackupJson('');
    }
  }, [exists]);


  function closeRecoveryModal() {
    setRecoveryModal(undefined);

    if (pendingUnlockResult) {
      const result = pendingUnlockResult;
      setPendingUnlockResult(undefined);
      onUnlocked(result);
    }
  }

  async function unlockActiveIdentity() {
    setError(undefined);
    setExportPath(undefined);
    setBusy(true);

    try {
      if (password.length < 8) {
        throw new Error('Password must be at least 8 characters');
      }

      const result = await backend.unlockVault(password);
      onUnlocked(result);
    } catch (e) {
      setError(cleanError(e));
    } finally {
      setBusy(false);
    }
  }

  async function createOrRestoreIdentity() {
    setError(undefined);
    setExportPath(undefined);
    setBusy(true);

    try {
      if (exists) {
        throw new Error('Active Mixer Identity already exists. Unlock it or reset the local app data first.');
      }

      if (password.length < 8) {
        throw new Error('Password must be at least 8 characters');
      }

      if (setupMode === 'create' && password !== confirm) {
        throw new Error('Password confirmation does not match');
      }

      if (setupMode === 'create') {
        const result = await backend.setupVault(password);
        await db.activity.clear();

        if (result.recoveryPhrase) {
          setPendingUnlockResult(result);
          setRecoveryModal(result.recoveryPhrase);
          return;
        }

        onUnlocked(result);
        return;
      }

      if (setupMode === 'recover') {
        if (!recoveryPhrase.trim()) {
          throw new Error('Recovery phrase is empty');
        }

        const result = await backend.setupVaultFromRecoveryPhrase(password, recoveryPhrase);
        await db.accounts.clear();
        await db.activity.clear();
        onUnlocked(result);
        return;
      }

      if (setupMode === 'import') {
        if (!backupJson.trim()) {
          throw new Error('Choose a backup file first');
        }

        const result = await backend.importMixerBackup(password, backupJson);
        await restoreUiStateJson(result.uiStateJson);
        onUnlocked(result);
      }
    } catch (e) {
      setError(cleanError(e));
    } finally {
      setBusy(false);
    }
  }

  async function showRecoveryPhrase() {
    setError(undefined);
    setExportPath(undefined);
    setBusy(true);

    try {
      if (!exists) {
        throw new Error('No active Mixer Identity exists yet');
      }

      if (password.length < 8) {
        throw new Error('Enter the active identity password first');
      }

      const phrase = await backend.exportIdentityRecoveryPhrase(password);
      setRecoveryModal(phrase);
    } catch (e) {
      setError(cleanError(e));
    } finally {
      setBusy(false);
    }
  }

  async function exportBackup() {
    setError(undefined);
    setExportPath(undefined);
    setBusy(true);

    try {
      if (!exists) {
        throw new Error('No active Mixer Identity exists yet');
      }

      if (password.length < 8) {
        throw new Error('Enter the active identity password first');
      }

      const uiStateJson = await collectUiStateJson();
      const result = await backend.exportMixerBackup(password, uiStateJson);
      setExportPath(result.path);
    } catch (e) {
      setError(cleanError(e));
    } finally {
      setBusy(false);
    }
  }

  async function chooseBackupFile(file: File | undefined) {
    setError(undefined);
    setBackupJson('');

    if (!file) return;

    try {
      const text = await file.text();
      setBackupJson(text);
    } catch (e) {
      setError(cleanError(e));
    }
  }

  if (exists) {
    return (
      <main className="unlock-screen">
        <section className="unlock-card unlock-card-wide active-identity-card">
          <div className="brand-mark">✦</div>

          <div className="locked-identity-grid">
            <div className="locked-login-panel">
              <h1>Stellar Mixer</h1>
              <p>
                Active Mixer Identity exists locally. Unlock it to continue.
              </p>

              <label>Vault password</label>
              <input
                type="password"
                value={password}
                autoFocus
                onChange={(e) => setPassword(e.currentTarget.value)}
                onKeyDown={(e) => e.key === 'Enter' && unlockActiveIdentity()}
              />

              {error && <div className="error-banner small unlock-error-visible">{error}</div>}

              {exportPath && (
                <div className="success-banner small">
                  Backup exported to <strong>{exportPath}</strong>
                </div>
              )}

              <button disabled={busy || password.length < 8} onClick={unlockActiveIdentity}>
                {busy ? 'Working…' : 'Unlock active identity'}
              </button>
            </div>

            <aside className="locked-tools-panel">
              <h2>Identity tools</h2>
              <p>
                Available only while the app is locked. To return here later, fully quit and reopen the app.
              </p>

              <button disabled={busy || password.length < 8} onClick={showRecoveryPhrase}>
                Show RSA recovery phrase
              </button>

              <button disabled={busy || password.length < 8} onClick={exportBackup}>
                Export encrypted full backup
              </button>

              <div className="locked-tool-note">
                <strong>Recovery phrase</strong>
                <span>Restores only the RSA Mixer Identity and rescans archive from zero.</span>
              </div>

              <div className="locked-tool-note">
                <strong>Full backup</strong>
                <span>Restores vault, accounts, notes, archive cursors, roles, and history.</span>
              </div>
            </aside>
          </div>

          <p className="fine-print">
            One local app installation has one active Mixer Identity. To use another identity, reset local app data or run another app identifier.
          </p>
        </section>

        {recoveryModal && (
          <RecoveryPhraseModal
            phrase={recoveryModal}
            onClose={() => closeRecoveryModal()}
          />
        )}
      </main>
    );
  }

  const primaryLabel =
    setupMode === 'create'
      ? 'Create new Mixer Identity'
      : setupMode === 'recover'
        ? 'Restore from RSA recovery phrase'
        : 'Import encrypted full backup';

  return (
    <main className="unlock-screen">
      <section className="unlock-card unlock-card-wide">
        <div className="brand-mark">✦</div>
        <h1>Stellar Mixer</h1>
        <p>
          No active local Mixer Identity found. Create a new one, restore only the RSA Mixer Identity,
          or import a full encrypted backup.
        </p>

        <div className="unlock-mode-tabs">
          <button className={setupMode === 'create' ? 'active' : ''} onClick={() => setSetupMode('create')}>
            New identity
          </button>
          <button className={setupMode === 'recover' ? 'active' : ''} onClick={() => setSetupMode('recover')}>
            RSA recovery
          </button>
          <button className={setupMode === 'import' ? 'active' : ''} onClick={() => setSetupMode('import')}>
            Full backup
          </button>
        </div>

        {setupMode === 'create' && (
          <>
            <label>New password</label>
            <input
              type="password"
              value={password}
              autoFocus
              onChange={(e) => setPassword(e.currentTarget.value)}
              onKeyDown={(e) => e.key === 'Enter' && createOrRestoreIdentity()}
            />

            <label>Confirm password</label>
            <input
              type="password"
              value={confirm}
              onChange={(e) => setConfirm(e.currentTarget.value)}
              onKeyDown={(e) => e.key === 'Enter' && createOrRestoreIdentity()}
            />
          </>
        )}

        {setupMode === 'recover' && (
          <>
            <label>RSA Mixer Identity recovery phrase</label>
            <textarea
              className="unlock-textarea"
              value={recoveryPhrase}
              autoFocus
              placeholder={'24 words, separated by spaces or new lines'}
              spellCheck={false}
              onChange={(e) => setRecoveryPhrase(e.currentTarget.value)}
            />

            <label>New local vault password</label>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.currentTarget.value)}
              onKeyDown={(e) => e.key === 'Enter' && createOrRestoreIdentity()}
            />

            <p className="unlock-note">
              The recovery phrase itself is not password-encrypted. This password only protects the new local encrypted vault on this device.
              Stellar accounts are not restored; archive sync starts from zero and recovers encrypted notes/nullifiers.
            </p>
          </>
        )}

        {setupMode === 'import' && (
          <>
            <label>Encrypted backup file</label>

            <input
              ref={fileInputRef}
              type="file"
              accept="application/json,.json"
              className="hidden-file-input"
              onChange={(e) => chooseBackupFile(e.currentTarget.files?.[0])}
            />

            <button
              type="button"
              className="secondary-action"
              disabled={busy}
              onClick={() => fileInputRef.current?.click()}
            >
              {backupJson ? 'Backup file loaded ✓' : 'Choose backup file'}
            </button>

            <label>Backup password</label>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.currentTarget.value)}
              onKeyDown={(e) => e.key === 'Enter' && createOrRestoreIdentity()}
            />

            <p className="unlock-note">
              This password decrypts the backup and becomes the local vault password after import.
              Full backup restores vault, accounts, notes, archive cursors, roles, and history.
            </p>
          </>
        )}

        {error && <div className="error-banner small unlock-error-visible">{error}</div>}

        <button disabled={busy || password.length < 8} onClick={createOrRestoreIdentity}>
          {busy ? 'Working…' : primaryLabel}
        </button>

        <p className="fine-print">
          New identity starts from current archive cursors. RSA recovery phrase is plain/unprotected and starts from zero. Full backup keeps saved cursors.
        </p>
      </section>

      {recoveryModal && (
        <RecoveryPhraseModal
          phrase={recoveryModal}
          onClose={() => closeRecoveryModal()}
        />
      )}
    </main>
  );
}

function RecoveryPhraseModal({
  phrase,
  onClose,
}: {
  phrase: string;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    await copyText(phrase);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  }

  return (
    <div className="secret-key-confirm-backdrop" onMouseDown={onClose}>
      <section className="secret-key-confirm-card recovery-card" onMouseDown={(event) => event.stopPropagation()}>
        <h3>Mixer Identity recovery phrase</h3>

        <p>
          Store this 24-word phrase offline. Anyone with it can recover the Mixer Identity and scan the archive for its notes.
        </p>

        <textarea readOnly className="recovery-phrase-output" value={phrase} />

        <div className="secret-key-confirm-actions">
          <button type="button" className="ghost-button" onClick={onClose}>
            Close
          </button>

          <button type="button" className="danger-button" onClick={copy}>
            {copied ? 'Copied ✓' : 'Copy phrase'}
          </button>
        </div>
      </section>
    </div>
  );
}

async function collectUiStateJson(): Promise<string> {
  const [accounts, activity] = await Promise.all([
    db.accounts.toArray(),
    db.activity.toArray(),
  ]);

  return JSON.stringify({
    version: 1,
    exportedAt: Date.now(),
    accounts,
    activity,
  });
}

async function restoreUiStateJson(value: string | undefined) {
  await db.accounts.clear();
  await db.activity.clear();

  if (!value?.trim()) return;

  const parsed = JSON.parse(value);

  if (Array.isArray(parsed.accounts)) {
    await upsertAccounts(parsed.accounts);
  }

  if (Array.isArray(parsed.activity) && parsed.activity.length > 0) {
    await db.activity.bulkPut(parsed.activity);
  }
}

async function copyText(value: string) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(value);
    return;
  }

  const textarea = document.createElement('textarea');
  textarea.value = value;
  textarea.style.position = 'fixed';
  textarea.style.left = '-9999px';
  textarea.setAttribute('readonly', 'true');

  document.body.appendChild(textarea);
  textarea.focus();
  textarea.select();
  document.execCommand('copy');
  textarea.remove();
}

function cleanError(error: unknown): string {
  const raw = String(error);

  if (raw.startsWith('Error: ')) {
    return raw.slice('Error: '.length);
  }

  return raw;
}
