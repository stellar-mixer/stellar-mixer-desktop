pub const APP_SERVICE: &str = "stellar-mixer-desktop";
pub const VAULT_KEY: &str = "main-vault";

pub const TREEPIR_URL: &str = "http://127.0.0.1:3000";
pub const MIXER_ARCHIVE_URL: &str = "http://127.0.0.1:3001";
pub const GROTH16_WRAP_URL: &str = "http://213.171.26.211:8080/wrap";

pub const STELLAR_RPC_URL: &str = "https://soroban-rpc.testnet.stellar.gateway.fm";
pub const MIXER_CONTRACT_ID: &str = "CCVPF5JH57FQV535OYMWWY3VXUC53JEMCUPQIBM6NQKI3FZ5BZ47WVRL";

pub const DEPTH: usize = 35;

pub const TRANSFER_GUEST_ELF_PATH: &str = "/Users/coolman/Code/stellar-zk-mixer-risc0-prover/target/riscv-guest/methods/transfer_guest/riscv32im-risc0-zkvm-elf/release/transfer_guest.bin";
pub const WITHDRAW_GUEST_ELF_PATH: &str = "/Users/coolman/Code/stellar-zk-mixer-risc0-prover/target/riscv-guest/methods/withdraw_guest/riscv32im-risc0-zkvm-elf/release/withdraw_guest.bin";

pub const TRANSFER_GUEST_ID: [u32; 8] = [
    1806926303, 881112581, 2328359865, 3507547059, 3107974702, 3612662461, 100106614, 1141583706,
];

pub const WITHDRAW_GUEST_ID: [u32; 8] = [
    2068924445, 4135432564, 1502870765, 2851623954, 19087385, 1290228615, 1923192755, 2248842611,
];

pub const DEFAULT_DEPOSIT_AMOUNT: u128 = 10_000_000;
