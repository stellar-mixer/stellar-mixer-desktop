import { useEffect, useMemo, useRef, useState } from 'react';
import { liveQuery } from 'dexie';
import { backend, listenProgress } from './lib/tauri';
import { addActivity, db, updateAccountRole, upsertAccounts } from './lib/db';
import { getNativeBalances } from './lib/stellarPublic';
import type {
  AccountRole,
  AccountView,
  ActivityItem,
  MixerStats,
  NoteView,
  PrivateBalance,
  ProgressEvent,
  PublicAccount,
  VaultStatus,
} from './lib/types';
import { SetupUnlock } from './components/SetupUnlock';
import { AccountSidebar } from './components/AccountSidebar';
import { MixerHeader } from './components/MixerHeader';
import { ActivityFeed } from './components/ActivityFeed';
import { ProgressLog } from './components/ProgressLog';
import { OperationHistory } from './components/OperationHistory';
import { OperationTabs } from './components/OperationTabs';
import { NotesModal } from './components/NotesModal';
import { ErrorModal } from './components/ErrorModal';

const ALL_BALANCES_REFRESH_MS = 10_000;
const TREEPIR_REFRESH_MS = 30_000;
const ARCHIVE_SYNC_REFRESH_MS = 30_000;
const PRIVATE_BALANCE_REFRESH_MS = 20_000;
const ACCOUNT_MIN_RESERVE_XLM = 1;
const MAX_CONTRACT_TX_FEE_XLM = 2;
const MIN_FEE_PAYER_XLM = ACCOUNT_MIN_RESERVE_XLM + MAX_CONTRACT_TX_FEE_XLM;

type UiError = {
  accountId?: string;
  message: string;
};

type PendingFeePayerWithdraw = {
  recipientAccountId: string;
  amount: string;
  recipientAddress?: string;
};

type PendingFeePayerTransfer = {
  senderAccountId: string;
  amount: string;
  recipientIdentity: string;
  message?: string;
};

type AccountFlashStatus = {
  id: string;
  label: string;
  tone: 'success' | 'error' | 'deposit' | 'withdraw' | 'transfer';
};

