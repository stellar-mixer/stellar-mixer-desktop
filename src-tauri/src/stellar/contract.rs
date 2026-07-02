use anyhow::{anyhow, bail, Context, Result};
use serde_json::json;
use soroban_client::{
    address::{Address, AddressTrait},
    contract::{ContractBehavior, Contracts},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    soroban_rpc::TransactionStatus,
    transaction::{TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior},
    xdr::{Int128Parts, ScVal},
    Options, Server,
};
use std::time::Duration;

use crate::config::{MIXER_CONTRACT_ID, STELLAR_RPC_URL};

#[derive(Debug, Clone)]
pub struct ContractCallOutcome {
    pub tx_hash: String,
    pub return_value: Option<ScVal>,
}

pub async fn ensure_source_account_exists(secret: &str, role: &str) -> Result<String> {
    let server = Server::new(STELLAR_RPC_URL, Options::default())
        .map_err(|error| anyhow!("failed to create Stellar RPC client: {error}"))?;

    let keypair = keypair_from_secret(secret)?;
    let public_key = keypair.public_key();

    server
        .get_account(&public_key)
        .await
        .map_err(|error| {
            anyhow!(
                "{role} source account {public_key} is not funded/available on Stellar RPC: {error}. Fund this selected account before starting the operation."
            )
        })?;

    Ok(public_key)
}

pub async fn invoke_deposit(
    secret: &str,
    amount: i128,
    leaf: [u8; 32],
    encrypted_note: &[u8],
) -> Result<(String, u64)> {
    let from = address_from_secret(secret)?;

    let args = vec![
        address_to_sc_val(&from)?,
        sc_i128(amount),
        sc_bytes32(&leaf)?,
        sc_bytes(encrypted_note)?,
    ];

    let outcome = invoke_contract(secret, "deposit", args, Duration::from_secs(90)).await?;

    let leaf_index = match outcome.return_value.context("deposit returned no value")? {
        ScVal::U64(value) => value,
        other => bail!("deposit returned non-u64 value: {other:?}"),
    };

    Ok((outcome.tx_hash, leaf_index))
}

pub async fn invoke_withdraw(
    secret: &str,
    seal: &[u8],
    journal: &[u8],
    recipient: Option<&str>,
    encrypted_note: &[u8],
) -> Result<String> {
    let recipient = match recipient {
        Some(address) => address_from_str(address)?,
        None => address_from_secret(secret)?,
    };

    let args = vec![
        sc_proof(seal)?,
        sc_withdraw_public_outputs(journal)?,
        address_to_sc_val(&recipient)?,
        sc_bytes(encrypted_note)?,
    ];

    Ok(
        invoke_contract(secret, "withdraw", args, Duration::from_secs(180))
            .await?
            .tx_hash,
    )
}

pub async fn invoke_transfer(
    secret: &str,
    seal: &[u8],
    journal: &[u8],
    encrypted_notes: &[Vec<u8>],
) -> Result<String> {
    let args = vec![
        sc_proof(seal)?,
        sc_transfer_public_outputs(journal)?,
        sc_bytes_vec(encrypted_notes)?,
    ];

    Ok(
        invoke_contract(secret, "transfer", args, Duration::from_secs(180))
            .await?
            .tx_hash,
    )
}

pub async fn invoke_contract(
    secret: &str,
    function: &str,
    args: Vec<ScVal>,
    wait_timeout: Duration,
) -> Result<ContractCallOutcome> {
    let server = Server::new(STELLAR_RPC_URL, Options::default())
        .map_err(|error| anyhow!("failed to create Stellar RPC client: {error}"))?;
    let keypair = keypair_from_secret(secret)?;
    let public_key = keypair.public_key();

    let mut source = server
        .get_account(&public_key)
        .await
        .map_err(|error| anyhow!("failed to fetch source account from Stellar RPC: {error}"))?;

    let contract = Contracts::new(MIXER_CONTRACT_ID)
        .map_err(|error| anyhow!("invalid mixer contract id {MIXER_CONTRACT_ID}: {error}"))?;

    let tx = TransactionBuilder::new(&mut source, Networks::testnet(), None)
        .fee(20_000_000u32)
        .add_operation(contract.call(function, Some(args)))
        .build();

    let mut prepared = server.prepare_transaction(&tx).await?;
    prepared.sign(&[keypair]);
    let sent = server.send_transaction(prepared).await?;
    let tx_hash = sent.hash.clone();

    let result = server
        .wait_transaction(&tx_hash, wait_timeout)
        .await
        .map_err(|(error, response)| {
            anyhow!("wait_transaction failed: error={error:?} response={response:?}")
        })?;

    if result.status != TransactionStatus::Success {
        bail!(
            "transaction failed: hash={} status={:?}",
            tx_hash,
            result.status
        );
    }

    let (_meta, return_value) = result
        .to_result_meta()
        .context("successful transaction has no result meta")?;

    Ok(ContractCallOutcome {
        tx_hash,
        return_value,
    })
}

pub fn keypair_from_secret(secret: &str) -> Result<Keypair> {
    Keypair::from_secret(secret)
        .map_err(|error| anyhow!("failed to parse Stellar secret key: {error}"))
}

pub fn address_from_secret(secret: &str) -> Result<Address> {
    let keypair = keypair_from_secret(secret)?;
    address_from_str(&keypair.public_key())
}

pub fn address_from_str(address: &str) -> Result<Address> {
    Address::new(address).map_err(|error| anyhow!("invalid Stellar address {address}: {error}"))
}

pub fn address_to_sc_val(address: &Address) -> Result<ScVal> {
    address
        .to_sc_val()
        .map_err(|error| anyhow!("failed to encode Stellar address as ScVal: {error}"))
}

pub fn sc_i128(value: i128) -> ScVal {
    ScVal::I128(Int128Parts {
        hi: (value >> 64) as i64,
        lo: value as u64,
    })
}

pub fn sc_bytes32(bytes: &[u8; 32]) -> Result<ScVal> {
    sc_val_json(json!({ "bytes": hex::encode(bytes) }))
}

pub fn sc_bytes(bytes: &[u8]) -> Result<ScVal> {
    sc_val_json(json!({ "bytes": hex::encode(bytes) }))
}

pub fn sc_bytes_vec(items: &[Vec<u8>]) -> Result<ScVal> {
    sc_val_json(json!({
        "vec": items
            .iter()
            .map(|item| json!({ "bytes": hex::encode(item) }))
            .collect::<Vec<_>>()
    }))
}

pub fn sc_proof(seal: &[u8]) -> Result<ScVal> {
    sc_val_json(json!({
        "map": [
            {
                "key": { "symbol": "seal" },
                "val": { "bytes": hex::encode(seal) }
            }
        ]
    }))
}

pub fn sc_withdraw_public_outputs(journal: &[u8]) -> Result<ScVal> {
    sc_val_json(json!({
        "map": [
            {
                "key": { "symbol": "journal" },
                "val": { "bytes": hex::encode(journal) }
            }
        ]
    }))
}

pub fn sc_transfer_public_outputs(journal: &[u8]) -> Result<ScVal> {
    sc_val_json(json!({
        "map": [
            {
                "key": { "symbol": "journal" },
                "val": { "bytes": hex::encode(journal) }
            }
        ]
    }))
}

fn sc_val_json(value: serde_json::Value) -> Result<ScVal> {
    Ok(serde_json::from_value(value)?)
}
