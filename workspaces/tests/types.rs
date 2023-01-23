use std::convert::TryFrom;
use std::str::FromStr;

use borsh::{BorshDeserialize, BorshSerialize};

use workspaces::types::{KeyType, PublicKey, SecretKey};
use workspaces::AccountId;

use near_sdk as sdk;

fn default_workspaces_pubkey() -> anyhow::Result<PublicKey> {
    let mut data = vec![KeyType::ED25519 as u8];
    data.extend(bs58::decode("6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp").into_vec()?);
    Ok(PublicKey::try_from_slice(data.as_slice())?)
}

#[test]
fn test_keypair_ed25519() -> anyhow::Result<()> {
    let pk_expected = "\"ed25519:DcA2MzgpJbrUATQLLceocVckhhAqrkingax4oJ9kZ847\"";
    let sk_expected = "\"ed25519:3KyUuch8pYP47krBq4DosFEVBMR5wDTMQ8AThzM8kAEcBQEpsPdYTZ2FPX5ZnSoLrerjwg66hwwJaW1wHzprd5k3\"";

    let sk = SecretKey::from_seed(KeyType::ED25519, "test");
    let pk = sk.public_key();
    assert_eq!(serde_json::to_string(&pk)?, pk_expected);
    assert_eq!(serde_json::to_string(&sk)?, sk_expected);
    assert_eq!(pk, serde_json::from_str(pk_expected)?);
    assert_eq!(sk, serde_json::from_str(sk_expected)?);

    Ok(())
}

#[test]
fn test_keypair_secp256k1() -> anyhow::Result<()> {
    let pk_expected = "\"secp256k1:5ftgm7wYK5gtVqq1kxMGy7gSudkrfYCbpsjL6sH1nwx2oj5NR2JktohjzB6fbEhhRERQpiwJcpwnQjxtoX3GS3cQ\"";
    let sk_expected = "\"secp256k1:X4ETFKtQkSGVoZEnkn7bZ3LyajJaK2b3eweXaKmynGx\"";

    let sk = SecretKey::from_seed(KeyType::SECP256K1, "test");
    let pk = sk.public_key();
    assert_eq!(serde_json::to_string(&pk)?, pk_expected);
    assert_eq!(serde_json::to_string(&sk)?, sk_expected);
    assert_eq!(pk, serde_json::from_str(pk_expected)?);
    assert_eq!(sk, serde_json::from_str(sk_expected)?);

    Ok(())
}

#[test]
fn test_pubkey_serialization() -> anyhow::Result<()> {
    for key_type in [KeyType::ED25519, KeyType::SECP256K1] {
        let sk = SecretKey::from_seed(key_type, "test");
        let pk = sk.public_key();
        let bytes = pk.try_to_vec()?;

        // Borsh Deserialization should equate to the original public key:
        assert_eq!(PublicKey::try_from_slice(&bytes)?, pk);

        // invalid public key should error out on deserialization:
        assert!(PublicKey::try_from_slice(&[0]).is_err());
    }

    Ok(())
}

#[tokio::test]
async fn test_pubkey_from_sdk_ser() -> anyhow::Result<()> {
    const PUBKEY_MANAGE_BYTES: &[u8] =
        include_bytes!("test-contracts/type-serialize/res/test_contract_type_serialization.wasm");
    let worker = workspaces::sandbox().await?;
    let contract = worker.dev_deploy(PUBKEY_MANAGE_BYTES).await?;

    let ws_pk = default_workspaces_pubkey()?;
    let sdk_pk: sdk::PublicKey = contract
        .call("pass_pk_back_and_forth")
        .args_json(serde_json::json!({ "pk": ws_pk }))
        .transact()
        .await?
        .json()?;

    assert_eq!(ws_pk, PublicKey::try_from(sdk_pk)?);
    Ok(())
}

#[test]
fn test_pubkey_borsh_format_change() -> anyhow::Result<()> {
    let pk = default_workspaces_pubkey()?;
    assert_eq!(
        pk.try_to_vec()?,
        bs58::decode("16E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp").into_vec()?
    );

    Ok(())
}

#[test]
fn test_valid_account_id() {
    let account_id = "testnet";
    assert!(
        AccountId::from_str(account_id).is_ok(),
        "Something changed underneath for testnet to not be a valid Account ID"
    );
}
