# Stellar Mixer Desktop

Tauri + React desktop shell for the Stellar RISC0 mixer pipeline.

## What is implemented

- Tauri 2 + React/Vite UI.
- Rust backend commands.
- OS keyring storage.
- Password-encrypted vault with sodiumoxide secretbox + pwhash.
- Local mixer identity using schnorrkel.
- Multiple Stellar accounts stored only inside the encrypted backend vault.
- Public UI state in IndexedDB via Dexie.
- Deposit command wired to Rust backend: creates note, calls mixer `deposit`, and stores note only after successful tx.
- Withdraw/transfer command skeletons are split into modules and intentionally emit progress; hook the existing e2e core in `src-tauri/src/mixer/flow.rs` as the next step.

## Requirements

- Rust stable
- Node.js 20+
- pnpm or npm
- Tauri prerequisites for your OS
- Local TreePIR server on `http://127.0.0.1:3000`
- Groth16 wrap server on `http://213.171.26.211:8080/wrap`

## Setup

```bash
cd /Users/coolman/Code
cp -R /path/to/stellar-mixer-desktop ./stellar-mixer-desktop
cd stellar-mixer-desktop

git init
pnpm install
pnpm tauri dev
```

You can also use npm:

```bash
npm install
npm run tauri:dev
```

## Security model

- React never receives private keys, note secrets, nullifiers, or raw vault contents.
- Public account names/addresses, UI history, and public balances live in IndexedDB.
- Secret material lives in OS keyring as an encrypted vault blob.
- Unlock flow decrypts the vault into Rust memory only.
- Closing the app drops the in-memory session.

## Constants

See `src-tauri/src/config.rs` for RPC URL, mixer contract id, TreePIR URL, Groth16 URL and guest ELF paths.
