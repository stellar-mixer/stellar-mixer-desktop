import { useMemo, useState } from 'react';
import type { AccountView, ActivityItem, NoteView } from '../lib/types';

type OperationKind = 'deposit' | 'withdraw' | 'transfer';
type OperationFilter = 'all' | OperationKind;
type NoteSide = 'inputs' | 'outputs';

type OperationActivity = ActivityItem & {
  kind: OperationKind;
  status: 'success';
};

type SelectedNoteRef = {
  noteId: string;
  note?: NoteView;
  side: NoteSide;
  externalRecipientOutput: boolean;
  resolvedSelfRecipientOutput: boolean;
  operation: OperationActivity;
};

const OPERATION_KINDS: OperationKind[] = ['deposit', 'withdraw', 'transfer'];
const EXTERNAL_TRANSFER_OUTPUT_PREFIX = 'external-transfer-recipient-note:';

export function OperationHistory({
  activity,
  notes,
  accounts,
  selectedAccountId,
  mixerIdentityAddress,
}: {
  activity: ActivityItem[];
  notes: NoteView[];
  accounts: AccountView[];
  selectedAccountId?: string;
  mixerIdentityAddress?: string;
}) {
  const [filter, setFilter] = useState<OperationFilter>('all');
  const [selectedNoteRef, setSelectedNoteRef] = useState<SelectedNoteRef>();

  const notesById = useMemo(() => {
    return new Map(notes.map((note) => [note.id, note]));
  }, [notes]);

  const accountsById = useMemo(() => {
    return new Map(accounts.map((account) => [account.id, account]));
  }, [accounts]);

  const operations = useMemo(() => {
    return activity
      .filter(isSuccessfulOperation)
      .map(normalizeOperation)
      .filter((operation) => filter === 'all' || operation.kind === filter)
      .sort((a, b) => b.createdAt - a.createdAt);
  }, [activity, filter]);

  const counters = useMemo(() => {
    const successful = activity.filter(isSuccessfulOperation);

    return {
      all: successful.length,
      deposit: successful.filter((item) => item.kind === 'deposit').length,
      withdraw: successful.filter((item) => item.kind === 'withdraw').length,
      transfer: successful.filter((item) => item.kind === 'transfer').length,
    };
  }, [activity]);

  return (
    <section className="op-history-panel">
      <header className="op-history-header">
        <div>
          <div className="op-history-kicker">Successful operations</div>
          <h3>Account operation log</h3>
          <p>
            Clear operation view built from successful local actions. Raw logs stay in the second tab.
          </p>
        </div>

        <div className="op-history-filter-tabs" role="tablist" aria-label="Operation filter">
          <FilterButton active={filter === 'all'} label="All" count={counters.all} onClick={() => setFilter('all')} />
          <FilterButton active={filter === 'deposit'} label="Deposit" count={counters.deposit} onClick={() => setFilter('deposit')} />
          <FilterButton active={filter === 'withdraw'} label="Withdraw" count={counters.withdraw} onClick={() => setFilter('withdraw')} />
          <FilterButton active={filter === 'transfer'} label="Transfer" count={counters.transfer} onClick={() => setFilter('transfer')} />
        </div>
      </header>

      <div className="op-history-list pretty-scroll">
        {operations.length === 0 && (
          <div className="op-history-empty">
            <strong>No successful {filter === 'all' ? 'operations' : filter} yet.</strong>
            <span>Successful Deposit, Withdraw and Transfer actions will appear here.</span>
          </div>
        )}

        {operations.map((operation) => {
          const account = operation.accountId ? accountsById.get(operation.accountId) : undefined;

          return (
            <OperationCard
              key={operation.id ?? `${operation.kind}-${operation.txHash ?? operation.createdAt}`}
              operation={operation}
              account={account}
              selectedAccountId={selectedAccountId}
              notes={notes}
              notesById={notesById}
              mixerIdentityAddress={mixerIdentityAddress}
              onOpenNote={(noteId, note, side, externalRecipientOutput, resolvedSelfRecipientOutput) =>
                setSelectedNoteRef({
                  noteId,
                  note,
                  side,
                  externalRecipientOutput,
                  resolvedSelfRecipientOutput,
                  operation,
                })
              }
            />
          );
        })}
      </div>

      {selectedNoteRef && (
        <NoteDetailsModal
          noteId={selectedNoteRef.noteId}
          note={selectedNoteRef.note}
          side={selectedNoteRef.side}
          externalRecipientOutput={selectedNoteRef.externalRecipientOutput}
          resolvedSelfRecipientOutput={selectedNoteRef.resolvedSelfRecipientOutput}
          operation={selectedNoteRef.operation}
          onClose={() => setSelectedNoteRef(undefined)}
        />
      )}
    </section>
  );
}

