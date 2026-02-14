/// Integration tests that hit the real testnet/devnet.
/// Run with: cargo test -- --ignored
use iota_wallet_core::commands::Command;
use iota_wallet_core::network::{NetworkClient, TransactionFilter};
use iota_wallet_core::wallet::{Network, NetworkConfig, Wallet};
use std::time::Duration;

fn testnet_config() -> NetworkConfig {
    NetworkConfig {
        network: Network::Testnet,
        custom_url: None,
    }
}

fn devnet_config() -> NetworkConfig {
    NetworkConfig {
        network: Network::Devnet,
        custom_url: None,
    }
}

/// Brief pause to let the indexer catch up after a transaction.
async fn wait_for_indexer() {
    tokio::time::sleep(Duration::from_secs(3)).await;
}

#[tokio::test]
#[ignore]
async fn testnet_query_balance() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let path = dir.path().join("balance-test.wallet");
    let password = b"integration-test";
    let config = testnet_config();

    let network = NetworkClient::new(&config, false).expect("failed to create testnet client");
    let wallet = Wallet::create_new(path, password, config).expect("failed to create wallet");

    // A fresh wallet should have zero balance
    let balance = network
        .balance(wallet.address())
        .await
        .expect("failed to query balance");
    assert_eq!(balance, 0, "fresh wallet should have 0 balance");
}

#[tokio::test]
#[ignore]
async fn testnet_query_transactions_empty() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let path = dir.path().join("txs-test.wallet");
    let password = b"integration-test";
    let config = testnet_config();

    let network = NetworkClient::new(&config, false).expect("failed to create testnet client");
    let wallet = Wallet::create_new(path, password, config).expect("failed to create wallet");

    let txs = network
        .transactions(wallet.address(), TransactionFilter::All)
        .await
        .expect("failed to query transactions");
    assert!(txs.is_empty(), "fresh wallet should have no transactions");
}

#[tokio::test]
#[ignore]
async fn devnet_faucet_and_balance() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let path = dir.path().join("faucet-test.wallet");
    let password = b"integration-test";
    let config = devnet_config();

    let network = NetworkClient::new(&config, false).expect("failed to create devnet client");
    let wallet = Wallet::create_new(path, password, config).expect("failed to create wallet");

    // Request faucet tokens
    network
        .faucet(wallet.address())
        .await
        .expect("faucet request failed");

    wait_for_indexer().await;

    // Balance should now be non-zero
    let balance = network
        .balance(wallet.address())
        .await
        .expect("failed to query balance after faucet");
    assert!(
        balance > 0,
        "balance should be > 0 after faucet, got {balance}"
    );
}

#[tokio::test]
#[ignore]
async fn devnet_send_iota() {
    let sender_dir = tempfile::tempdir().expect("failed to create temp dir");
    let recipient_dir = tempfile::tempdir().expect("failed to create temp dir");
    let sender_path = sender_dir.path().join("sender.wallet");
    let recipient_path = recipient_dir.path().join("recipient.wallet");
    let password = b"integration-test";
    let config = devnet_config();

    let network = NetworkClient::new(&config, false).expect("failed to create devnet client");
    let sender = Wallet::create_new(sender_path, password, config.clone())
        .expect("failed to create sender wallet");
    let recipient = Wallet::create_new(recipient_path, password, config)
        .expect("failed to create recipient wallet");

    // Fund the sender via faucet
    network
        .faucet(sender.address())
        .await
        .expect("faucet request failed");

    // Send 0.1 IOTA (100_000_000 nanos)
    let amount = 100_000_000u64;
    let result = network
        .send_iota(
            &sender.signer().unwrap(),
            sender.address(),
            *recipient.address(),
            amount,
        )
        .await
        .expect("send_iota failed");

    assert!(
        !result.digest.is_empty(),
        "transaction digest should not be empty"
    );
    eprintln!("send_iota digest: {}", result.digest);

    wait_for_indexer().await;

    // Recipient should have received tokens
    let recipient_balance = network
        .balance(recipient.address())
        .await
        .expect("failed to query recipient balance");
    assert!(
        recipient_balance >= amount,
        "recipient balance should be >= {amount}, got {recipient_balance}"
    );
}