export default function App() {
  const [vaultStatus, setVaultStatus] = useState<VaultStatus>({ exists: false, unlocked: false });
  const [accounts, setAccounts] = useState<AccountView[]>([]);
  const [activity, setActivity] = useState<ActivityItem[]>([]);
  const [selectedAccountId, setSelectedAccountId] = useState<string | undefined>();
  const [identityPublicKey, setIdentityPublicKey] = useState<string | undefined>();
  const [mixerIdentityAddress, setMixerIdentityAddress] = useState<string | undefined>();
  const [stats, setStats] = useState<MixerStats>({ anonymitySet: 0 });
  const [privateBalance, setPrivateBalance] = useState<PrivateBalance | undefined>();
  const [progress, setProgress] = useState<ProgressEvent[]>([]);
  const [busyOps, setBusyOps] = useState(0);
  const [spendBusy, setSpendBusy] = useState(false);
  const [activeAccountOps, setActiveAccountOps] = useState<Record<string, string[]>>({});
  const [accountStatuses, setAccountStatuses] = useState<Record<string, AccountFlashStatus[]>>({});
  const [refreshingBalances, setRefreshingBalances] = useState(false);
  const [error, setError] = useState<UiError | undefined>();
  const [pendingFeePayerWithdraw, setPendingFeePayerWithdraw] = useState<PendingFeePayerWithdraw | undefined>();
  const [pendingFeePayerTransfer, setPendingFeePayerTransfer] = useState<PendingFeePayerTransfer | undefined>();
  const [notesModalOpen, setNotesModalOpen] = useState(false);
  const [notes, setNotes] = useState<NoteView[]>([]);
  const [incomingTransferNotes, setIncomingTransferNotes] = useState<NoteView[]>([]);
  const [historyTab, setHistoryTab] = useState<'operations' | 'raw'>('operations');

  const isBusy = busyOps > 0;
  const balancesInFlight = useRef(false);
  const archiveSyncInFlight = useRef(false);
  const lastAllBalancesRefresh = useRef(0);

  useEffect(() => {
    backend.vaultStatus().then(setVaultStatus).catch((e) => setError({ message: String(e) }));

    const sub1 = liveQuery(() => db.accounts.toArray()).subscribe((rows) => {
      setAccounts((prev) => {
        const balanceById = new Map(prev.map((a) => [a.id, a.balance]));
        return rows.map((r) => ({ ...r, balance: balanceById.get(r.id) ?? r.balance }));
      });
    });

    const sub2 = liveQuery(() => db.activity.orderBy('createdAt').reverse().limit(200).toArray()).subscribe(setActivity);

    const refreshStats = () => backend.treePirStatus().then(setStats).catch(() => undefined);
    refreshStats();

    const statsTimer = window.setInterval(refreshStats, TREEPIR_REFRESH_MS);

    let unlisten: (() => void) | undefined;
    let cancelled = false;

    listenProgress((event) => {
      setProgress((prev) => [event, ...prev].slice(0, 160));
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      cancelled = true;
      sub1.unsubscribe();
      sub2.unsubscribe();
      window.clearInterval(statsTimer);
      unlisten?.();
    };
  }, []);


  useEffect(() => {
    if (!vaultStatus.unlocked) return;

    let cancelled = false;

    const run = async () => {
      if (cancelled) return;
      await syncArchiveFromServer();
    };

    window.setTimeout(run, 300);

    const timer = window.setInterval(run, ARCHIVE_SYNC_REFRESH_MS);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [vaultStatus.unlocked]);

  useEffect(() => {
    if (!vaultStatus.unlocked) return;

    refreshPrivateBalance();

    const timer = window.setInterval(() => {
      refreshPrivateBalance();
    }, PRIVATE_BALANCE_REFRESH_MS);

    return () => window.clearInterval(timer);
  }, [vaultStatus.unlocked]);

  useEffect(() => {
    if (!selectedAccountId && accounts[0]) {
      setSelectedAccountId(accounts[0].id);
      return;
    }

    if (selectedAccountId && accounts.length > 0 && !accounts.some((account) => account.id === selectedAccountId)) {
      setSelectedAccountId(accounts[0]?.id);
    }
  }, [accounts, selectedAccountId]);

  useEffect(() => {
    if (!vaultStatus.unlocked || accounts.length === 0) return;

    refreshAllBalances(true);

    const timer = window.setInterval(() => {
      refreshAllBalances(false);
    }, ALL_BALANCES_REFRESH_MS);

    return () => window.clearInterval(timer);
  }, [vaultStatus.unlocked, accounts.map((a) => a.id).join('|')]);

  const selectedAccount = useMemo(
    () => accounts.find((a) => a.id === selectedAccountId),
    [accounts, selectedAccountId],
  );

  const selectedAllowedOperations = useMemo(
    () => operationPermissionsForRole(accountRole(selectedAccount)),
    [selectedAccount?.role],
  );

  const feePayerCandidates = useMemo(() => {
    const recipientId = pendingFeePayerWithdraw?.recipientAccountId ?? selectedAccountId;

    return accounts.filter((account) => {
      if (account.id === recipientId) return false;
      return accountCanRunOperation(account, 'withdraw') && parseXlmBalance(account.balance) >= MIN_FEE_PAYER_XLM;
    });
  }, [accounts, pendingFeePayerWithdraw, selectedAccountId]);

  const transferFeePayerCandidates = useMemo(() => {
    const senderId = pendingFeePayerTransfer?.senderAccountId ?? selectedAccountId;

    return accounts.filter((account) => {
      if (account.id === senderId) return false;
      return accountCanRunOperation(account, 'transfer') && parseXlmBalance(account.balance) >= MIN_FEE_PAYER_XLM;
    });
  }, [accounts, pendingFeePayerTransfer, selectedAccountId]);

  function beginOperation() {
    setBusyOps((value) => value + 1);
  }

  function endOperation() {
    setBusyOps((value) => Math.max(0, value - 1));
  }

  function startAccountOp(accountId: string, op: string) {
    setActiveAccountOps((prev) => {
      const existing = prev[accountId] ?? [];
      return { ...prev, [accountId]: [...new Set([...existing, op])] };
    });
  }

  function finishAccountOp(accountId: string, op: string) {
    setActiveAccountOps((prev) => {
      const nextOps = (prev[accountId] ?? []).filter((item) => item !== op);
      const next = { ...prev };

      if (nextOps.length === 0) {
        delete next[accountId];
      } else {
        next[accountId] = nextOps;
      }

      return next;
    });
  }

  function setAccountFlashStatus(
    accountId: string,
    label: string,
    tone: AccountFlashStatus['tone'],
  ) {
    const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;

    setAccountStatuses((prev) => ({
      ...prev,
      [accountId]: [{ id, label, tone }],
    }));
  }

  function clearAccountFlashStatusAfterClick(accountId: string) {
    window.setTimeout(() => {
      setAccountStatuses((prev) => {
        if (!prev[accountId]?.length) return prev;

        const next = { ...prev };
        delete next[accountId];

        return next;
      });
    }, 1000);
  }



  async function syncArchiveFromServer() {
    if (archiveSyncInFlight.current) return;

    archiveSyncInFlight.current = true;

    try {
      const report = await backend.syncArchive();
      const receivedTransferNotes = report.receivedTransferNotes ?? [];

      if (receivedTransferNotes.length > 0) {
        setIncomingTransferNotes((prev) => {
          const existing = new Set(prev.map((note) => note.id));
          const fresh = receivedTransferNotes.filter((note) => !existing.has(note.id));

          return [...fresh, ...prev].slice(0, 8);
        });

        await Promise.all(
          receivedTransferNotes.map((note) =>
            addActivity({
              kind: 'transfer',
              title: 'Incoming private note received',
              detail: incomingTransferNoteDetail(note),
              noteId: note.id,
              outputNoteIds: [note.id],
              leafIndex: note.leafIndex,
              message: note.message,
              createdAt: Date.now(),
              status: 'success',
            }),
          ),
        );
      }

      if (report.importedNoteCount > 0 || report.spentNoteCount > 0) {
        await addActivity({
          kind: 'system',
          title: 'Mixer archive synced',
          detail: `imported ${report.importedNoteCount} note(s), marked spent ${report.spentNoteCount}; encrypted_note cursor ${report.encryptedNoteCursor}, nullifier cursor ${report.nullifierCursor}`,
          createdAt: Date.now(),
          status: 'success',
        });

        await refreshPrivateBalance();
        await refreshNotes();
        backend.treePirStatus().then(setStats).catch(() => undefined);
      }
    } catch (e) {
      console.warn('mixer archive sync failed', e);
    } finally {
      archiveSyncInFlight.current = false;
    }
  }

  async function refreshPrivateBalance() {
    try {
      const balance = await backend.privateBalance();
      setPrivateBalance(balance);
    } catch {
      // vault can be locked during startup
    }
  }

  async function refreshNotes() {
    try {
      const value = await backend.notesSummary();
      setNotes(value);
    } catch {
      // vault can be locked during startup
    }
  }

  async function openNotes() {
    await refreshNotes();
    setNotesModalOpen(true);
  }

  async function refreshAllBalances(force = false) {
    const now = Date.now();

    if (balancesInFlight.current) return;
    if (!force && typeof document !== 'undefined' && document.hidden) return;
    if (!force && now - lastAllBalancesRefresh.current < ALL_BALANCES_REFRESH_MS) return;
    if (accounts.length === 0) return;

    balancesInFlight.current = true;
    setRefreshingBalances(true);

    try {
      const publicKeys = accounts.map((account) => account.publicKey);
      const balances = await getNativeBalances(publicKeys);

      lastAllBalancesRefresh.current = Date.now();

      await Promise.all(
        accounts.map((account) =>
          db.accounts.update(account.id, {
            balance: balances[account.publicKey] ?? '0 XLM',
            updatedAt: Date.now(),
          }),
        ),
      );

      setAccounts((prev) =>
        prev.map((account) => ({
          ...account,
          balance: balances[account.publicKey] ?? account.balance ?? '0 XLM',
        })),
      );
    } catch (e) {
      setError({ message: `failed to refresh balances: ${String(e)}` });
    } finally {
      balancesInFlight.current = false;
      setRefreshingBalances(false);
    }
  }

  async function handleUnlocked(result: { identityPublicKey: string; accounts: PublicAccount[] }) {
    setIdentityPublicKey(result.identityPublicKey);
    setMixerIdentityAddress(result.identityPublicKey);

    backend.identityAddress?.()
      .then(setMixerIdentityAddress)
      .catch(() => setMixerIdentityAddress(result.identityPublicKey));

    const existingAccountRows = await db.accounts.toArray();
    const roleById = new Map<string, AccountRole>(
      existingAccountRows.map((account) => [account.id, account.role ?? 'all']),
    );
    const hydratedAccounts = result.accounts.map((account) => ({
      ...account,
      role: roleById.get(account.id) ?? 'all' as AccountRole,
    }));

    await db.accounts.clear();
    await upsertAccounts(hydratedAccounts);

    setAccounts(hydratedAccounts.map((account) => ({ ...account })));
    setSelectedAccountId(hydratedAccounts[0]?.id);
    setError(undefined);

    setVaultStatus({ exists: true, unlocked: true });

    await refreshPrivateBalance();
    await refreshNotes();
    setTimeout(() => refreshAllBalances(true), 250);
  }

  async function handleSelectAccount(id: string) {
    setSelectedAccountId(id);

    if (!error?.accountId || error.accountId === id) {
      setError(undefined);
    }

    clearAccountFlashStatusAfterClick(id);
  }

  async function handleCreateAccount(name: string) {
    setError(undefined);

    const account = { ...(await backend.createAccount(name)), role: 'all' as AccountRole };

    await upsertAccounts([account]);

    setAccounts((prev) => [...prev.filter((item) => item.id !== account.id), account]);
    setSelectedAccountId(account.id);

    await addActivity({
      accountId: account.id,
      kind: 'system',
      title: `Created account ${name}`,
      detail: account.publicKey,
      createdAt: Date.now(),
      status: 'success',
    });

    setTimeout(() => refreshAllBalances(true), 250);
    await refreshPrivateBalance();
    await refreshNotes();
  }

  async function handleSetAccountRole(accountId: string, role: AccountRole) {
    const normalizedRole = role ?? 'all';

    await updateAccountRole(accountId, normalizedRole);

    setAccounts((prev) =>
      prev.map((account) =>
        account.id === accountId ? { ...account, role: normalizedRole } : account,
      ),
    );
  }

  function blockIfAccountRoleDisallows(
    account: AccountView,
    operation: 'deposit' | 'withdraw' | 'transfer',
  ): boolean {
    if (accountCanRunOperation(account, operation)) return false;

    const msg = `${account.name} is marked ${roleDisplayName(accountRole(account))}; ${operationDisplayName(operation)} is disabled for this account. Change the account group to All or ${operationDisplayName(operation)} to use it here.`;

    setError({ accountId: account.id, message: msg });
    setAccountFlashStatus(account.id, 'role blocked', 'error');

    return true;
  }

  async function handleDeposit(amount: string) {
    if (!selectedAccount) return;
    if (blockIfAccountRoleDisallows(selectedAccount, 'deposit')) return;

    const account = selectedAccount;

    beginOperation();
    startAccountOp(account.id, 'deposit');
    setError(undefined);

    await addActivity({
      accountId: account.id,
      kind: 'deposit',
      amount,
      title: 'Deposit started',
      detail: 'Creates a note in the identity-wide private pool',
      createdAt: Date.now(),
      status: 'pending',
    });

    try {
      const result = await backend.deposit(account.id, amount);

      await addActivity({
        accountId: account.id,
        kind: 'deposit',
        amount,
        title: 'Deposit successful',
        detail: `identity note ${result.noteId}, leaf index ${result.leafIndex}`,
        txHash: result.txHash,
        createdAt: Date.now(),
        status: 'success',
        operationId: result.txHash,
        accountName: account.name,
        accountPublicKey: account.publicKey,
        noteId: result.noteId,
        outputNoteIds: [result.noteId],
        leafIndex: result.leafIndex,
        leafHex: result.leafHex,
      });

      setAccountFlashStatus(account.id, 'deposited', 'success');

      await refreshAllBalances(true);
      await refreshPrivateBalance();
      await refreshNotes();
      backend.treePirStatus().then(setStats).catch(() => undefined);
    } catch (e) {
      const msg = friendlyError(e);
      setError({ accountId: account.id, message: msg });
      setAccountFlashStatus(account.id, 'error', 'error');

      await addActivity({
        accountId: account.id,
        kind: 'error',
        title: 'Deposit failed',
        detail: msg,
        createdAt: Date.now(),
        status: 'error',
      });
    } finally {
      finishAccountOp(account.id, 'deposit');
      endOperation();
    }
  }

  async function handleWithdraw(amount: string, recipientAddress?: string) {
    if (!selectedAccount || spendBusy) return;
    if (blockIfAccountRoleDisallows(selectedAccount, 'withdraw')) return;

    const recipientReserveError = await withdrawRecipientReserveErrorForAddress(
      selectedAccount,
      amount,
      recipientAddress,
      accounts,
    );

    if (recipientReserveError) {
      setError({ accountId: selectedAccount.id, message: recipientReserveError });
      setAccountFlashStatus(selectedAccount.id, 'error', 'error');

      await addActivity({
        accountId: selectedAccount.id,
        kind: 'error',
        title: 'Withdraw blocked',
        detail: recipientReserveError,
        createdAt: Date.now(),
        status: 'error',
      });

      return;
    }

    const selectedHasFeeBalance = parseXlmBalance(selectedAccount.balance) >= MIN_FEE_PAYER_XLM;
    const otherFeePayers = accounts.filter(
      (account) =>
        account.id !== selectedAccount.id &&
        accountCanRunOperation(account, 'withdraw') &&
        parseXlmBalance(account.balance) >= MIN_FEE_PAYER_XLM,
    );

    if (!selectedHasFeeBalance && otherFeePayers.length > 0) {
      setPendingFeePayerWithdraw({
        recipientAccountId: selectedAccount.id,
        amount,
        recipientAddress: normalizeOptionalStellarAddress(recipientAddress),
      });
      return;
    }

    await runWithdraw(selectedAccount.id, amount, undefined, recipientAddress);
  }

  async function runWithdraw(
    recipientAccountId: string,
    amount: string,
    feePayerAccountId?: string,
    recipientAddress?: string,
  ) {
    const recipient = accounts.find((account) => account.id === recipientAccountId);
    const feePayer = feePayerAccountId
      ? accounts.find((account) => account.id === feePayerAccountId)
      : recipient;

    if (!recipient) return;

    if (feePayer && !accountCanRunOperation(feePayer, 'withdraw')) {
      const msg = `fee payer account is marked ${roleDisplayName(accountRole(feePayer))}; withdraw fee payment is disabled for it.`;

      setError({ accountId: recipient.id, message: msg });
      setAccountFlashStatus(recipient.id, 'role blocked', 'error');

      return;
    }

    const normalizedRecipientAddress = normalizeOptionalStellarAddress(recipientAddress);

    beginOperation();
    setSpendBusy(true);
    startAccountOp(recipient.id, 'withdraw');
    setError(undefined);
    setPendingFeePayerWithdraw(undefined);

    await addActivity({
      accountId: recipient.id,
      kind: 'withdraw',
      amount,
      title: 'Withdraw started',
      detail: withdrawStartedDetail(recipient.name, feePayer?.name, normalizedRecipientAddress),
      createdAt: Date.now(),
      status: 'pending',
    });

    try {
      const result = await backend.withdraw(
        recipient.id,
        amount,
        feePayerAccountId,
        normalizedRecipientAddress,
      );

      await addActivity({
        accountId: recipient.id,
        kind: 'withdraw',
        amount,
        title: 'Withdraw successful',
        detail: `spent ${result.spentNoteIds.length} identity note(s)`,
        txHash: result.txHash,
        createdAt: Date.now(),
        status: 'success',
        operationId: result.txHash,
        accountName: recipient.name,
        accountPublicKey: recipient.publicKey,
        inputNoteIds: result.spentNoteIds,
        spentNoteIds: result.spentNoteIds,
        outputNoteIds: result.createdNoteId ? [result.createdNoteId] : [],
        createdNoteIds: result.createdNoteId ? [result.createdNoteId] : [],
        createdNoteId: result.createdNoteId,
        recipientAddress: normalizedRecipientAddress ?? recipient.publicKey,
        feePayerAccountId,
        feePayerName: feePayer?.name,
        feePayerPublicKey: feePayer?.publicKey,
      });

      setAccountFlashStatus(recipient.id, 'withdrawn', 'success');

      await refreshAllBalances(true);
      await refreshPrivateBalance();
      await refreshNotes();
      backend.treePirStatus().then(setStats).catch(() => undefined);
    } catch (e) {
      const msg = friendlyError(e);
      setError({ accountId: recipient.id, message: msg });
      setAccountFlashStatus(recipient.id, 'error', 'error');

      await addActivity({
        accountId: recipient.id,
        kind: 'error',
        title: 'Withdraw failed',
        detail: msg,
        createdAt: Date.now(),
        status: 'error',
      });

      await refreshPrivateBalance();
      await refreshNotes();
      await refreshAllBalances(true);
    } finally {
      finishAccountOp(recipient.id, 'withdraw');
      setSpendBusy(false);
      endOperation();
    }
  }

  async function handleTransfer(amount: string, recipientIdentity: string, message?: string) {
    if (!selectedAccount || spendBusy) return;
    if (blockIfAccountRoleDisallows(selectedAccount, 'transfer')) return;

    const selectedHasFeeBalance = parseXlmBalance(selectedAccount.balance) >= MIN_FEE_PAYER_XLM;
    const otherFeePayers = accounts.filter(
      (account) =>
        account.id !== selectedAccount.id &&
        accountCanRunOperation(account, 'transfer') &&
        parseXlmBalance(account.balance) >= MIN_FEE_PAYER_XLM,
    );

    if (!selectedHasFeeBalance && otherFeePayers.length > 0) {
      setPendingFeePayerTransfer({
        senderAccountId: selectedAccount.id,
        amount,
        recipientIdentity,
        message,
      });
      return;
    }

    await runTransfer(selectedAccount.id, amount, recipientIdentity, message);
  }

  async function runTransfer(
    senderAccountId: string,
    amount: string,
    recipientIdentity: string,
    message?: string,
    feePayerAccountId?: string,
  ) {
    const account = accounts.find((candidate) => candidate.id === senderAccountId);
    const feePayer = feePayerAccountId
      ? accounts.find((candidate) => candidate.id === feePayerAccountId)
      : account;

    if (!account) return;

    if (feePayer && !accountCanRunOperation(feePayer, 'transfer')) {
      const msg = `fee payer account is marked ${roleDisplayName(accountRole(feePayer))}; transfer fee payment is disabled for it.`;

      setError({ accountId: account.id, message: msg });
      setAccountFlashStatus(account.id, 'role blocked', 'error');

      return;
    }

    beginOperation();
    setSpendBusy(true);
    startAccountOp(account.id, 'transfer');
    setError(undefined);
    setPendingFeePayerTransfer(undefined);

    await addActivity({
      accountId: account.id,
      kind: 'transfer',
      amount,
      title: 'Transfer started',
      detail: transferStartedDetail(account.name, feePayer?.name, recipientIdentity, message),
      createdAt: Date.now(),
      status: 'pending',
    });

    try {
      const result = await backend.transfer(
        account.id,
        amount,
        recipientIdentity,
        message,
        feePayerAccountId,
      );

      const externalOutputId = externalTransferOutputId(
        result.txHash,
        result.recipientIdentity ?? recipientIdentity,
      );

      await addActivity({
        accountId: account.id,
        kind: 'transfer',
        amount,
        title: 'Transfer successful',
        detail: `spent ${result.spentNoteIds.length} identity note(s)`,
        txHash: result.txHash,
        createdAt: Date.now(),
        status: 'success',
        operationId: result.txHash,
        accountName: account.name,
        accountPublicKey: account.publicKey,
        inputNoteIds: result.spentNoteIds,
        spentNoteIds: result.spentNoteIds,
        outputNoteIds: [externalOutputId, ...result.createdNoteIds],
        createdNoteIds: result.createdNoteIds,
        recipientIdentity: result.recipientIdentity ?? recipientIdentity,
        feePayerAccountId,
        feePayerName: feePayer?.name,
        feePayerPublicKey: feePayer?.publicKey,
        message,
      });

      setAccountFlashStatus(account.id, 'transferred', 'success');

      await refreshAllBalances(true);
      await refreshPrivateBalance();
      await refreshNotes();
      backend.treePirStatus().then(setStats).catch(() => undefined);
    } catch (e) {
      const msg = friendlyError(e);
      setError({ accountId: account.id, message: msg });
      setAccountFlashStatus(account.id, 'error', 'error');

      await addActivity({
        accountId: account.id,
        kind: 'error',
        title: 'Transfer failed',
        detail: msg,
        createdAt: Date.now(),
        status: 'error',
      });

      await refreshPrivateBalance();
      await refreshNotes();
      await refreshAllBalances(true);
    } finally {
      finishAccountOp(account.id, 'transfer');
      setSpendBusy(false);
      endOperation();
    }
  }

  const selectedActivity = useMemo(() => {
    if (!selectedAccountId) return [];
    return activity.filter((item) => item.accountId === selectedAccountId);
  }, [activity, selectedAccountId]);

  const selectedProgress = useMemo(() => {
    if (!selectedAccountId) return [];
    return progress.filter((item) => item.accountId === selectedAccountId);
  }, [progress, selectedAccountId]);

  const visibleError = error && (!error.accountId || error.accountId === selectedAccountId) ? error.message : undefined;

  if (!vaultStatus.unlocked) {
    return <SetupUnlock exists={vaultStatus.exists} onUnlocked={handleUnlocked} />;
  }

  return (
    <main className="app-shell">
      <AccountSidebar
        accounts={accounts}
        selectedAccountId={selectedAccountId}
        refreshingBalances={refreshingBalances}
        activeAccountOps={activeAccountOps}
        accountStatuses={accountStatuses}
        onSetAccountRole={handleSetAccountRole}
        onSelect={handleSelectAccount}
        onCreate={handleCreateAccount}
        onRefreshBalances={() => refreshAllBalances(true)}
      />

      <section className="workspace">
        <MixerHeader
          identityPublicKey={mixerIdentityAddress ?? identityPublicKey}
          stats={stats}
          privateBalance={privateBalance}
          totalNoteCount={notes.length}
          onOpenNotes={openNotes}
        />

        {incomingTransferNotes.length > 0 && (
          <section className="incoming-note-panel">
            <div className="incoming-note-header">
              <strong>New private note received</strong>
              <span>{incomingTransferNotes.length}</span>
            </div>

            {incomingTransferNotes.map((note) => (
              <article key={note.id} className="incoming-note-card">
                <div>
                  <strong>{note.amountXlm}</strong>
                  <p>{incomingTransferNoteDetail(note)}</p>
                </div>

                <div className="incoming-note-actions">
                  <button className="incoming-note-view-button" onClick={openNotes}>
                    View notes
                  </button>
                  <button
                    className="incoming-note-ok-button"
                    onClick={() =>
                      setIncomingTransferNotes((prev) =>
                        prev.filter((item) => item.id !== note.id),
                      )
                    }
                  >
                    OK
                  </button>
                </div>
              </article>
            ))}
          </section>
        )}

        {pendingFeePayerWithdraw && (
          <div className="fee-payer-panel">
            <div>
              <strong>Selected recipient account does not seem to have enough XLM for transaction fees.</strong>
              <p>
                Choose another funded account to pay the Stellar transaction fee. The withdraw recipient remains the selected account.
              </p>
            </div>

            <div className="fee-payer-list">
              {feePayerCandidates.length === 0 && (
                <button disabled>No funded fee payer found. Fund one account or refresh balances.</button>
              )}

              {feePayerCandidates.map((account) => (
                <button
                  key={account.id}
                  onClick={() =>
                    runWithdraw(
                      pendingFeePayerWithdraw.recipientAccountId,
                      pendingFeePayerWithdraw.amount,
                      account.id,
                      pendingFeePayerWithdraw.recipientAddress,
                    )
                  }
                >
                  Pay fee with {account.name} · {account.balance ?? 'unknown'}
                </button>
              ))}

              <button className="ghost-button" onClick={() => setPendingFeePayerWithdraw(undefined)}>
                Cancel
              </button>
            </div>
          </div>
        )}


        {pendingFeePayerTransfer && (
          <div className="fee-payer-panel">
            <div>
              <strong>Selected transfer account does not seem to have enough XLM for transaction fees.</strong>
              <p>
                Choose another funded account to pay the Stellar transaction fee. Private input notes still belong to the selected Mixer Identity.
              </p>
            </div>

            <div className="fee-payer-list">
              {transferFeePayerCandidates.length === 0 && (
                <button disabled>No funded transfer fee payer found. Fund one account or refresh balances.</button>
              )}

              {transferFeePayerCandidates.map((account) => (
                <button
                  key={account.id}
                  onClick={() =>
                    runTransfer(
                      pendingFeePayerTransfer.senderAccountId,
                      pendingFeePayerTransfer.amount,
                      pendingFeePayerTransfer.recipientIdentity,
                      pendingFeePayerTransfer.message,
                      account.id,
                    )
                  }
                >
                  Pay fee with {account.name} · {account.balance ?? 'unknown'}
                </button>
              ))}

              <button className="ghost-button" onClick={() => setPendingFeePayerTransfer(undefined)}>
                Cancel
              </button>
            </div>
          </div>
        )}

        <OperationTabs
          selectedAccountId={selectedAccountId}
          disabled={!selectedAccount}
          spendBusy={spendBusy}
          allowedOperations={selectedAllowedOperations}
          onDeposit={handleDeposit}
          onWithdraw={handleWithdraw}
          onTransfer={handleTransfer}
        />

        <section className="activity-tabs-shell">
          <header className="activity-tabs-header">
            <button
              className={historyTab === 'operations' ? 'active' : ''}
              onClick={() => setHistoryTab('operations')}
            >
              Operations
            </button>

            <button
              className={historyTab === 'raw' ? 'active' : ''}
              onClick={() => setHistoryTab('raw')}
            >
              History & backend progress
            </button>
          </header>

          {historyTab === 'operations' ? (
            <OperationHistory
              activity={selectedActivity}
              notes={notes}
              accounts={accounts}
              selectedAccountId={selectedAccountId}
              mixerIdentityAddress={mixerIdentityAddress ?? identityPublicKey}
            />
          ) : (
            <div className="bottom-grid">
              <ActivityFeed activity={selectedActivity} />
              <ProgressLog progress={selectedProgress} />
            </div>
          )}
        </section>

        {notesModalOpen && <NotesModal notes={notes} onClose={() => setNotesModalOpen(false)} />}

        {visibleError && (
          <ErrorModal
            message={visibleError}
            onClose={() => setError(undefined)}
          />
        )}
      </section>
    </main>
  );
}


const EXTERNAL_TRANSFER_OUTPUT_PREFIX = 'external-transfer-recipient-note:';

function externalTransferOutputId(txHash: string, recipientIdentity: string): string {
  return `${EXTERNAL_TRANSFER_OUTPUT_PREFIX}${txHash}:${recipientIdentity}`;
}

function incomingTransferNoteDetail(note: NoteView): string {
  const parts = [`note ${shortNoteId(note.id)}`];

  if (note.leafIndex !== undefined) {
    parts.push(`leaf ${note.leafIndex}`);
  }

  if (note.sourceLedger !== undefined) {
    parts.push(`ledger ${note.sourceLedger}`);
  }

  if (note.message) {
    parts.push(`message: ${note.message}`);
  }

  return parts.join(' · ');
}

function shortNoteId(value: string): string {
  if (value.length <= 18) return value;
  return `${value.slice(0, 8)}…${value.slice(-8)}`;
}

function transferStartedDetail(
  selectedAccountName: string,
  feePayerName: string | undefined,
  recipientIdentity: string,
  message: string | undefined,
): string {
  const feeText = feePayerName && feePayerName !== selectedAccountName
    ? `fee payer: ${feePayerName}`
    : 'selected account pays fee';

  const messageText = message ? ` · message: ${message}` : '';

  return `Recipient Mixer Identity: ${recipientIdentity.slice(0, 18)}…; ${feeText}${messageText}`;
}


type OperationKind = 'deposit' | 'withdraw' | 'transfer';

function accountRole(account?: Pick<AccountView, 'role'>): AccountRole {
  return account?.role ?? 'all';
}

function accountCanRunOperation(account: Pick<AccountView, 'role'>, operation: OperationKind): boolean {
  const role = accountRole(account);

  return role === 'all' || role === operation;
}

function operationPermissionsForRole(role: AccountRole): Record<OperationKind, boolean> {
  return {
    deposit: role === 'all' || role === 'deposit',
    withdraw: role === 'all' || role === 'withdraw',
    transfer: role === 'all' || role === 'transfer',
  };
}

function roleDisplayName(role: AccountRole): string {
  if (role === 'all') return 'All';
  if (role === 'deposit') return 'Deposit only';
  if (role === 'withdraw') return 'Withdraw only';
  return 'Transfer only';
}

function operationDisplayName(operation: OperationKind): string {
  if (operation === 'deposit') return 'Deposit';
  if (operation === 'withdraw') return 'Withdraw';
  return 'Transfer';
}

function parseXlmBalance(value?: string): number {
  if (!value) return 0;

  const normalized = value.replace('XLM', '').trim();
  const parsed = Number(normalized);

  return Number.isFinite(parsed) ? parsed : 0;
}





async function withdrawRecipientReserveErrorForAddress(
  selectedAccount: AccountView,
  amountText: string,
  recipientAddressText: string | undefined,
  accounts: AccountView[],
): Promise<string | null> {
  const recipientAddress = normalizeOptionalStellarAddress(recipientAddressText);

  if (recipientAddress && !isLikelyStellarPublicKey(recipientAddress)) {
    return 'recipient address must be a Stellar public address starting with G.';
  }

  let balanceText = selectedAccount.balance;

  if (recipientAddress && recipientAddress !== selectedAccount.publicKey) {
    const known = accounts.find((account) => account.publicKey === recipientAddress);
    balanceText = known?.balance;

    if (!known) {
      try {
        const balances = await getNativeBalances([recipientAddress]);
        balanceText = balances[recipientAddress] ?? '0 XLM';
      } catch {
        balanceText = '0 XLM';
      }
    }
  }

  return withdrawRecipientReserveErrorForBalance(balanceText, amountText);
}

function withdrawRecipientReserveErrorForBalance(balanceText: string | undefined, amountText: string): string | null {
  const amount = parseXlmInputForReserveCheck(amountText);

  if (!Number.isFinite(amount) || amount <= 0) {
    return null;
  }

  const currentBalance = parseXlmBalanceForReserveCheck(balanceText);
  const balanceAfterWithdraw = currentBalance + amount;
  const minimumBalance = ACCOUNT_MIN_RESERVE_XLM;

  if (balanceAfterWithdraw + 0.0000001 < minimumBalance) {
    const needed = Math.max(0, minimumBalance - currentBalance);

    return [
      'recipient account would end below Stellar minimum balance.',
      `Current recipient balance: ${formatReserveXlm(currentBalance)} XLM.`,
      `Withdraw amount: ${formatReserveXlm(amount)} XLM.`,
      `Minimum required recipient balance after withdraw: ${formatReserveXlm(minimumBalance)} XLM.`,
      `Withdraw at least ${formatReserveXlm(needed)} XLM to this recipient, or fund the recipient first.`
    ].join(' ');
  }

  return null;
}

function normalizeOptionalStellarAddress(value: string | undefined): string | undefined {
  const trimmed = value?.trim();

  return trimmed ? trimmed : undefined;
}

function isLikelyStellarPublicKey(value: string): boolean {
  return /^G[A-Z2-7]{55}$/.test(value);
}

function withdrawStartedDetail(
  selectedRecipientName: string,
  feePayerName: string | undefined,
  recipientAddress: string | undefined,
): string {
  const feeText = feePayerName && feePayerName !== selectedRecipientName
    ? `fee payer: ${feePayerName}`
    : 'selected account pays fee';

  if (recipientAddress) {
    return `Recipient address: ${recipientAddress.slice(0, 12)}…${recipientAddress.slice(-8)}; ${feeText}`;
  }

  return `Recipient: ${selectedRecipientName}; ${feeText}`;
}

function parseXlmInputForReserveCheck(value: string): number {
  return Number(value.trim().replace(',', '.'));
}

function parseXlmBalanceForReserveCheck(value: string | undefined): number {
  if (!value) return 0;

  const normalized = value.replace('XLM', '').trim().replace(',', '.');
  const parsed = Number(normalized);

  return Number.isFinite(parsed) ? parsed : 0;
}

function formatReserveXlm(value: number): string {
  if (!Number.isFinite(value)) return '0';

  return value.toFixed(7).replace(/\.?0+$/, '');
}

function friendlyError(error: unknown): string {
  const raw = String(error);
  const lower = raw.toLowerCase();

  if (lower.includes('resulting balance is not within the allowed range')) {
    return 'not enough public XLM for this operation: the token transfer would violate Stellar balance/reserve/fee limits. Try a smaller amount or fund this account.';
  }

  if (
    lower.includes('simulationfailed') &&
    lower.includes('contract call failed') &&
    lower.includes('transfer')
  ) {
    return 'transaction simulation failed during token transfer. The selected account probably cannot spend this amount after reserve and fees. Try a smaller amount or fund the account.';
  }

  if (lower.includes('simulationfailed')) {
    const code = raw.match(/Error\(Contract,\s*#(\d+)\)/)?.[1];

    if (code) {
      return `transaction simulation failed with contract error #${code}. The operation was rejected before submit.`;
    }

    return 'transaction simulation failed before submit. The operation was rejected by Stellar RPC.';
  }

  if (raw.includes('WaitTransactionTimeout') && raw.includes('NotFound')) {
    return 'transaction was submitted but Stellar RPC did not find it before timeout. Refresh balances and retry; the RPC node may have dropped or not indexed the transaction.';
  }

  if (raw.includes('AccountError') || raw.includes('source account') || raw.includes('not funded')) {
    return 'source account is not available or not funded enough on Stellar RPC. Refresh balances, fund this account, or choose another funded account as fee payer.';
  }

  if (lower.includes('insufficient') || lower.includes('underfunded')) {
    return 'not enough XLM for this operation after accounting for account reserve and transaction fees.';
  }

  if (raw.length > 420) {
    const code = raw.match(/Error\(Contract,\s*#(\d+)\)/)?.[1];

    if (code) {
      return `operation failed with contract error #${code}. Open developer logs for full diagnostic details.`;
    }

    return `${raw.slice(0, 360)}…`;
  }

  return raw;
}

