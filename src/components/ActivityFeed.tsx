import type { ActivityItem } from '../lib/types';

const TESTNET_TX_EXPLORER_BASE = 'https://stellar.expert/explorer/testnet/tx';

export function ActivityFeed({ activity }: { activity: ActivityItem[] }) {
  return (
    <section className="activity-feed panel-scroll">
      <div className="panel-header">
        <h3>History</h3>
        <span>{activity.length}</span>
      </div>

      <div className="panel-scroll-body pretty-scroll">
        {activity.length === 0 && <p className="empty-state">No activity for this account yet.</p>}

        {activity.map((item, index) => (
          <article key={item.id ?? `${item.createdAt}-${index}`} className={`activity-item ${item.status}`}>
            <div className="activity-title-row">
              <strong>{item.title}</strong>
              {item.amount && <span className="activity-amount">{formatSignedAmount(item)}</span>}
            </div>

            {item.detail && <p className="activity-detail">{compactDetail(item.detail)}</p>}

            {item.txHash && (
              <p className="activity-tx mono">
                <span>tx {item.txHash.slice(0, 12)}…{item.txHash.slice(-12)}</span>
                <a
                  className="activity-tx-link"
                  href={`${TESTNET_TX_EXPLORER_BASE}/${item.txHash}`}
                  target="_blank"
                  rel="noreferrer"
                  title="Open transaction in StellarExpert"
                >
                  Open ↗
                </a>
              </p>
            )}

            <time>{new Date(item.createdAt).toLocaleString()}</time>
          </article>
        ))}
      </div>
    </section>
  );
}

function formatSignedAmount(item: ActivityItem) {
  const amount = item.amount ?? '';

  if (item.kind === 'withdraw') return `-${amount} XLM`;
  if (item.kind === 'deposit') return `+${amount} XLM`;

  return `${amount} XLM`;
}


function compactDetail(detail: string): string {
  const lower = detail.toLowerCase();

  if (lower.includes('resulting balance is not within the allowed range')) {
    return 'not enough public XLM: the token transfer would violate Stellar balance/reserve/fee limits.';
  }

  if (
    lower.includes('simulationfailed') &&
    lower.includes('contract call failed') &&
    lower.includes('transfer')
  ) {
    return 'simulation failed during token transfer. Try a smaller amount or fund this account.';
  }

  if (lower.includes('simulationfailed')) {
    const code = detail.match(/Error\(Contract,\s*#(\d+)\)/)?.[1];

    if (code) {
      return `simulation failed with contract error #${code}.`;
    }

    return 'simulation failed before submit.';
  }

  if (detail.includes('WaitTransactionTimeout') && detail.includes('NotFound')) {
    return 'transaction was submitted but RPC did not find it before timeout. Refresh balances and retry.';
  }

  if (detail.length > 360) {
    return `${detail.slice(0, 320)}…`;
  }

  return detail;
}