#[tokio::test]
#[ignore]
async fn testnet_client_creation() {
    // Verify we can create clients for each network type
    let testnet = NetworkClient::new(&testnet_config(), false);
    assert!(testnet.is_ok(), "testnet client creation should succeed");

    let devnet = NetworkClient::new(&devnet_config(), false);
    assert!(devnet.is_ok(), "devnet client creation should succeed");

    let mainnet = NetworkClient::new(
        &NetworkConfig {
            network: Network::Mainnet,
            custom_url: None,
        },
        false,
    );
    assert!(mainnet.is_ok(), "mainnet client creation should succeed");

    // Custom without URL should fail
    let custom_no_url = NetworkClient::new(
        &NetworkConfig {
            network: Network::Custom,
            custom_url: None,
        },
        false,
    );
    assert!(custom_no_url.is_err(), "custom without URL should fail");
}

#[tokio::test]
#[ignore]
async fn wallet_create_recover_open_roundtrip() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let path1 = dir.path().join("roundtrip-original.wallet");
    let path2 = dir.path().join("roundtrip-recovered.wallet");
    let password = b"roundtrip-test";

    // Create new wallet
    let wallet1 = Wallet::create_new(path1.clone(), password, testnet_config())
        .expect("failed to create wallet");
    let mnemonic = wallet1.mnemonic().unwrap().to_string();
    let address1 = *wallet1.address();

    // Open the same wallet from disk
    let wallet2 = Wallet::open(&path1, password).expect("failed to open wallet");
    assert_eq!(
        *wallet2.address(),
        address1,
        "reopened wallet should have same address"
    );
    assert_eq!(
        wallet2.mnemonic().unwrap(),
        mnemonic,
        "reopened wallet should have same mnemonic"
    );

    // Recover from mnemonic to a different file
    let wallet3 = Wallet::recover_from_mnemonic(path2, password, &mnemonic, testnet_config())
        .expect("failed to recover wallet");
    assert_eq!(
        *wallet3.address(),
        address1,
        "recovered wallet should have same address"
    );
}

// --- New integration tests ---

#[tokio::test]
#[ignore]
async fn devnet_send_insufficient_balance() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let sender_path = dir.path().join("insufficient-sender.wallet");
    let password = b"integration-test";
    let config = devnet_config();

    let network = NetworkClient::new(&config, false).expect("failed to create devnet client");
    let sender =
        Wallet::create_new(sender_path, password, config).expect("failed to create sender wallet");

    // Don't fund via faucet — balance is 0
    let recipient = iota_sdk::types::Address::ZERO;
    let amount = 100_000_000u64; // 0.1 IOTA

    let result = network
        .send_iota(
            &sender.signer().unwrap(),
            sender.address(),
            recipient,
            amount,
        )
        .await;

    assert!(result.is_err(), "sending with no balance should fail");
    let err = result.err().expect("already checked is_err").to_string();
    eprintln!("Expected error for insufficient balance: {err}");
}

#[tokio::test]
#[ignore]
async fn devnet_send_to_self() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let path = dir.path().join("self-send.wallet");
    let password = b"integration-test";
    let config = devnet_config();

    let network = NetworkClient::new(&config, false).expect("failed to create devnet client");
    let wallet = Wallet::create_new(path, password, config).expect("failed to create wallet");

    // Fund via faucet
    network
        .faucet(wallet.address())
        .await
        .expect("faucet request failed");

    wait_for_indexer().await;

    let balance_before = network
        .balance(wallet.address())
        .await
        .expect("failed to query balance before self-send");
    assert!(balance_before > 0, "balance should be > 0 after faucet");

    // Send to self
    let send_amount = 100_000_000u64; // 0.1 IOTA
    let result = network
        .send_iota(
            &wallet.signer().unwrap(),
            wallet.address(),
            *wallet.address(),
            send_amount,
        )
        .await
        .expect("self-send failed");

    assert!(
        !result.digest.is_empty(),
        "self-send should produce a digest"
    );

    wait_for_indexer().await;

    // Balance should be slightly less due to gas. On IOTA, gas is always consumed
    // even for self-sends, but the indexer may lag. Just verify we still have funds.
    let balance_after = network
        .balance(wallet.address())
        .await
        .expect("failed to query balance after self-send");

    // Gas cost should make balance_after <= balance_before
    assert!(
        balance_after <= balance_before,
        "balance after self-send should be <= original due to gas, before={balance_before} after={balance_after}"
    );
    assert!(
        balance_after > 0,
        "balance after self-send should still be > 0"
    );
    eprintln!(
        "self-send: before={balance_before} after={balance_after} gas_cost={}",
        balance_before - balance_after
    );
}

