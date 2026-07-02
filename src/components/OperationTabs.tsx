import { useEffect, useState } from 'react';
import { ActionPanel } from './ActionPanel';
import { WithdrawPanel } from './WithdrawPanel';
import { TransferPanel } from './TransferPanel';

type OperationTab = 'deposit' | 'withdraw' | 'transfer';
type OperationPermissions = Record<OperationTab, boolean>;

const ALL_ALLOWED: OperationPermissions = {
  deposit: true,
  withdraw: true,
  transfer: true,
};

export function OperationTabs({
  selectedAccountId,
  disabled,
  spendBusy,
  allowedOperations = ALL_ALLOWED,
  onDeposit,
  onWithdraw,
  onTransfer,
}: {
  selectedAccountId?: string;
  disabled?: boolean;
  spendBusy?: boolean;
  allowedOperations?: OperationPermissions;
  onDeposit: (amount: string) => Promise<void>;
  onWithdraw: (amount: string, recipientAddress?: string) => Promise<void>;
  onTransfer: (amount: string, recipientIdentity: string, message?: string) => Promise<void>;
}) {
  const [activeTab, setActiveTab] = useState<OperationTab>('deposit');

  useEffect(() => {
    if (allowedOperations[activeTab]) return;

    const next =
      (['deposit', 'withdraw', 'transfer'] as OperationTab[]).find((tab) => allowedOperations[tab]) ??
      'deposit';

    setActiveTab(next);
  }, [activeTab, allowedOperations]);

  function tabDisabled(tab: OperationTab) {
    return disabled || !allowedOperations[tab];
  }

  return (
    <section className="operation-tabs-card">
      <div className="operation-tab-bar">
        <button
          className={`operation-tab deposit ${activeTab === 'deposit' ? 'active' : ''} ${!allowedOperations.deposit ? 'disabled-by-role' : ''}`}
          disabled={disabled || !allowedOperations.deposit}
          title={!allowedOperations.deposit ? 'Selected account is not assigned to deposit operations' : undefined}
          onClick={() => setActiveTab('deposit')}
        >
          Deposit
        </button>

        <button
          className={`operation-tab withdraw ${activeTab === 'withdraw' ? 'active' : ''} ${!allowedOperations.withdraw ? 'disabled-by-role' : ''}`}
          disabled={disabled || !allowedOperations.withdraw}
          title={!allowedOperations.withdraw ? 'Selected account is not assigned to withdraw operations' : undefined}
          onClick={() => setActiveTab('withdraw')}
        >
          Withdraw
        </button>

        <button
          className={`operation-tab transfer ${activeTab === 'transfer' ? 'active' : ''} ${!allowedOperations.transfer ? 'disabled-by-role' : ''}`}
          disabled={disabled || !allowedOperations.transfer}
          title={!allowedOperations.transfer ? 'Selected account is not assigned to transfer operations' : undefined}
          onClick={() => setActiveTab('transfer')}
        >
          Transfer
        </button>
      </div>

      <div className="operation-tab-body">
        {activeTab === 'deposit' && (
          <ActionPanel
            title="Deposit"
            tone="deposit"
            disabled={tabDisabled('deposit')}
            resetKey={`${selectedAccountId ?? 'none'}:deposit`}
            onSubmit={onDeposit}
          />
        )}

        {activeTab === 'withdraw' && (
          <WithdrawPanel
            disabled={tabDisabled('withdraw')}
            spendBusy={spendBusy}
            resetKey={`${selectedAccountId ?? 'none'}:withdraw`}
            onSubmit={onWithdraw}
          />
        )}

        {activeTab === 'transfer' && (
          <TransferPanel
            disabled={tabDisabled('transfer')}
            spendBusy={spendBusy}
            resetKey={`${selectedAccountId ?? 'none'}:transfer`}
            onSubmit={onTransfer}
          />
        )}
      </div>
    </section>
  );
}
