/// Wallet state — holds decrypted mnemonic, keypair, derived address, and network config.
use anyhow::{Context, Result};
use iota_sdk::crypto::ed25519::Ed25519PrivateKey;
use iota_sdk::crypto::FromMnemonic;
use iota_sdk::types::Address;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, Zeroizing};

use crate::signer::SoftwareSigner;
use crate::wallet_file;

/// Whether the wallet is backed by a software mnemonic or a Ledger hardware device.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum WalletType {
    #[default]
    Software,
    Ledger,
}

/// Per-account metadata stored alongside the wallet.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AccountRecord {
    pub index: u64,
    #[serde(default)]
    pub last_balance: Option<u64>,
}

/// Serialized wallet state — what gets encrypted and stored on disk.
#[derive(Serialize, Deserialize)]
pub struct WalletData {
    #[serde(default)]
    pub mnemonic: Option<String>,
    #[serde(rename = "network")]
    pub network_config: NetworkConfig,
    #[serde(default)]
    pub active_account_index: u64,
    #[serde(default)]
    pub accounts: Vec<AccountRecord>,
    #[serde(default)]
    pub wallet_type: WalletType,
    /// Stored address for Ledger wallets (to verify on reconnect).
    #[serde(default)]
    pub address: Option<String>,
}

impl Drop for WalletData {
    fn drop(&mut self) {
        if let Some(ref mut m) = self.mnemonic {
            m.zeroize();
        }
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
fn generate_mnemonic() -> Result<String> {
    let mnemonic = bip39::Mnemonic::generate(24)
        .map_err(|e| anyhow::anyhow!("Failed to generate mnemonic: {e}"))?;
    Ok(mnemonic.to_string())
}

/// Derive an Ed25519 keypair and address from a mnemonic + account index.
fn derive_key(mnemonic: &str, account_index: u64) -> Result<(Ed25519PrivateKey, Address)> {
    let idx = if account_index == 0 { None } else { Some(account_index) };
    let private_key = Ed25519PrivateKey::from_mnemonic(mnemonic, idx, None)
        .map_err(|e| anyhow::anyhow!("Failed to derive key from mnemonic: {e}"))?;
    let address = private_key.public_key().derive_address();
    Ok((private_key, address))
}

/// Insert an account index into the list if not already present, keeping it sorted.
fn ensure_account_in_list(accounts: &mut Vec<AccountRecord>, index: u64) {
    if !accounts.iter().any(|a| a.index == index) {
        accounts.push(AccountRecord { index, last_balance: None });
        accounts.sort_by_key(|a| a.index);
    }
}

/// Serialize, encrypt, and write wallet data to disk, then update the meta file.
fn persist_wallet_to_file(
    path: &Path,
    data: &WalletData,
    password: &[u8],
) -> Result<()> {
    let json = Zeroizing::new(
        serde_json::to_vec(data)
            .context("Failed to serialize wallet data")?,
    );
    wallet_file::save_to_file(path, &json, password)
        .context("Failed to save wallet file")?;
    crate::write_wallet_meta(path, data.wallet_type);
    Ok(())
}

/// In-memory wallet with derived key material.
pub struct Wallet {
    data: WalletData,
    /// `None` for Ledger wallets — signing happens on the device.
    private_key: Option<Ed25519PrivateKey>,
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
        let mnemonic = generate_mnemonic()?;
        let (private_key, address) = derive_key(&mnemonic, 0)?;

        let data = WalletData {
            mnemonic: Some(mnemonic),
            network_config,
            active_account_index: 0,
            accounts: vec![AccountRecord { index: 0, last_balance: None }],
            wallet_type: WalletType::Software,
            address: None,
        };

        persist_wallet_to_file(&path, &data, password)?;

        Ok(Self {
            data,
            private_key: Some(private_key),
            address,
            path,
        })
    }

    /// Open an existing wallet file.
    pub fn open(path: &Path, password: &[u8]) -> Result<Self> {
        let json = wallet_file::load_from_file(path, password)
            .context("Failed to open wallet file. Wrong password or corrupt file?")?;
        let mut data: WalletData = serde_json::from_slice(&json)
            .context("Failed to parse wallet data. File may be corrupt.")?;

        // Ensure active account is in the known list (handles old wallet files)
        ensure_account_in_list(&mut data.accounts, data.active_account_index);

        match data.wallet_type {
            WalletType::Software => {
                let mnemonic = data.mnemonic.as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Software wallet is missing its mnemonic."))?;
                let (private_key, address) = derive_key(mnemonic, data.active_account_index)?;
                Ok(Self {
                    data,
                    private_key: Some(private_key),
                    address,
                    path: path.to_path_buf(),
                })
            }
            WalletType::Ledger => {
                let address = data.address.as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Ledger wallet is missing its stored address."))?;
                let address = address.parse::<Address>()
                    .map_err(|e| anyhow::anyhow!("Invalid stored address: {e}"))?;
                Ok(Self {
                    data,
                    private_key: None,
                    address,
                    path: path.to_path_buf(),
                })
            }
        }
    }

    /// Recover a wallet from an existing mnemonic phrase.
    pub fn recover_from_mnemonic(
        path: PathBuf,
        password: &[u8],
        mnemonic: &str,
        network_config: NetworkConfig,
    ) -> Result<Self> {
        let (private_key, address) = derive_key(mnemonic, 0)?;

        let data = WalletData {
            mnemonic: Some(mnemonic.to_string()),
            network_config,
            active_account_index: 0,
            accounts: vec![AccountRecord { index: 0, last_balance: None }],
            wallet_type: WalletType::Software,
            address: None,
        };

        persist_wallet_to_file(&path, &data, password)?;

        Ok(Self {
            data,
            private_key: Some(private_key),
            address,
            path,
        })
    }

    /// Create a new Ledger wallet file. The address comes from the connected device.
    pub fn create_ledger(
        path: PathBuf,
        password: &[u8],
        address: Address,
        network_config: NetworkConfig,
    ) -> Result<Self> {
        let data = WalletData {
            mnemonic: None,
            network_config,
            active_account_index: 0,
            accounts: vec![AccountRecord { index: 0, last_balance: None }],
            wallet_type: WalletType::Ledger,
            address: Some(address.to_string()),
        };

        persist_wallet_to_file(&path, &data, password)?;

        Ok(Self {
            data,
            private_key: None,
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
        persist_wallet_to_file(&self.path, &self.data, password)
    }

    /// Switch to a different account index. Re-derives the keypair and address.
    /// Does NOT save — caller decides whether to persist.
    ///
    /// For Ledger wallets this only updates the index — the caller must reconnect
    /// the device with the new derivation path and call `set_address()`.
    pub fn switch_account(&mut self, index: u64) -> Result<()> {
        match self.data.wallet_type {
            WalletType::Software => {
                let mnemonic = self.data.mnemonic.as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Software wallet is missing its mnemonic."))?;
                let (private_key, address) = derive_key(mnemonic, index)?;
                self.private_key = Some(private_key);
                self.address = address;
            }
            WalletType::Ledger => {
                // Index update only — caller reconnects the device
            }
        }
        self.data.active_account_index = index;
        ensure_account_in_list(&mut self.data.accounts, index);
        Ok(())
    }

    pub fn account_index(&self) -> u64 {
        self.data.active_account_index
    }

    pub fn known_accounts(&self) -> &[AccountRecord] {
        &self.data.accounts
    }

    /// Derive the address for an account index without switching to it.
    /// Only available for software wallets.
    pub fn derive_address_for(&self, index: u64) -> Result<Address> {
        let mnemonic = self.data.mnemonic.as_deref()
            .ok_or_else(|| anyhow::anyhow!("Cannot derive addresses for a Ledger wallet."))?;
        let (_, address) = derive_key(mnemonic, index)?;
        Ok(address)
    }

    pub fn address(&self) -> &Address {
        &self.address
    }

    /// Update the address (used after Ledger reconnect with a new derivation path).
    pub fn set_address(&mut self, address: Address) {
        self.data.address = Some(address.to_string());
        self.address = address;
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

    /// Build a software signer. Returns an error if called on a Ledger wallet.
    pub fn signer(&self) -> Result<SoftwareSigner> {
        let key = self.private_key.clone().ok_or_else(|| {
            anyhow::anyhow!("Cannot build software signer for a Ledger wallet. Use LedgerSigner instead.")
        })?;
        Ok(SoftwareSigner::new(key))
    }

    pub fn mnemonic(&self) -> Option<&str> {
        self.data.mnemonic.as_deref()
    }

    pub fn wallet_type(&self) -> &WalletType {
        &self.data.wallet_type
    }

    pub fn is_ledger(&self) -> bool {
        self.data.wallet_type == WalletType::Ledger
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
        let mnemonic = wallet.mnemonic().expect("software wallet should have mnemonic");
        let word_count = mnemonic.split_whitespace().count();
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

        assert_eq!(wallet1.mnemonic().unwrap(), wallet2.mnemonic().unwrap());
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
            original.mnemonic().unwrap(),
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
    fn switch_account_changes_address() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wallet");
        let password = b"test-password";

        let mut wallet = Wallet::create_new(
            path,
            password,
            NetworkConfig::default(),
        )
        .unwrap();

        let addr0 = *wallet.address();
        assert_eq!(wallet.account_index(), 0);
        assert_eq!(wallet.known_accounts().len(), 1);

        wallet.switch_account(1).unwrap();
        assert_eq!(wallet.account_index(), 1);
        assert_ne!(*wallet.address(), addr0, "account 1 should have a different address");
        assert_eq!(wallet.known_accounts().len(), 2);

        wallet.switch_account(0).unwrap();
        assert_eq!(*wallet.address(), addr0, "switching back to 0 should restore the original address");
        // Still 2 known accounts (0 and 1)
        assert_eq!(wallet.known_accounts().len(), 2);

        // Switching to same index again doesn't duplicate
        wallet.switch_account(1).unwrap();
        assert_eq!(wallet.known_accounts().len(), 2);

        // Known accounts are sorted
        let indices: Vec<u64> = wallet.known_accounts().iter().map(|a| a.index).collect();
        assert_eq!(indices, vec![0, 1]);
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

        let original_mnemonic = wallet.mnemonic().unwrap().to_string();
        let original_address = *wallet.address();

        // Re-save (simulates persisting after potential state changes)
        wallet.save(password).unwrap();

        // Reopen and verify everything persisted correctly
        let reopened = Wallet::open(&path, password).unwrap();
        assert_eq!(
            reopened.mnemonic().unwrap(),
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
