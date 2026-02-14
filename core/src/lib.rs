pub mod cache;
pub mod commands;
pub mod display;
#[cfg(feature = "ledger")]
pub mod ledger_signer;
#[cfg(feature = "ledger")]
pub use ledger_iota_rebased::Bip32Path;
#[cfg(feature = "ledger")]
pub use ledger_signer::LedgerSigner;

/// Derive the BIP32 path for a given network and account index.
#[cfg(feature = "ledger")]
pub fn bip32_path_for(network: wallet::Network, account_index: u32) -> Bip32Path {
    match network {
        wallet::Network::Mainnet => Bip32Path::iota(account_index, 0, 0),
        _ => Bip32Path::testnet(account_index, 0, 0),
    }
}
pub mod network;
pub mod recipient;
pub mod service;
pub mod signer;
pub mod wallet;
pub mod wallet_file;

pub use cache::TransactionCache;
pub use commands::Command;
pub use network::NetworkClient;
pub use recipient::{Recipient, ResolvedRecipient};
pub use service::WalletService;
pub use signer::{verify_message, SignedMessage, Signer, SoftwareSigner};
pub use wallet::{AccountRecord, HardwareKind, Wallet, WalletType};

pub use iota_sdk::types::{Address, ObjectId};

/// Reject wallet names containing path separators or traversal sequences.
pub fn validate_wallet_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() {
        anyhow::bail!("Wallet name cannot be empty.");
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        anyhow::bail!("Invalid wallet name '{name}'. Must not contain '/', '\\', or '..'.");
    }
    if name.contains(std::path::MAIN_SEPARATOR) {
        anyhow::bail!("Invalid wallet name '{name}'. Must not contain path separators.");
    }
    Ok(())
}

/// Entry from the wallet directory listing.
#[derive(Debug, Clone)]
pub struct WalletEntry {
    pub name: String,
    pub wallet_type: WalletType,
}

/// List wallet files in a directory with their type.
/// Type is read from a `.meta` sidecar file; defaults to Software if missing.
pub fn list_wallets(dir: &std::path::Path) -> Vec<WalletEntry> {
    let mut entries = Vec::new();
    if let Ok(dir_entries) = std::fs::read_dir(dir) {
        for entry in dir_entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "wallet").unwrap_or(false) {
                if let Some(stem) = path.file_stem() {
                    let name = stem.to_string_lossy().to_string();
                    let meta_path = path.with_extension("meta");
                    let wallet_type = read_wallet_meta(&meta_path);
                    entries.push(WalletEntry { name, wallet_type });
                }
            }
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Write wallet type metadata to an unencrypted sidecar file.
pub fn write_wallet_meta(wallet_path: &std::path::Path, wallet_type: WalletType) {
    let meta_path = wallet_path.with_extension("meta");
    let type_str = match wallet_type {
        WalletType::Hardware(kind) => match kind {
            wallet::HardwareKind::Ledger => "hardware:ledger",
        },
        WalletType::Software => "software",
    };
    let _ = std::fs::write(&meta_path, type_str);
}

fn read_wallet_meta(path: &std::path::Path) -> WalletType {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| match s.trim() {
            "ledger" | "hardware:ledger" => {
                WalletType::Hardware(wallet::HardwareKind::Ledger)
            }
            _ => WalletType::Software,
        })
        .unwrap_or(WalletType::Software)
}
