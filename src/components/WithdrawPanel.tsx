import { useEffect, useState } from 'react';

export function WithdrawPanel({
  disabled,
  spendBusy,
  resetKey,
  onSubmit,
}: {
  disabled?: boolean;
  spendBusy?: boolean;
  resetKey?: string;
  onSubmit: (amount: string, recipientAddress?: string) => Promise<void>;
}) {
  const [amount, setAmount] = useState('');
  const [recipientAddress, setRecipientAddress] = useState('');

  useEffect(() => {
    setAmount('');
    setRecipientAddress('');
  }, [resetKey]);

  const blocked = disabled || spendBusy;
  const validAmount = isValidPositiveXlm(amount);
  const recipient = recipientAddress.trim();

  async function submit() {
    const value = amount.trim().replace(',', '.');

    if (!validAmount || blocked) return;

    await onSubmit(value, recipient || undefined);
    setAmount('');
    setRecipientAddress('');
  }

  return (
    <section className={`action-panel withdraw ${blocked ? 'panel-blocked' : ''}`}>
      <div className="withdraw-fields">
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

        <input
          placeholder="Recipient address, optional"
          value={recipientAddress}
          autoComplete="off"
          spellCheck={false}
          disabled={blocked}
          onChange={(event) => setRecipientAddress(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') submit();
          }}
        />

        <button disabled={blocked || !validAmount} onClick={submit}>
          Withdraw
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