function FilterButton({
  active,
  label,
  count,
  onClick,
}: {
  active: boolean;
  label: string;
  count: number;
  onClick: () => void;
}) {
  return (
    <button className={active ? 'active' : ''} onClick={onClick}>
      <span>{label}</span>
      <em>{count}</em>
    </button>
  );
}

function OperationCard({
  operation,
  account,
  selectedAccountId,
  notes,
  notesById,
  mixerIdentityAddress,
  onOpenNote,
}: {
  operation: OperationActivity;
  account?: AccountView;
  selectedAccountId?: string;
  notes: NoteView[];
  notesById: Map<string, NoteView>;
  mixerIdentityAddress?: string;
  onOpenNote: (
    noteId: string,
    note: NoteView | undefined,
    side: NoteSide,
    externalRecipientOutput: boolean,
    resolvedSelfRecipientOutput: boolean,
  ) => void;
}) {
  const inputNoteIds = operationInputNoteIds(operation);
  const outputNoteIds = operationOutputNoteIds(operation);
  const [noteSide, setNoteSide] = useState<NoteSide>(inputNoteIds.length > 0 ? 'inputs' : 'outputs');

  const activeIds = noteSide === 'inputs' ? inputNoteIds : outputNoteIds;
  const accountName = operation.accountName ?? account?.name ?? 'Unknown account';
  const accountPublicKey = operation.accountPublicKey ?? account?.publicKey;
  const isSelected = operation.accountId && selectedAccountId === operation.accountId;

  return (
    <article className={`op-card op-card-${operation.kind}`}>
      <div className="op-card-main">
        <div className="op-card-left">
          <div className="op-kind-row">
            <span className={`op-kind-pill ${operation.kind}`}>{operationLabel(operation.kind)}</span>
            <span className="op-time">{formatDateTime(operation.createdAt)}</span>
            {isSelected && <span className="op-selected-pill">selected account</span>}
          </div>

          <h4>{operationTitle(operation)}</h4>

          <div className="op-detail-grid">
            <Detail label="Amount" value={operation.amount ? `${operation.amount} XLM` : '—'} />
            <Detail label="Account" value={accountName} />
            {accountPublicKey && <Detail label="Public key" value={shortMiddle(accountPublicKey, 12, 8)} mono />}
            {operation.txHash && <Detail label="Tx hash" value={shortMiddle(operation.txHash, 14, 10)} mono />}
            {operation.leafIndex !== undefined && <Detail label="Leaf index" value={String(operation.leafIndex)} mono />}
            {operation.recipientAddress && <Detail label="Recipient address" value={shortMiddle(operation.recipientAddress, 12, 8)} mono />}
            {operation.recipientIdentity && <Detail label="Recipient identity" value={shortMiddle(operation.recipientIdentity, 16, 10)} mono />}
            {operation.feePayerName && <Detail label="Fee payer" value={operation.feePayerName} />}
            {operation.message && <Detail label="Private message" value={operation.message} wide />}
          </div>

          <div className="op-action-row">
            {operation.txHash && (
              <>
                <button onClick={() => copyText(operation.txHash!)}>Copy tx</button>
                <button onClick={() => openExplorer(operation.txHash!)}>Open explorer</button>
              </>
            )}

            {operation.leafHex && <button onClick={() => copyText(operation.leafHex!)}>Copy leaf</button>}
          </div>
        </div>

        <div className="op-notes-box">
          <div className="op-note-tabs" role="tablist" aria-label="Operation notes">
            <button className={noteSide === 'inputs' ? 'active' : ''} onClick={() => setNoteSide('inputs')}>
              Inputs <em>{inputNoteIds.length}</em>
            </button>
            <button className={noteSide === 'outputs' ? 'active' : ''} onClick={() => setNoteSide('outputs')}>
              Outputs <em>{outputNoteIds.length}</em>
            </button>
          </div>

          <div className="op-note-list pretty-scroll">
            {activeIds.length === 0 && (
              <div className="op-note-empty">
                No {noteSide === 'inputs' ? 'input' : 'output'} notes recorded for this operation.
              </div>
            )}

            {activeIds.map((noteId) => {
              const externalRecipientOutput = isExternalTransferOutputId(noteId);
              const resolvedSelfRecipientNote = externalRecipientOutput
                ? resolveSelfTransferRecipientNote(operation, notes, mixerIdentityAddress)
                : undefined;

              const note = notesById.get(noteId) ?? resolvedSelfRecipientNote;
              const resolvedSelfRecipientOutput = Boolean(externalRecipientOutput && resolvedSelfRecipientNote);

              return (
                <button
                  key={`${noteSide}-${noteId}`}
                  className={`op-note-chip ${
                    note
                      ? resolvedSelfRecipientOutput
                        ? 'self-recipient'
                        : ''
                      : externalRecipientOutput
                        ? 'external'
                        : 'missing'
                  }`}
                  onClick={() =>
                    onOpenNote(
                      noteId,
                      note,
                      noteSide,
                      externalRecipientOutput,
                      resolvedSelfRecipientOutput,
                    )
                  }
                >
                  <span className="op-note-chip-title">
                    {externalRecipientOutput
                      ? resolvedSelfRecipientOutput
                        ? 'Recipient output · self'
                        : 'Recipient output'
                      : noteSide === 'inputs'
                        ? 'Input note'
                        : 'Output note'}
                  </span>

                  <span className="op-note-chip-id">
                    {note
                      ? shortMiddle(note.id, 10, 6)
                      : externalRecipientOutput
                        ? 'encrypted for recipient'
                        : shortMiddle(noteId, 10, 6)}
                  </span>

                  {note ? (
                    <span className="op-note-chip-meta">
                      {note.amountXlm} XLM · {note.status}
                      {note.leafIndex !== undefined ? ` · #${note.leafIndex}` : ''}
                    </span>
                  ) : externalRecipientOutput ? (
                    <span className="op-note-chip-meta">
                      {operation.amount ?? 'unknown'} XLM · not decryptable by this identity
                    </span>
                  ) : (
                    <span className="op-note-chip-meta">not found in local notes list</span>
                  )}
                </button>
              );
            })}
          </div>
        </div>
      </div>
    </article>
  );
}

