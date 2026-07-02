import { Keypair, xdr } from '@stellar/stellar-sdk';

const STELLAR_RPC_URL = 'https://soroban-rpc.testnet.stellar.gateway.fm';
const BALANCE_CHUNK_SIZE = 100;

export async function getNativeBalance(publicKey: string): Promise<string> {
  const balances = await getNativeBalances([publicKey]);
  return balances[publicKey] ?? '0 XLM';
}

export async function getNativeBalances(publicKeys: string[]): Promise<Record<string, string>> {
  const unique = [...new Set(publicKeys.filter(Boolean))];
  const result: Record<string, string> = {};

  for (const publicKey of unique) {
    result[publicKey] = '0 XLM';
  }

  for (let i = 0; i < unique.length; i += BALANCE_CHUNK_SIZE) {
    const chunk = unique.slice(i, i + BALANCE_CHUNK_SIZE);
    const keys = chunk.map((publicKey) => accountLedgerKey(publicKey).toXDR('base64'));

    const response = await fetch(STELLAR_RPC_URL, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: Date.now(),
        method: 'getLedgerEntries',
        params: { keys },
      }),
    });

    if (!response.ok) {
      throw new Error(`Stellar RPC balance request failed: HTTP ${response.status}`);
    }

    const payload = await response.json();

    if (payload.error) {
      const message = payload.error?.message ?? JSON.stringify(payload.error);
      if (String(message).toLowerCase().includes('not found')) continue;
      throw new Error(`Stellar RPC balance error: ${JSON.stringify(payload.error)}`);
    }

    const entries = payload.result?.entries ?? [];

    entries.forEach((entry: any, entryIndex: number) => {
      const key = entry.key;
      const index = typeof key === 'string' ? keys.indexOf(key) : entryIndex;
      const publicKey = chunk[index];

      if (!publicKey) return;

      const entryXdr = entry.xdr ?? entry.data ?? entry.value;
      if (!entryXdr) return;

      const data = decodeLedgerEntryData(entryXdr);
      const account = data.account();
      const stroops = toBigInt(account.balance());

      result[publicKey] = `${formatStroops(stroops)} XLM`;
    });
  }

  return result;
}

function accountLedgerKey(publicKey: string): any {
  const accountId = Keypair.fromPublicKey(publicKey).xdrAccountId();

  const LedgerKey: any = xdr.LedgerKey;
  const LedgerKeyAccount: any = xdr.LedgerKeyAccount;
  const LedgerEntryType: any = xdr.LedgerEntryType;

  const keyAccount = new LedgerKeyAccount({ accountId });

  if (typeof LedgerKey.account === 'function') {
    return LedgerKey.account(keyAccount);
  }

  return new LedgerKey({
    switch: LedgerEntryType.account(),
    account: keyAccount,
  });
}

function decodeLedgerEntryData(entryXdr: string): any {
  const Xdr: any = xdr;

  try {
    return Xdr.LedgerEntryData.fromXDR(entryXdr, 'base64');
  } catch {
    const fullEntry = Xdr.LedgerEntry.fromXDR(entryXdr, 'base64');
    return fullEntry.data();
  }
}

function toBigInt(value: any): bigint {
  if (typeof value === 'bigint') return value;
  if (typeof value === 'number') return BigInt(value);
  if (typeof value === 'string') return BigInt(value);
  return BigInt(value.toString());
}

function formatStroops(stroops: bigint): string {
  const sign = stroops < 0n ? '-' : '';
  const abs = stroops < 0n ? -stroops : stroops;

  const whole = abs / 10_000_000n;
  const frac = abs % 10_000_000n;
  const fracText = frac.toString().padStart(7, '0').replace(/0+$/, '');

  return fracText.length > 0 ? `${sign}${whole}.${fracText}` : `${sign}${whole}`;
}
