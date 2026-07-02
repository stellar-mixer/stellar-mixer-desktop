import Dexie, { Table } from 'dexie';
import type { AccountRole, ActivityItem, PublicAccount } from './types';

export type AccountRow = PublicAccount & {
  balance?: string;
  role?: AccountRole;
  updatedAt: number;
};

type AccountInput = PublicAccount & {
  balance?: string;
  role?: AccountRole;
};

class MixerDb extends Dexie {
  accounts!: Table<AccountRow, string>;
  activity!: Table<ActivityItem, number>;

  constructor() {
    const profile = ((import.meta as any).env?.VITE_STELLAR_MIXER_PROFILE ?? '').trim() || 'default';
    const dbName = profile === 'default'
      ? 'stellar-mixer-desktop'
      : `stellar-mixer-desktop-${profile}`;

    super(dbName);

    this.version(1).stores({
      accounts: '&id, publicKey, name, updatedAt',
      activity: '++id, accountId, kind, createdAt, status, txHash',
    });
  }
}

export const db = new MixerDb();

export async function upsertAccounts(accounts: AccountInput[]) {
  const now = Date.now();
  const existingRows = await db.accounts.bulkGet(accounts.map((account) => account.id));
  const existingById = new Map(existingRows.filter(Boolean).map((row) => [row!.id, row!]));

  await db.accounts.bulkPut(
    accounts.map((account) => {
      const existing = existingById.get(account.id);

      return {
        ...account,
        balance: account.balance ?? existing?.balance,
        role: account.role ?? existing?.role ?? 'all',
        updatedAt: now,
      };
    }),
  );
}

export async function updateAccountRole(accountId: string, role: AccountRole) {
  await db.accounts.update(accountId, {
    role,
    updatedAt: Date.now(),
  });
}

export async function addActivity(item: ActivityItem) {
  return db.activity.add(item);
}