function NoteDetailsModal({
  noteId,
  note,
  side,
  externalRecipientOutput,
  resolvedSelfRecipientOutput,
  operation,
  onClose,
}: {
  noteId: string;
  note?: NoteView;
  side: NoteSide;
  externalRecipientOutput: boolean;
  resolvedSelfRecipientOutput: boolean;
  operation: OperationActivity;
  onClose: () => void;
}) {
  const extra = (note ?? {}) as NoteView & Record<string, any>;

  return (
    <div className="op-note-modal-backdrop" onMouseDown={onClose}>
      <section className="op-note-modal" onMouseDown={(event) => event.stopPropagation()}>
        <header className="op-note-modal-header">
          <div>
            <div className="op-history-kicker">
              {side === 'inputs' ? 'Input note' : 'Output note'} · {operationLabel(operation.kind)}
            </div>
            <h3>
              {note
                ? `${note.amountXlm} XLM`
                : externalRecipientOutput
                  ? `${operation.amount ?? 'unknown'} XLM recipient output`
                  : 'Unknown local note'}
            </h3>
          </div>

          <button onClick={onClose} aria-label="Close note details">×</button>
        </header>

        {externalRecipientOutput && resolvedSelfRecipientOutput && (
          <div className="op-note-success">
            This recipient output was addressed to this Mixer Identity and was decrypted from the archive as a local received note.
          </div>
        )}

        {externalRecipientOutput && !resolvedSelfRecipientOutput && (
          <div className="op-note-warning">
            This output note was created by your transfer, but it is encrypted for the recipient Mixer Identity.
            This local identity cannot decrypt it, so only public/local operation metadata is available here.
          </div>
        )}

        {!note && !externalRecipientOutput && (
          <div className="op-note-warning">
            This operation references the note id, but the note is not present in the current local notes summary.
            It may belong to another restored state, old local activity, or a pruned test vault.
          </div>
        )}

        <div className="op-note-detail-grid">
          {externalRecipientOutput && operation.recipientIdentity && (
            <Detail label="Recipient identity" value={operation.recipientIdentity} mono wide />
          )}
          {externalRecipientOutput && operation.txHash && (
            <Detail label="Transfer tx" value={operation.txHash} mono wide />
          )}
          <Detail label={externalRecipientOutput ? 'Output ref' : 'Note id'} value={note?.id ?? noteId} mono wide />
          {note && <Detail label="Amount" value={`${note.amountXlm} XLM`} />}
          {note && <Detail label="Amount stroops" value={note.amountStroops} mono />}
          {note?.leafIndex !== undefined && <Detail label="Leaf index" value={String(note.leafIndex)} mono />}
          {note && <Detail label="Status" value={note.status} />}
          {note?.sourceKind && <Detail label="Source kind" value={note.sourceKind} />}
          {note?.sourceLedger !== undefined && <Detail label="Source ledger" value={String(note.sourceLedger)} mono />}
          {note && <Detail label="Created" value={formatDateTime(note.createdAt)} />}
          {note?.spentAt !== undefined && <Detail label="Spent" value={formatDateTime(note.spentAt)} />}
          {note?.depositedByAccountName && <Detail label="Account name" value={note.depositedByAccountName} />}
          {note?.depositedByAccountId && <Detail label="Account id" value={note.depositedByAccountId} mono />}
          {note?.depositedByPublicKey && <Detail label="Account public key" value={note.depositedByPublicKey} mono wide />}
          {note?.message && <Detail label="Private message" value={note.message} wide />}

          {typeof extra.leafHex === 'string' && <Detail label="Leaf" value={extra.leafHex} mono wide />}
          {typeof extra.nullifierHex === 'string' && <Detail label="Nullifier" value={extra.nullifierHex} mono wide />}
          {typeof extra.eventId === 'string' && <Detail label="Archive event id" value={extra.eventId} mono wide />}
          {typeof extra.ledger === 'number' && <Detail label="Archive ledger" value={String(extra.ledger)} mono />}
        </div>

        <div className="op-note-modal-actions">
          <button onClick={() => copyText(note?.id ?? noteId)}>Copy note/ref id</button>
          {note?.depositedByPublicKey && <button onClick={() => copyText(note.depositedByPublicKey)}>Copy public key</button>}
          {typeof extra.leafHex === 'string' && <button onClick={() => copyText(extra.leafHex)}>Copy leaf</button>}
          {typeof extra.nullifierHex === 'string' && <button onClick={() => copyText(extra.nullifierHex)}>Copy nullifier</button>}
        </div>
      </section>
    </div>
  );
}

