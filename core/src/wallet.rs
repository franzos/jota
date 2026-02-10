/// Wallet state — holds decrypted mnemonic, keypair, derived address, and network config.
use anyhow::{Context, Result};
use iota_sdk::crypto::ed25519::Ed25519PrivateKey;
use iota_sdk::crypto::FromMnemonic;
use iota_sdk::types::Address;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, Zeroizing};

use crate::wallet_file;

/// Serialized wallet state — what gets encrypted and stored on disk.
#[derive(Serialize, Deserialize)]
pub struct WalletData {
    pub mnemonic: String,
    #[serde(rename = "network")]
    pub network_config: NetworkConfig,
}

impl Drop for WalletData {
    fn drop(&mut self) {
        self.mnemonic.zeroize();
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NetworkConfig {
    pub network: Network,
    pub custom_url: Option<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            network: Network::Testnet,
            custom_url: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Testnet,
    Mainnet,
    Devnet,
    Custom,
}

impl std::fmt::Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Network::Testnet => write!(f, "testnet"),
            Network::Mainnet => write!(f, "mainnet"),
            Network::Devnet => write!(f, "devnet"),
            Network::Custom => write!(f, "custom"),
        }
    }
}

/// Generate a new 24-word BIP-39 mnemonic.
fn generate_mnemonic() -> String {
    let mnemonic = bip39::Mnemonic::generate(24).expect("mnemonic generation failed");
    mnemonic.to_string()
}

/// In-memory wallet with derived key material.
pub struct Wallet {
    data: WalletData,
    private_key: Ed25519PrivateKey,
    address: Address,
    path: PathBuf,
}

impl Wallet {
    /// Create a brand new wallet with a fresh mnemonic.
    pub fn create_new(
        path: PathBuf,
        password: &[u8],
        network_config: NetworkConfig,
    ) -> Result<Self> {
        let mnemonic = generate_mnemonic();

        let private_key = Ed25519PrivateKey::from_mnemonic(&mnemonic, None, None)
            .map_err(|e| anyhow::anyhow!("Failed to derive key from mnemonic: {e}"))?;
        let public_key = private_key.public_key();
        let address = public_key.derive_address();

        let data = WalletData {
            mnemonic,
            network_config,
        };

        let json = Zeroizing::new(
            serde_json::to_vec(&data)
                .context("Failed to serialize wallet data")?,
        );
        wallet_file::save_to_file(&path, &json, password)
            .context("Failed to save wallet file")?;

        Ok(Self {
            data,
            private_key,
            address,
            path,
        })
    }

    /// Open an existing wallet file.
    pub fn open(path: &Path, password: &[u8]) -> Result<Self> {
        let json = wallet_file::load_from_file(path, password)
            .context("Failed to open wallet file. Wrong password or corrupt file?")?;
        let data: WalletData = serde_json::from_slice(&json)
            .context("Failed to parse wallet data. File may be corrupt.")?;

        let private_key = Ed25519PrivateKey::from_mnemonic(&data.mnemonic, None, None)
            .map_err(|e| anyhow::anyhow!("Failed to derive key from stored mnemonic: {e}"))?;
        let public_key = private_key.public_key();
        let address = public_key.derive_address();

        Ok(Self {
            data,
            private_key,
            address,
            path: path.to_path_buf(),
        })
    }

    /// Recover a wallet from an existing mnemonic phrase.
    pub fn recover_from_mnemonic(
        path: PathBuf,
        password: &[u8],
        mnemonic: &str,
        network_config: NetworkConfig,
    ) -> Result<Self> {
        // Validate mnemonic by trying to derive a key
        let private_key = Ed25519PrivateKey::from_mnemonic(mnemonic, None, None)
            .map_err(|e| anyhow::anyhow!("Invalid mnemonic phrase: {e}"))?;
        let public_key = private_key.public_key();
        let address = public_key.derive_address();

        let data = WalletData {
            mnemonic: mnemonic.to_string(),
            network_config,
        };

        let json = Zeroizing::new(
            serde_json::to_vec(&data)
                .context("Failed to serialize wallet data")?,
        );
        wallet_file::save_to_file(&path, &json, password)
            .context("Failed to save wallet file")?;

        Ok(Self {
            data,
            private_key,
            address,
            path,
        })
    }

    /// Change the encryption password for a wallet file.
    /// Verifies the current password before re-encrypting.
    pub fn change_password(
        path: &Path,
        old_password: &[u8],
        new_password: &[u8],
    ) -> Result<()> {
        let plaintext = wallet_file::load_from_file(path, old_password)
            .map_err(|e| match e {
                wallet_file::WalletFileError::DecryptionFailed => {
                    anyhow::anyhow!("Current password is incorrect")
                }
                other => anyhow::anyhow!("{other}"),
            })?;
        wallet_file::save_to_file(path, &plaintext, new_password)
            .context("Failed to save wallet with new password")?;
        Ok(())
    }

    /// Re-encrypt and save the wallet to disk (e.g. after changing network config).
    pub fn save(&self, password: &[u8]) -> Result<()> {
        let json = Zeroizing::new(
            serde_json::to_vec(&self.data)
                .context("Failed to serialize wallet data")?,
        );
        wallet_file::save_to_file(&self.path, &json, password)
            .context("Failed to save wallet file")?;
        Ok(())
    }

