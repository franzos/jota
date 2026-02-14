//! Domain error type for wallet operations.

use thiserror::Error;

use crate::wallet_file::WalletFileError;

/// Typed error enum for wallet operations, allowing callers to match on
/// specific failure modes instead of inspecting opaque `anyhow::Error` messages.
#[derive(Debug, Error)]
pub enum WalletError {
    /// Insufficient balance for the requested operation.
    #[error("{0}")]
    InsufficientBalance(String),

    /// Invalid recipient address or name resolution failure.
    #[error("{0}")]
    InvalidRecipient(String),

    /// Invalid amount, token not found, or ambiguous token alias.
    #[error("{0}")]
    InvalidAmount(String),

    /// Network or RPC communication failure.
    #[error("{0}")]
    Network(String),

    /// Storage or cache error (SQLite, file I/O).
    #[error("{0}")]
    Storage(String),

    /// Hardware wallet (Ledger) communication or signing error.
    #[error("{0}")]
    HardwareWallet(String),

    /// Signing or key derivation error.
    #[error("{0}")]
    Signing(String),

    /// Wallet file encryption, decryption, or persistence error.
    #[error(transparent)]
    WalletFile(#[from] WalletFileError),

    /// Invalid wallet state or configuration.
    #[error("{0}")]
    InvalidState(String),

    /// Unexpected error from internal subsystems.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Alias for `std::result::Result<T, WalletError>`.
pub type Result<T> = std::result::Result<T, WalletError>;
