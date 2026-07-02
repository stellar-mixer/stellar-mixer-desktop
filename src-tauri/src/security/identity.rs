use anyhow::{anyhow, Context, Result};
use bip39::{Language, Mnemonic};
use rand::rngs::OsRng as SchnorrkelOsRng;
use rand_chacha::ChaCha20Rng;
use rand_core::{OsRng as RsaOsRng, RngCore, SeedableRng};
use rsa_risc0::pkcs1::EncodeRsaPrivateKey;
use rsa_risc0::traits::PublicKeyParts;
use rsa_risc0::{RsaPrivateKey, RsaPublicKey};
use schnorrkel::Keypair;
use sha2_risc0::{Digest, Sha256};

use crate::models::MixerIdentitySecret;
use crate::proofs::note::fixed_be_array;

pub const RECOVERY_KIND_BIP39_RSA_V2: &str = "bip39-rsa-v2";
pub const RECOVERY_KIND_LEGACY_SMXREC1: &str = "legacy-smxrec1";

pub fn new_mixer_identity() -> Result<MixerIdentitySecret> {
    let phrase = generate_recovery_phrase()?;
    mixer_identity_from_bip39_phrase(&phrase)
}

pub fn generate_recovery_phrase() -> Result<String> {
    let mut entropy = [0u8; 32];
    RsaOsRng.fill_bytes(&mut entropy);

    let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
        .context("failed to create 24-word recovery phrase")?;

    Ok(mnemonic.to_string())
}

pub fn mixer_identity_from_bip39_phrase(phrase: &str) -> Result<MixerIdentitySecret> {
    let mnemonic = parse_bip39_phrase(phrase)?;
    let normalized_phrase = mnemonic.to_string();

    let (rsa_private_hex, rsa_modulus_hex) = deterministic_identity_rsa_keypair(&mnemonic)?;

    let schnorrkel_keypair = Keypair::generate_with(SchnorrkelOsRng);

    Ok(MixerIdentitySecret {
        schnorrkel_keypair_hex: hex::encode(schnorrkel_keypair.to_bytes()),
        schnorrkel_public_hex: hex::encode(schnorrkel_keypair.public.to_bytes()),
        treepir_identity_bincode_hex: None,
        recovery_kind: Some(RECOVERY_KIND_BIP39_RSA_V2.to_string()),
        recovery_phrase: Some(normalized_phrase),
        rsa_private_key_pkcs1_der_hex: Some(rsa_private_hex),
        rsa_public_modulus_hex: Some(rsa_modulus_hex),
        created_at: chrono::Utc::now().timestamp_millis(),
    })
}

pub fn parse_bip39_phrase(phrase: &str) -> Result<Mnemonic> {
    let normalized = phrase
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    Mnemonic::parse_in_normalized(Language::English, &normalized)
        .map_err(|error| anyhow!("invalid 24-word recovery phrase: {error}"))
}

pub fn format_bip39_phrase_for_display(phrase: &str) -> String {
    let words: Vec<&str> = phrase.split_whitespace().collect();

    if words.is_empty() {
        return phrase.to_string();
    }

    words
        .chunks(6)
        .map(|chunk| chunk.join(" "))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn new_identity_rsa_keypair() -> Result<(String, String)> {
    let mut rng = RsaOsRng;
    let private_key = RsaPrivateKey::new(&mut rng, 2048)?;
    encode_rsa_keypair(private_key)
}

fn deterministic_identity_rsa_keypair(mnemonic: &Mnemonic) -> Result<(String, String)> {
    let seed = mnemonic.to_seed_normalized("");

    let mut h = Sha256::new();
    h.update(b"stellar-mixer-deterministic-rsa-v2");
    h.update(seed);
    let rng_seed: [u8; 32] = h.finalize().into();

    let mut rng = ChaCha20Rng::from_seed(rng_seed);
    let private_key = RsaPrivateKey::new(&mut rng, 2048)
        .context("failed to deterministically generate RSA key from recovery phrase")?;

    encode_rsa_keypair(private_key)
}

fn encode_rsa_keypair(private_key: RsaPrivateKey) -> Result<(String, String)> {
    let public_key = RsaPublicKey::from(&private_key);

    let modulus: [u8; 256] = fixed_be_array(&public_key.n().to_bytes_be())?;
    let der = private_key.to_pkcs1_der()?;

    Ok((hex::encode(der.as_bytes()), hex::encode(modulus)))
}
