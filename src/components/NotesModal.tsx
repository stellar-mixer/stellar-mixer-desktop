import { X } from 'lucide-react';
import type { NoteView } from '../lib/types';

export function NotesModal({
  notes,
  onClose,
}: {
  notes: NoteView[];
  onClose: () => void;
}) {
  const total = notes.length;
  const spendable = notes.filter((note) => note.status === 'spendable').length;

  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <section className="notes-modal" onMouseDown={(event) => event.stopPropagation()}>
        <header className="notes-modal-header">
          <div>
            <h3>Mixer Identity notes</h3>
            <p>
              {spendable} spendable / {total} total
            </p>
          </div>

          <button className="icon-button" onClick={onClose}>
            <X size={17} />
          </button>
        </header>

        <div className="notes-table">
          {notes.length === 0 && (
            <div className="empty-notes">No private notes yet.</div>
          )}

          {notes.map((note) => (
            <article key={note.id} className={`note-row ${note.status}`}>
              <div className="note-main">
                <strong>{note.amountXlm}</strong>
                <span className="note-status">{formatStatus(note.status)}</span>
              </div>

              <div className="note-meta">
                <span>Deposited by</span>
                <strong>{note.depositedByAccountName}</strong>
              </div>

              <div className="note-meta mono">
                <span>Account</span>
                <strong>{short(note.depositedByPublicKey)}</strong>
              </div>

              <div className="note-meta">
                <span>Leaf index</span>
                <strong>{note.leafIndex ?? 'pending'}</strong>
              </div>

              <div className="note-meta">
                <span>Created</span>
                <strong>{new Date(note.createdAt).toLocaleString()}</strong>
              </div>

              {note.spentAt && (
                <div className="note-meta">
                  <span>Spent</span>
                  <strong>{new Date(note.spentAt).toLocaleString()}</strong>
                </div>
              )}

              {note.message && (
                <div className="note-message-row">
                  <span>Private message</span>
                  <strong>{note.message}</strong>
                </div>
              )}
            </article>
          ))}
        </div>
      </section>
    </div>
  );
}

function short(value: string) {
  if (!value || value === 'unknown') return value;
  if (value.length <= 18) return value;
  return `${value.slice(0, 9)}…${value.slice(-9)}`;
}

function formatStatus(status: string) {
  if (status === 'pending-index') return 'pending index';
  return status;
}
