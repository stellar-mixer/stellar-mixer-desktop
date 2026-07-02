import { useEffect, useState } from 'react';

export function TransferPanel({
  disabled,
  spendBusy,
  resetKey,
  onSubmit,
}: {
  disabled?: boolean;
  spendBusy?: boolean;
  resetKey?: string;
  onSubmit: (amount: string, recipientIdentity: string, message?: string) => Promise<void>;
}) {
  const [amount, setAmount] = useState('');
  const [recipientIdentity, setRecipientIdentity] = useState('');
  const [message, setMessage] = useState('');

  useEffect(() => {
    setAmount('');
    setRecipientIdentity('');
    setMessage('');
  }, [resetKey]);

  const blocked = disabled || spendBusy;
  const validAmount = isValidPositiveXlm(amount);
  const validRecipient = recipientIdentity.trim().length > 20;
  const validMessage = new TextEncoder().encode(message.trim()).length <= 64;

  async function submit() {
    const value = amount.trim().replace(',', '.');
    const recipient = recipientIdentity.trim();

    if (!validAmount || !validRecipient || !validMessage || blocked) return;

    await onSubmit(value, recipient, message.trim() || undefined);
    setAmount('');
    setRecipientIdentity('');
    setMessage('');
  }

  return (
    <section className={`action-panel transfer ${blocked ? 'panel-blocked' : ''}`}>
      <div className="transfer-fields">
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
          placeholder="Recipient Mixer Identity"
          value={recipientIdentity}
          autoComplete="off"
          spellCheck={false}
          disabled={blocked}
          onChange={(event) => setRecipientIdentity(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') submit();
          }}
        />

        <input
          placeholder="Private message, optional"
          value={message}
          autoComplete="off"
          spellCheck={false}
          disabled={blocked}
          onChange={(event) => setMessage(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') submit();
          }}
        />

        <button disabled={blocked || !validAmount || !validRecipient || !validMessage} onClick={submit}>
          Transfer
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