    pub fn address(&self) -> &Address {
        &self.address
    }

    /// Short address string for display in prompts (first 4 hex chars after 0x).
    pub fn short_address(&self) -> String {
        let full = self.address.to_string();
        if full.len() >= 6 {
            full[2..6].to_string()
        } else {
            full
        }
    }

    pub fn private_key(&self) -> &Ed25519PrivateKey {
        &self.private_key
    }

    pub fn mnemonic(&self) -> &str {
        &self.data.mnemonic
    }

    pub fn network_config(&self) -> &NetworkConfig {
        &self.data.network_config
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_mainnet(&self) -> bool {
        self.data.network_config.network == Network::Mainnet
    }
}

impl Drop for Wallet {
    fn drop(&mut self) {
        // WalletData.mnemonic is zeroized via WalletData's Drop impl.
        // Ed25519PrivateKey wraps ed25519_dalek::SigningKey, which zeroizes its
        // key material on drop (via ed25519-dalek's Drop impl that calls zeroize).
        // The SDK wrapper does not re-export Zeroize, but the underlying key is safe.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_new_wallet() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wallet");
        let password = b"test-password";

        let wallet = Wallet::create_new(
            path.clone(),
            password,
            NetworkConfig::default(),
        )
        .unwrap();

        // Mnemonic should be 24 words
        let word_count = wallet.mnemonic().split_whitespace().count();
        assert_eq!(word_count, 24, "expected 24-word mnemonic, got {word_count}");

        // Address should be non-zero
        assert_ne!(*wallet.address(), Address::ZERO);

        // Short address should be 4 hex chars
        assert_eq!(wallet.short_address().len(), 4);
    }

    #[test]
    fn open_existing_wallet() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wallet");
        let password = b"open-password";

        let wallet1 = Wallet::create_new(
            path.clone(),
            password,
            NetworkConfig::default(),
        )
        .unwrap();

        let wallet2 = Wallet::open(&path, password).unwrap();

        assert_eq!(wallet1.mnemonic(), wallet2.mnemonic());
        assert_eq!(*wallet1.address(), *wallet2.address());
    }

    #[test]
    fn recover_from_mnemonic_produces_same_address() {
        let dir = tempfile::tempdir().unwrap();
        let path1 = dir.path().join("original.wallet");
        let path2 = dir.path().join("recovered.wallet");
        let password = b"recover-password";

        let original = Wallet::create_new(
            path1,
            password,
            NetworkConfig::default(),
        )
        .unwrap();

        let recovered = Wallet::recover_from_mnemonic(
            path2,
            password,
            original.mnemonic(),
            NetworkConfig::default(),
        )
        .unwrap();

        assert_eq!(*original.address(), *recovered.address());
    }

    #[test]
    fn wrong_password_fails_to_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wallet");

        Wallet::create_new(
            path.clone(),
            b"correct",
            NetworkConfig::default(),
        )
        .unwrap();

        let result = Wallet::open(&path, b"wrong");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_mnemonic_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wallet");

        let result = Wallet::recover_from_mnemonic(
            path,
            b"password",
            "not a valid mnemonic phrase at all",
            NetworkConfig::default(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn change_password() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wallet");

        let wallet = Wallet::create_new(
            path.clone(),
            b"old-password",
            NetworkConfig::default(),
        )
        .unwrap();
        let original_address = *wallet.address();

        // Change password
        Wallet::change_password(&path, b"old-password", b"new-password").unwrap();

        // Old password no longer works
        assert!(Wallet::open(&path, b"old-password").is_err());

        // New password works, data intact
        let reopened = Wallet::open(&path, b"new-password").unwrap();
        assert_eq!(*reopened.address(), original_address);
    }

    #[test]
    fn change_password_wrong_old_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wallet");

        Wallet::create_new(
            path.clone(),
            b"correct",
            NetworkConfig::default(),
        )
        .unwrap();

        let result = Wallet::change_password(&path, b"wrong", b"new");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("incorrect"));
    }

    #[test]
    fn save_persists_network_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save-test.wallet");
        let password = b"save-password";

        let devnet_config = NetworkConfig {
            network: Network::Devnet,
            custom_url: None,
        };

        // Create wallet with devnet config
        let wallet = Wallet::create_new(
            path.clone(),
            password,
            devnet_config.clone(),
        )
        .unwrap();

        let original_mnemonic = wallet.mnemonic().to_string();
        let original_address = *wallet.address();

        // Re-save (simulates persisting after potential state changes)
        wallet.save(password).unwrap();

        // Reopen and verify everything persisted correctly
        let reopened = Wallet::open(&path, password).unwrap();
        assert_eq!(
            reopened.mnemonic(),
            original_mnemonic,
            "mnemonic should persist across save/reopen"
        );
        assert_eq!(
            *reopened.address(),
            original_address,
            "address should persist across save/reopen"
        );
        assert_eq!(
            *reopened.network_config(),
            devnet_config,
            "network config should persist across save/reopen"
        );
    }
}