function Detail({
  label,
  value,
  mono = false,
  wide = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
  wide?: boolean;
}) {
  return (
    <div className={`op-detail ${wide ? 'wide' : ''}`}>
      <span>{label}</span>
      <strong className={mono ? 'mono' : ''}>{value}</strong>
    </div>
  );
}

function isSuccessfulOperation(item: ActivityItem): item is OperationActivity {
  return item.status === 'success' && OPERATION_KINDS.includes(item.kind as OperationKind);
}

function normalizeOperation(operation: OperationActivity): OperationActivity {
  if (operation.kind !== 'deposit') {
    return operation;
  }

  if (operation.outputNoteIds?.length || operation.noteId) {
    return operation;
  }

  const parsedNoteId = operation.detail?.match(/identity note\s+([^,\s]+)/i)?.[1];
  const parsedLeafIndex = operation.detail?.match(/leaf index\s+(\d+)/i)?.[1];

  return {
    ...operation,
    noteId: parsedNoteId,
    outputNoteIds: parsedNoteId ? [parsedNoteId] : operation.outputNoteIds,
    leafIndex: operation.leafIndex ?? (parsedLeafIndex ? Number(parsedLeafIndex) : undefined),
  };
}

function operationInputNoteIds(operation: OperationActivity): string[] {
  const ids = [
    ...(operation.inputNoteIds ?? []),
    ...(operation.spentNoteIds ?? []),
  ];

  return uniqueNonEmpty(ids);
}

function operationOutputNoteIds(operation: OperationActivity): string[] {
  const ids = [
    ...(operation.outputNoteIds ?? []),
    ...(operation.createdNoteIds ?? []),
    operation.noteId,
    operation.createdNoteId,
  ];

  if (
    operation.kind === 'transfer' &&
    operation.recipientIdentity &&
    !ids.some((value) => value && isExternalTransferOutputId(value))
  ) {
    ids.unshift(externalTransferOutputId(operation.txHash ?? String(operation.createdAt), operation.recipientIdentity));
  }

  return uniqueNonEmpty(ids);
}

