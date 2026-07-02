import { useEffect, useState } from 'react';

export function ActionPanel({
  title,
  tone,
  disabled,
  spendBusy,
  resetKey,
  onSubmit,
}: {
  title: string;
  tone: 'deposit' | 'withdraw';
  disabled?: boolean;
  spendBusy?: boolean;
  resetKey?: string;
  onSubmit: (amount: string) => Promise<void>;
}) {
  const [amount, setAmount] = useState('');

  useEffect(() => {
    setAmount('');
  }, [resetKey]);

  const isSpendPanel = tone === 'withdraw';
  const blocked = disabled || (isSpendPanel && spendBusy);
  const valid = isValidPositiveXlm(amount);

  async function submit() {
    const value = amount.trim().replace(',', '.');

    if (!valid || blocked) return;

    await onSubmit(value);
    setAmount('');
  }

  return (
    <section className={`action-panel ${tone} ${blocked ? 'panel-blocked' : ''}`}>
      <h3>{title}</h3>

      <div className="action-row">
        <input
          placeholder="Amount in XLM"
          value={amount}
          inputMode="decimal"
          autoComplete="off"
          spellCheck={false}
          disabled={blocked}
          onChange={(event) => {
            const next = event.currentTarget.value.replace(',', '.');
            if (/^\d*(\.\d{0,7})?$/.test(next)) {
              setAmount(next);
            }
          }}
          onKeyDown={(event) => {
            if (event.key === 'Enter') submit();
          }}
        />

        <button disabled={blocked || !valid} onClick={submit}>
          {title}
        </button>
      </div>
    </section>
  );
}

function isValidPositiveXlm(value: string): boolean {
  const normalized = value.trim().replace(',', '.');

  if (!/^\d+(\.\d{0,7})?$/.test(normalized)) return false;

  const numeric = Number(normalized);

  return Number.isFinite(numeric) && numeric > 0;
}
