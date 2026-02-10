pub mod commands;
pub mod display;
pub mod network;
pub mod wallet;
pub mod wallet_file;

pub use wallet::Wallet;
pub use network::NetworkClient;
pub use commands::Command;

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