function resolveSelfTransferRecipientNote(
  operation: OperationActivity,
  notes: NoteView[],
  mixerIdentityAddress?: string,
): NoteView | undefined {
  if (operation.kind !== 'transfer') return undefined;
  if (!operation.recipientIdentity || !mixerIdentityAddress) return undefined;
  if (!sameMixerIdentity(operation.recipientIdentity, mixerIdentityAddress)) return undefined;

  const operationAmount = normalizeXlmAmount(operation.amount);
  const operationMessage = operation.message?.trim();
  const expectedLeafIndex = expectedRecipientLeafIndex(operation, notes);
  const localOutputIds = new Set([
    ...(operation.inputNoteIds ?? []),
    ...(operation.spentNoteIds ?? []),
    ...(operation.createdNoteIds ?? []),
    operation.createdNoteId,
  ].filter(Boolean) as string[]);

  const candidates = notes
    .filter((note) => !localOutputIds.has(note.id))
    .filter((note) => normalizeXlmAmount(note.amountXlm) === operationAmount)
    .filter((note) => {
      if (operationMessage) {
        return note.message?.trim() === operationMessage;
      }

      return true;
    })
    .filter((note) => noteLooksLikeTransferReceived(note, operationMessage))
    .filter((note) => {
      if (expectedLeafIndex !== undefined) {
        return note.leafIndex === expectedLeafIndex;
      }

      return note.createdAt >= operation.createdAt - 10 * 60_000;
    })
    .sort((a, b) => {
      if (expectedLeafIndex !== undefined) {
        return Math.abs((a.leafIndex ?? -1) - expectedLeafIndex) - Math.abs((b.leafIndex ?? -1) - expectedLeafIndex);
      }

      return Math.abs(a.createdAt - operation.createdAt) - Math.abs(b.createdAt - operation.createdAt);
    });

  return candidates[0];
}

function expectedRecipientLeafIndex(operation: OperationActivity, notes: NoteView[]): number | undefined {
  for (const noteId of operation.createdNoteIds ?? []) {
    const note = notes.find((candidate) => candidate.id === noteId);

    if (note?.leafIndex !== undefined && note.leafIndex > 0) {
      return note.leafIndex - 1;
    }
  }

  return undefined;
}

function noteLooksLikeTransferReceived(note: NoteView, operationMessage?: string): boolean {
  const source = normalizeKind(note.sourceKind);

  if (source.includes('transferreceived')) return true;
  if (source.includes('transfer') && operationMessage && note.message?.trim() === operationMessage) return true;

  // Fallback for older/recovered notes where sourceKind was not saved,
  // but the private transfer message was decrypted from the recipient note.
  if (!source && operationMessage && note.message?.trim() === operationMessage) return true;

  return false;
}

function normalizeKind(value?: string): string {
  return (value ?? '').toLowerCase().replace(/[^a-z0-9]/g, '');
}

function sameMixerIdentity(left: string, right: string): boolean {
  return left.trim().toLowerCase() === right.trim().toLowerCase();
}

function normalizeXlmAmount(value?: string): string {
  if (!value) return '';

  const parsed = Number(value.replace('XLM', '').trim().replace(',', '.'));

  if (!Number.isFinite(parsed)) {
    return value.trim();
  }

  return parsed.toFixed(7).replace(/\.?0+$/, '');
}

function externalTransferOutputId(txHash: string, recipientIdentity: string): string {
  return `${EXTERNAL_TRANSFER_OUTPUT_PREFIX}${txHash}:${recipientIdentity}`;
}

function isExternalTransferOutputId(value: string): boolean {
  return value.startsWith(EXTERNAL_TRANSFER_OUTPUT_PREFIX);
}

function uniqueNonEmpty(values: Array<string | undefined>): string[] {
  return [...new Set(values.filter((value): value is string => Boolean(value?.trim())))];
}

function operationLabel(kind: OperationKind): string {
  if (kind === 'deposit') return 'Deposit';
  if (kind === 'withdraw') return 'Withdraw';
  return 'Transfer';
}

function operationTitle(operation: OperationActivity): string {
  if (operation.kind === 'deposit') {
    return `Deposited ${operation.amount ?? '—'} XLM into Mixer Identity`;
  }

  if (operation.kind === 'withdraw') {
    return `Withdrew ${operation.amount ?? '—'} XLM from Mixer Identity`;
  }

  return `Transferred ${operation.amount ?? '—'} XLM privately`;
}

function formatDateTime(value: number): string {
  if (!Number.isFinite(value)) return '—';

  return new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(new Date(value));
}

function shortMiddle(value: string, left = 10, right = 8): string {
  if (value.length <= left + right + 3) return value;

  return `${value.slice(0, left)}…${value.slice(-right)}`;
}

async function copyText(value: string) {
  try {
    await navigator.clipboard?.writeText(value);
  } catch {
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
}

function openExplorer(txHash: string) {
  const url = `https://stellar.expert/explorer/testnet/tx/${encodeURIComponent(txHash)}`;

  try {
    window.open(url, '_blank', 'noopener,noreferrer');
  } catch {
    void copyText(url);
  }
}