#[tokio::test]
#[ignore]
async fn devnet_faucet_twice() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let path = dir.path().join("faucet-twice.wallet");
    let password = b"integration-test";
    let config = devnet_config();

    let network = NetworkClient::new(&config, false).expect("failed to create devnet client");
    let wallet = Wallet::create_new(path, password, config).expect("failed to create wallet");

    // First faucet request
    network
        .faucet(wallet.address())
        .await
        .expect("first faucet request failed");

    wait_for_indexer().await;

    let balance_after_first = network
        .balance(wallet.address())
        .await
        .expect("failed to query balance after first faucet");
    assert!(
        balance_after_first > 0,
        "balance should be > 0 after first faucet"
    );

    // Second faucet request
    network
        .faucet(wallet.address())
        .await
        .expect("second faucet request failed");

    wait_for_indexer().await;

    let balance_after_second = network
        .balance(wallet.address())
        .await
        .expect("failed to query balance after second faucet");
    assert!(
        balance_after_second > balance_after_first,
        "balance should increase after second faucet, first={balance_after_first} second={balance_after_second}"
    );
}

#[tokio::test]
#[ignore]
async fn devnet_transaction_history_after_send() {
    let sender_dir = tempfile::tempdir().expect("failed to create temp dir");
    let recipient_dir = tempfile::tempdir().expect("failed to create temp dir");
    let sender_path = sender_dir.path().join("txhist-sender.wallet");
    let recipient_path = recipient_dir.path().join("txhist-recipient.wallet");
    let password = b"integration-test";
    let config = devnet_config();

    let network = NetworkClient::new(&config, false).expect("failed to create devnet client");
    let sender = Wallet::create_new(sender_path, password, config.clone())
        .expect("failed to create sender wallet");
    let recipient = Wallet::create_new(recipient_path, password, config)
        .expect("failed to create recipient wallet");

    // Fund sender
    network
        .faucet(sender.address())
        .await
        .expect("faucet request failed");

    // Send some IOTA
    let amount = 100_000_000u64;
    network
        .send_iota(
            &sender.signer().unwrap(),
            sender.address(),
            *recipient.address(),
            amount,
        )
        .await
        .expect("send_iota failed");

    wait_for_indexer().await;

    // Query sender's outgoing transactions — should have at least 1
    let txs = network
        .transactions(sender.address(), TransactionFilter::Out)
        .await
        .expect("failed to query sender transactions");
    assert!(
        !txs.is_empty(),
        "sender should have at least 1 outgoing transaction after send"
    );
    eprintln!(
        "sender has {} outgoing transaction(s), first digest: {}",
        txs.len(),
        txs[0].digest
    );
}

#[tokio::test]
#[ignore]
async fn testnet_balance_known_address() {
    let config = testnet_config();
    let network = NetworkClient::new(&config, false).expect("failed to create testnet client");

    // Query balance of the zero address — just verify the query succeeds.
    // The zero address may have funds from various test activities.
    let balance = network
        .balance(&iota_sdk::types::Address::ZERO)
        .await
        .expect("failed to query balance of zero address");
    eprintln!("zero address balance on testnet: {balance} nanos");
    // No specific balance assertion — we just verify the RPC call works
}

/// Test that sending 0 IOTA is rejected at the command parsing level.
/// This doesn't need network access — it tests the full command parse path.
#[tokio::test]
async fn devnet_send_zero_rejected() {
    // "transfer <valid_address> 0" should fail at the Command::parse level
    let result = Command::parse(
        "transfer 0x0000000000000000000000000000000000000000000000000000000000000000 0",
    );
    assert!(
        result.is_err(),
        "sending 0 IOTA should be rejected at parse time"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Cannot send 0"),
        "error should mention 0 IOTA, got: {err}"
    );
}
