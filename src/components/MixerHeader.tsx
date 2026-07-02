import { useState } from 'react';
import type { MixerStats, PrivateBalance } from '../lib/types';

export function MixerHeader({
  identityPublicKey,
  stats,
  privateBalance,
  totalNoteCount,
  onOpenNotes,
}: {
  identityPublicKey?: string;
  stats: MixerStats;
  privateBalance?: PrivateBalance;
  totalNoteCount: number;
  onOpenNotes: () => void;
}) {
  const [copied, setCopied] = useState(false);

  const fullIdentity = identityPublicKey ?? 'unknown';
  const activeNotes = privateBalance?.spendableNoteCount ?? 0;
  const totalNotes = Math.max(totalNoteCount, activeNotes);

  async function copyIdentity() {
    await copyText(fullIdentity);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1100);
  }

  return (
    <header className="mixer-header compact-mixer-header">
      <div className="identity-block">
        <span className="identity-label">Mixer Identity</span>

        <div className="identity-copy-row">
          <code className="identity-value compact" title={fullIdentity}>
            {shortIdentity(fullIdentity)}
          </code>

          <button
            className={`copy-address-button identity-copy-button ${copied ? 'copied' : ''}`}
            onClick={copyIdentity}
            title="Copy Mixer Identity"
            aria-label="Copy Mixer Identity"
          >
            {copied ? '✓' : '⧉'}
          </button>
        </div>
      </div>

      <button className="header-stat clickable-stat" onClick={onOpenNotes}>
        <span>Private balance</span>
        <strong>{privateBalance?.totalXlm ?? '0 XLM'}</strong>
        <em>View notes</em>
      </button>

      <button className="header-stat clickable-stat notes-stat" onClick={onOpenNotes}>
        <span>Active notes</span>
        <strong>
          {activeNotes}/{totalNotes}
        </strong>
        <em>Spendable / total</em>
      </button>

      <div className="header-stat">
        <span>Anonymity set</span>
        <strong>{stats.anonymitySet}</strong>
      </div>
    </header>
  );
}

function shortIdentity(value: string): string {
  if (value.length <= 38) return value;

  if (value.startsWith('smxid1:')) {
    return `${value.slice(0, 14)}…${value.slice(-14)}`;
  }

  return `${value.slice(0, 12)}…${value.slice(-12)}`;
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

  document.body.appendChild(textarea);
  textarea.focus();
  textarea.select();
  document.execCommand('copy');
  textarea.remove();
}
