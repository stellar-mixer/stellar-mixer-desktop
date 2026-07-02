import { useMemo, useState, type KeyboardEvent, type MouseEvent } from 'react';
import type { AccountRole, AccountView } from '../lib/types';
import { backend, openExternalUrl } from '../lib/tauri';

const TESTNET_ACCOUNT_EXPLORER_BASE = 'https://stellar.expert/explorer/testnet/account';

type AccountFilter = AccountRole;

export type AccountStatusBadge = {
  id: string;
  label: string;
  tone: 'success' | 'error' | 'deposit' | 'withdraw' | 'transfer';
};

type Props = {
  accounts: AccountView[];
  selectedAccountId?: string;
  refreshingBalances?: boolean;
  activeAccountOps: Record<string, string[]>;
  accountStatuses?: Record<string, AccountStatusBadge[]>;
  accountActivityKinds?: Record<string, unknown>;
  onSelect: (id: string) => void;
  onCreate?: (name: string) => Promise<void>;
  onCreateAccount?: (name: string) => Promise<void>;
  onSetAccountRole?: (accountId: string, role: AccountRole) => Promise<void> | void;
  onRefreshBalances: () => void;
};

export function AccountSidebar({
  accounts,
  selectedAccountId,
  refreshingBalances,
  activeAccountOps,
  accountStatuses,
  onSelect,
  onCreate,
  onCreateAccount,
  onSetAccountRole,
  onRefreshBalances,
}: Props) {
  const [name, setName] = useState('');
  const [creating, setCreating] = useState(false);
  const [copiedAccountId, setCopiedAccountId] = useState<string | undefined>();
  const [secretCopiedAccountId, setSecretCopiedAccountId] = useState<string | undefined>();
  const [secretExportAccount, setSecretExportAccount] = useState<AccountView | undefined>();
  const [secretCopyBusy, setSecretCopyBusy] = useState(false);
  const [secretCopyError, setSecretCopyError] = useState<string | undefined>();
  const [filter, setFilter] = useState<AccountFilter>('all');
  const [roleMenuAccountId, setRoleMenuAccountId] = useState<string | undefined>();

  const createHandler = onCreate ?? onCreateAccount;

  const sortedAccounts = useMemo(() => {
    return [...accounts].sort((left, right) => {
      const balanceDiff = parseXlmBalance(right.balance) - parseXlmBalance(left.balance);

      if (Math.abs(balanceDiff) > 0.0000001) return balanceDiff;
      return left.name.localeCompare(right.name);
    });
  }, [accounts]);

  const visibleAccounts = useMemo(() => {
    if (filter === 'all') return sortedAccounts;

    return sortedAccounts.filter((account) => accountRole(account) === filter);
  }, [sortedAccounts, filter]);

  const tabs = useMemo(() => {
    const count = (role: AccountFilter) =>
      role === 'all'
        ? sortedAccounts.length
        : sortedAccounts.filter((account) => accountRole(account) === role).length;

    return [
      { id: 'all' as const, label: 'All', count: count('all') },
      { id: 'deposit' as const, label: 'Deposit', count: count('deposit') },
      { id: 'withdraw' as const, label: 'Withdraw', count: count('withdraw') },
      { id: 'transfer' as const, label: 'Transfer', count: count('transfer') },
    ];
  }, [sortedAccounts]);

  async function create() {
    const trimmed = name.trim();

    if (!trimmed || creating || !createHandler) return;

    setCreating(true);

    try {
      await createHandler(trimmed);
      setName('');
      setFilter('all');
    } finally {
      setCreating(false);
    }
  }

  function handleCardKey(event: KeyboardEvent<HTMLDivElement>, accountId: string) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onSelect(accountId);
    }
  }

  async function copyAddress(account: AccountView, event: MouseEvent<HTMLButtonElement>) {
    event.stopPropagation();

    await copyText(account.publicKey);

    setCopiedAccountId(account.id);
    window.setTimeout(() => {
      setCopiedAccountId((current) => (current === account.id ? undefined : current));
    }, 1100);
  }

  async function openAccountExplorer(account: AccountView, event: MouseEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();

    await openExternalUrl(`${TESTNET_ACCOUNT_EXPLORER_BASE}/${account.publicKey}`);
  }

  function requestSecretExport(account: AccountView, event: MouseEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();

    setSecretCopyError(undefined);
    setSecretExportAccount(account);
  }

  async function confirmCopySecretKey(account: AccountView) {
    setSecretCopyBusy(true);
    setSecretCopyError(undefined);

    try {
      await backend.copyStellarSecretKey(account.id);

      setSecretCopiedAccountId(account.id);
      setSecretExportAccount(undefined);

      window.setTimeout(() => {
        setSecretCopiedAccountId((current) => (current === account.id ? undefined : current));
      }, 1400);
    } catch (error) {
      setSecretCopyError(String(error));
    } finally {
      setSecretCopyBusy(false);
    }
  }

  function toggleRoleMenu(accountId: string, event: MouseEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();

    setRoleMenuAccountId((current) => (current === accountId ? undefined : accountId));
  }

  async function setRole(accountId: string, role: AccountRole, event: MouseEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();

    await onSetAccountRole?.(accountId, role);
    setRoleMenuAccountId(undefined);

    if (filter !== 'all' && role !== filter) {
      setFilter('all');
    }
  }

  return (
    <aside className="account-sidebar">
      <button
        type="button"
        className="tiny-refresh-button"
        onClick={onRefreshBalances}
        disabled={refreshingBalances}
        title="Refresh balances"
        aria-label="Refresh balances"
      >
        {refreshingBalances ? '…' : '↻'}
      </button>

      <div className="account-filter-tabs" role="tablist" aria-label="Account role filters">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={filter === tab.id}
            className={`account-filter-tab ${filter === tab.id ? 'active' : ''}`}
            onClick={() => {
              setFilter(tab.id);
              setRoleMenuAccountId(undefined);
            }}
          >
            <span>{tab.label}</span>
            <strong>{tab.count}</strong>
          </button>
        ))}
      </div>

      <div className="account-list">
        {visibleAccounts.length === 0 && (
          <div className="account-empty-filter">
            No accounts in this group yet.
          </div>
        )}

        {visibleAccounts.map((account) => {
          const selected = account.id === selectedAccountId;
          const role = accountRole(account);
          const ops = activeAccountOps[account.id] ?? [];
          const statuses = accountStatuses?.[account.id] ?? [];
          const menuOpen = roleMenuAccountId === account.id;

          return (
            <div
              key={account.id}
              role="button"
              tabIndex={0}
              className={`account-card ${selected ? 'selected' : ''}`}
              onClick={() => {
                setRoleMenuAccountId(undefined);
                onSelect(account.id);
              }}
              onKeyDown={(event) => handleCardKey(event, account.id)}
            >
              <button
                type="button"
                className="account-role-edit-button"
                onClick={(event) => toggleRoleMenu(account.id, event)}
                title="Change account group"
                aria-label={`Change group for ${account.name}`}
              >
                ⋯
              </button>

              {menuOpen && (
                <div className="account-role-menu" onClick={(event) => event.stopPropagation()}>
                  <div className="account-role-menu-title">Account group</div>

                  {(['all', 'deposit', 'withdraw', 'transfer'] as AccountRole[]).map((option) => (
                    <button
                      key={option}
                      type="button"
                      className={`account-role-menu-item ${role === option ? 'active' : ''}`}
                      onClick={(event) => setRole(account.id, option, event)}
                    >
                      <span>{roleDisplayName(option)}</span>
                      {role === option && <strong>✓</strong>}
                    </button>
                  ))}
                </div>
              )}

              <div className="account-card-top">
                <div className="account-card-name-role">
                  <strong>{account.name}</strong>
                  <span className={`account-role-label role-${role}`}>
                    {roleDisplayName(role)}
                  </span>
                </div>

                <span className="account-balance">{account.balance ?? '0 XLM'}</span>
              </div>

              <div className="account-address-row account-address-row-export">
                <div className="account-address-left">
                  <span className="account-address" title={account.publicKey}>
                    {shortPublicKey(account.publicKey)}
                  </span>

                  <button
                    type="button"
                    className={`copy-address-button ${copiedAccountId === account.id ? 'copied' : ''}`}
                    onClick={(event) => copyAddress(account, event)}
                    title="Copy address"
                    aria-label={`Copy ${account.name} address`}
                  >
                    {copiedAccountId === account.id ? '✓' : '⧉'}
                  </button>

                  <button
                    type="button"
                    className="account-explorer-button"
                    onClick={(event) => openAccountExplorer(account, event)}
                    title="Open account in StellarExpert"
                    aria-label={`Open ${account.name} in StellarExpert`}
                  >
                    ↗
                  </button>
                </div>

                <button
                  type="button"
                  className={`secret-key-copy-button ${secretCopiedAccountId === account.id ? 'copied' : ''}`}
                  onClick={(event) => requestSecretExport(account, event)}
                  title="Copy Stellar secret key"
                  aria-label={`Copy ${account.name} secret key`}
                >
                  {secretCopiedAccountId === account.id ? '✓' : 'SK'}
                </button>
              </div>

              {(ops.length > 0 || statuses.length > 0) && (
                <div className="account-op-row">
                  {ops.map((op) => (
                    <span key={`active-${op}`} className={`op-badge ${op}`}>
                      {op}
                    </span>
                  ))}

                  {statuses.map((status) => (
                    <span key={status.id} className={`op-badge ${status.tone}`}>
                      {status.label}
                    </span>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {secretExportAccount && (
        <div className="secret-key-confirm-backdrop" onMouseDown={() => setSecretExportAccount(undefined)}>
          <section className="secret-key-confirm-card" onMouseDown={(event) => event.stopPropagation()}>
            <h3>Copy secret key?</h3>

            <p>
              This will copy the Stellar secret key for <strong>{secretExportAccount.name}</strong> to your clipboard.
              Anyone with this key can control this Stellar account.
            </p>

            {secretCopyError && (
              <div className="secret-key-confirm-error">
                {secretCopyError}
              </div>
            )}

            <div className="secret-key-confirm-actions">
              <button type="button" className="ghost-button" onClick={() => setSecretExportAccount(undefined)}>
                Cancel
              </button>

              <button
                type="button"
                className="danger-button"
                disabled={secretCopyBusy}
                onClick={() => confirmCopySecretKey(secretExportAccount)}
              >
                {secretCopyBusy ? 'Copying…' : 'OK, copy secret key'}
              </button>
            </div>
          </section>
        </div>
      )}

      <div className="create-account-box">
        <input
          placeholder="Account name"
          value={name}
          disabled={creating}
          onChange={(event) => setName(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') create();
          }}
        />

        <button type="button" disabled={!name.trim() || creating || !createHandler} onClick={create}>
          + Add account
        </button>
      </div>
    </aside>
  );
}

function accountRole(account: Pick<AccountView, 'role'>): AccountRole {
  return account.role ?? 'all';
}

function roleDisplayName(role: AccountRole): string {
  if (role === 'deposit') return 'Deposit only';
  if (role === 'withdraw') return 'Withdraw only';
  if (role === 'transfer') return 'Transfer only';

  return 'All';
}

function shortPublicKey(value: string): string {
  if (!value || value.length <= 18) return value;

  return `${value.slice(0, 8)}…${value.slice(-8)}`;
}

function parseXlmBalance(value?: string): number {
  if (!value) return 0;

  const normalized = value.replace('XLM', '').trim().replace(',', '.');
  const parsed = Number(normalized);

  return Number.isFinite(parsed) ? parsed : 0;
}

async function copyText(value: string) {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value);
      return;
    }
  } catch {
    // fallback below
  }

  const textarea = document.createElement('textarea');
  textarea.value = value;
  textarea.setAttribute('readonly', 'true');
  textarea.style.position = 'fixed';
  textarea.style.left = '-9999px';
  textarea.style.top = '0';

  document.body.appendChild(textarea);
  textarea.focus();
  textarea.select();

  const copied = document.execCommand('copy');
  textarea.remove();

  if (!copied) {
    throw new Error('clipboard copy failed');
  }
}
