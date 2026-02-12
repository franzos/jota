pub mod cache;
pub mod commands;
pub mod display;
pub mod network;
pub mod recipient;
pub mod service;
pub mod signer;
pub mod wallet;
pub mod wallet_file;

pub use cache::TransactionCache;
pub use wallet::{AccountRecord, Wallet};
pub use network::NetworkClient;
pub use commands::Command;
pub use recipient::{Recipient, ResolvedRecipient};
pub use service::WalletService;
pub use signer::{Signer, SoftwareSigner, SignedMessage, verify_message};

pub use iota_sdk::types::{Address, ObjectId};

/// Reject wallet names containing path separators or traversal sequences.
pub fn validate_wallet_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() {
        anyhow::bail!("Wallet name cannot be empty.");
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        anyhow::bail!(
            "Invalid wallet name '{name}'. Must not contain '/', '\\', or '..'."
        );
    }
    if name.contains(std::path::MAIN_SEPARATOR) {
        anyhow::bail!(
            "Invalid wallet name '{name}'. Must not contain path separators."
        );
    }
    Ok(())
}

/// List wallet files in a directory (stem names of `.wallet` files).
pub fn list_wallets(dir: &std::path::Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "wallet").unwrap_or(false) {
                if let Some(stem) = path.file_stem() {
                    names.push(stem.to_string_lossy().to_string());
                }
            }
        }
    }
    names.sort();
    names
}
