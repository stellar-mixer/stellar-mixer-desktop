pub const APP_SERVICE: &str = "stellar-mixer-desktop";
pub const VAULT_KEY: &str = "main-vault";

pub const TREEPIR_URL: &str = "http://213.171.26.211:3000";
pub const MIXER_ARCHIVE_URL: &str = "http://213.171.26.211:3001";
pub const GROTH16_WRAP_URL: &str = "http://213.171.26.211:8080/wrap";

pub const STELLAR_RPC_URL: &str = "https://soroban-rpc.testnet.stellar.gateway.fm";
pub const MIXER_CONTRACT_ID: &str = "CCO2BNPJQENNYXHYE5JWGNX74SNHZT74V3OL2IFCJOVZHEJVO2KVWEN5";

pub const DEPTH: usize = 45;

pub const TRANSFER_GUEST_ELF_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/guests/transfer_guest.bin");
pub const WITHDRAW_GUEST_ELF_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/guests/withdraw_guest.bin");

pub const TRANSFER_GUEST_ID: [u32; 8] = [
    498159648,
    2938926520,
    1961483861,
    318186971,
    124339936,
    981829501,
    2383877649,
    2412115495,
];

pub const WITHDRAW_GUEST_ID: [u32; 8] = [
    58232140,
    1509054718,
    113098096,
    1232102061,
    150807468,
    3067204521,
    460803923,
    1733823451,
];

pub const DEFAULT_DEPOSIT_AMOUNT: u128 = 10_000_000;
