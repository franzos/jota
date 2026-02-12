use crate::TokenOption;
use crate::state::{Screen, SignMode, WalletInfo};
use iota_wallet_core::network::{CoinMeta, StakedIotaSummary, TokenBalance, TransactionSummary};
use iota_wallet_core::wallet::Network;
use iota_wallet_core::SignedMessage;
use zeroize::Zeroizing;

// -- Messages --

#[derive(Debug, Clone)]
pub(crate) enum Message {
    // Navigation
    GoTo(Screen),

    // Wallet select
    WalletSelected(String),

    // Form inputs
    PasswordChanged(String),
    PasswordConfirmChanged(String),
    WalletNameChanged(String),
    MnemonicInputChanged(String),
    RecipientChanged(String),
    AmountChanged(String),

    // Unlock
    UnlockWallet,
    WalletOpened(Result<WalletInfo, String>),

    // Create
    CreateWallet,
    WalletCreated(Result<(WalletInfo, Zeroizing<String>), String>),
    MnemonicConfirmed,

    // Recover
    RecoverWallet,
    WalletRecovered(Result<WalletInfo, String>),

    // Dashboard
    RefreshBalance,
    BalanceUpdated(Result<u64, String>),
    RequestFaucet,
    FaucetCompleted(Result<(), String>),
    CopyAddress,
    TransactionsLoaded(Result<(Vec<TransactionSummary>, u32, Vec<(u64, i64)>), String>),

    // Send
    RecipientResolved(Result<String, String>),
    TokenSelected(TokenOption),
    TokenBalancesLoaded(Result<(Vec<TokenBalance>, Vec<CoinMeta>), String>),
    ConfirmSend,
    SendCompleted(Result<String, String>),

    // History
    ToggleTxDetail(usize),
    OpenExplorer(String),
    RefreshHistory,
    HistoryNextPage,
    HistoryPrevPage,

    // Staking
    ValidatorResolved(Result<String, String>),
    ValidatorAddressChanged(String),
    StakeAmountChanged(String),
    ConfirmStake,
    StakeCompleted(Result<String, String>),
    ConfirmUnstake(String),
    UnstakeCompleted(Result<String, String>),
    StakesLoaded(Result<Vec<StakedIotaSummary>, String>),
    RefreshStakes,

    // Account switching
    AccountInputChanged(String),
    AccountGoPressed,
    AccountIndexChanged(u64),
    AccountSwitched(Result<WalletInfo, String>),

    // Sign / Verify
    SignMessageInputChanged(String),
    SignModeChanged(SignMode),
    ConfirmSign,
    SignCompleted(Result<SignedMessage, String>),
    CopySignature,
    CopyPublicKey,
    VerifyMessageInputChanged(String),
    VerifySignatureInputChanged(String),
    VerifyPublicKeyInputChanged(String),
    ConfirmVerify,
    VerifyCompleted(Result<bool, String>),

    // Settings
    NetworkChanged(Network),
    SettingsOldPasswordChanged(String),
    SettingsNewPasswordChanged(String),
    SettingsNewPasswordConfirmChanged(String),
    ChangePassword,
    ChangePasswordCompleted(Result<(), String>),
}
