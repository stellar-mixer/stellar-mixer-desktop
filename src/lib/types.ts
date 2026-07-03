export type AccountRole = 'all' | 'deposit' | 'withdraw' | 'transfer';

export type VaultStatus = {
  exists: boolean;
  unlocked: boolean;
};

export type PublicAccount = {
  id: string;
  name: string;
  publicKey: string;
  createdAt: number;
};

export type UnlockResult = {
  identityPublicKey: string;
  accounts: PublicAccount[];
  noteCount: number;
  recoveryPhrase?: string;
};

export type AccountView = PublicAccount & {
  balance?: string;
  role?: AccountRole;
  isLoadingBalance?: boolean;
};

export type ActivityKind = 'deposit' | 'withdraw' | 'transfer' | 'system' | 'error';

export type ActivityItem = {
  id?: number;
  accountId?: string;
  kind: ActivityKind;
  amount?: string;
  title: string;
  detail?: string;
  txHash?: string;
  createdAt: number;
  status: 'pending' | 'success' | 'error';

  operationId?: string;

  accountName?: string;
  accountPublicKey?: string;

  inputNoteIds?: string[];
  outputNoteIds?: string[];
  spentNoteIds?: string[];
  createdNoteIds?: string[];

  noteId?: string;
  createdNoteId?: string;

  leafIndex?: number;
  leafHex?: string;

  recipientAddress?: string;
  recipientIdentity?: string;

  feePayerAccountId?: string;
  feePayerName?: string;
  feePayerPublicKey?: string;

  message?: string;
};

export type MixerStats = {
  anonymitySet: number;
  rootHex?: string;
  ready?: boolean;
};

export type PrivateBalance = {
  totalStroops: string;
  totalXlm: string;
  spendableStroops: string;
  spendableXlm: string;
  pendingStroops: string;
  pendingXlm: string;
  unspentNoteCount: number;
  spendableNoteCount: number;
  maxInputs: number;
};

export type ArchiveSyncReport = {
  archiveContractId?: string;
  encryptedNoteCursor: number;
  nullifierCursor: number;
  scannedEncryptedNotes: number;
  scannedNullifiers: number;
  importedNoteCount: number;
  spentNoteCount: number;
  receivedTransferNotes: NoteView[];
};

export type BackupExportResult = {
  path: string;
};

export type BackupImportResult = UnlockResult & {
  uiStateJson?: string;
};

export type ProgressEvent = {
  opId: string;
  accountId?: string;
  step: string;
  message: string;
  at: number;
};

export type DepositResult = {
  txHash: string;
  leafIndex: number;
  noteId: string;
  leafHex: string;
  amount: string;
};

export type WithdrawResult = {
  txHash: string;
  spentNoteIds: string[];
  createdNoteId?: string;
  amount: string;
};


export type NoteView = {
  id: string;
  depositedByAccountId: string;
  depositedByAccountName: string;
  depositedByPublicKey: string;
  amountStroops: string;
  amountXlm: string;
  leafIndex?: number;
  status: 'spendable' | 'spent' | 'pending-index' | string;
  createdAt: number;
  spentAt?: number;
  sourceKind?: string;
  sourceLedger?: number;
  message?: string;
};


export type TransferResult = {
  txHash: string;
  spentNoteIds: string[];
  createdNoteIds: string[];
  amount: string;
  recipientIdentity: string;
};
