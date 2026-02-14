use crate::state::{Screen, SignMode, WalletInfo};
use crate::TokenOption;
use iota_wallet_core::network::{
    CoinMeta, NftSummary, StakedIotaSummary, TokenBalance, TransactionSummary,
};
use iota_wallet_core::wallet::Network;
use iota_wallet_core::SignedMessage;
use zeroize::Zeroizing;

// -- Messages --

#[derive(Clone)]
pub(crate) enum Message {
    // Navigation
    GoTo(Screen),

    // Wallet select
    WalletSelected(String),

    // Form inputs
    PasswordChanged(Zeroizing<String>),
    PasswordConfirmChanged(Zeroizing<String>),
    WalletNameChanged(String),
    MnemonicInputChanged(Zeroizing<String>),
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

    // Hardware wallet
    #[cfg(feature = "hardware-wallets")]
    HardwareConnect,
    #[cfg(feature = "hardware-wallets")]
    HardwareConnected(Result<WalletInfo, String>),

    // Hardware wallet address verification
    #[cfg(feature = "hardware-wallets")]
    HardwareVerifyAddress,
    #[cfg(feature = "hardware-wallets")]
    HardwareVerifyAddressCompleted(Result<(), String>),

    // Hardware wallet reconnect
    #[cfg(feature = "hardware-wallets")]
    HardwareReconnect,
    #[cfg(feature = "hardware-wallets")]
    HardwareReconnected(Result<(), String>),

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

    // NFTs
    NftsLoaded(Result<Vec<NftSummary>, String>),
    RefreshNfts,
    SendNftSelected(String),
    SendNftRecipientChanged(String),
    ConfirmSendNft,
    SendNftCompleted(Result<String, String>),
    CancelSendNft,

    // Account switching
    AccountInputChanged(String),
    AccountGoPressed,
    AccountIndexChanged(u64),
    AccountSwitched(Result<WalletInfo, String>),

    // Sign / Verify / Notarize
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
    NotarizeDescriptionChanged(String),
    ConfirmNotarize,
    NotarizeCompleted(Result<String, String>),

    // Settings
    NetworkChanged(Network),
    SettingsOldPasswordChanged(Zeroizing<String>),
    SettingsNewPasswordChanged(Zeroizing<String>),
    SettingsNewPasswordConfirmChanged(Zeroizing<String>),
    ChangePassword,
    ChangePasswordCompleted(Result<Zeroizing<Vec<u8>>, String>),
}
