use anyhow::{anyhow, Result};
use rand_core::OsRng;
use rsa_risc0::pkcs1::EncodeRsaPrivateKey;
use rsa_risc0::traits::PublicKeyParts;
use rsa_risc0::{RsaPrivateKey, RsaPublicKey};
use soroban_client::keypair::{Keypair, KeypairBehavior};

use crate::models::{AccountSecret, PublicAccount};
use crate::proofs::note::fixed_be_array;

pub fn create_stellar_account_secret(name: String) -> Result<AccountSecret> {
    let keypair = Keypair::random()
        .map_err(|error| anyhow!("failed to generate Stellar keypair: {error}"))?;

    let rsa_private = RsaPrivateKey::new(&mut OsRng, 2048)?;
    let rsa_public = RsaPublicKey::from(&rsa_private);
    let rsa_modulus: [u8; 256] = fixed_be_array(&rsa_public.n().to_bytes_be())?;
    let rsa_der = rsa_private.to_pkcs1_der()?;

    let created_at = chrono::Utc::now().timestamp_millis();

    Ok(AccountSecret {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        stellar_secret_key: keypair
            .secret_key()
            .map_err(|error| anyhow!("failed to export Stellar secret key: {error}"))?,
        stellar_public_key: keypair.public_key(),
        rsa_private_key_pkcs1_der_hex: hex::encode(rsa_der.as_bytes()),
        rsa_public_modulus_hex: hex::encode(rsa_modulus),
        created_at,
    })
}

impl From<&AccountSecret> for PublicAccount {
    fn from(value: &AccountSecret) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            public_key: value.stellar_public_key.clone(),
            created_at: value.created_at,
        }
    }
}
