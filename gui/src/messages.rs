use crate::native_messaging::NativeRequest;
use crate::state::{Screen, SignMode, WalletInfo};
use crate::TokenOption;
use jota_core::network::{
    CoinMeta, NftSummary, StakedIotaSummary, TokenBalance, TransactionSummary, ValidatorSummary,
};
use jota_core::wallet::Network;
use jota_core::SignedMessage;
use zeroize::Zeroizing;

/// (transactions, total_count, epoch_deltas)
pub(crate) type TransactionPage = (Vec<TransactionSummary>, u32, Vec<(u64, i64)>);

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
    TransactionsLoaded(Result<TransactionPage, String>),

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

    // Explorer
    OpenExplorerAddress(String),

    // Contacts
    ContactsLoaded(Result<Vec<jota_core::Contact>, String>),
    OpenContactForm,
    CloseContactForm,
    ContactNameChanged(String),
    ContactAddressChanged(String),
    SaveContact,
    DeleteContact(usize),
    EditContact(usize),
    ContactSaved(Result<(), String>),
    ContactDeleted(Result<Vec<jota_core::Contact>, String>),
    SelectContact(String),
    SaveContactOffer,
    DismissContactOffer,

    // Staking
    StakeAmountChanged(String),
    ConfirmStake,
    StakeCompleted(Result<String, String>),
    ConfirmUnstake(String),
    UnstakeCompleted(Result<String, String>),
    StakesLoaded(Result<Vec<StakedIotaSummary>, String>),
    ValidatorsLoaded(Result<Vec<ValidatorSummary>, String>),
    SelectValidator(usize),
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

    // Native messaging (browser extension bridge)
    NativeRequest(NativeRequest),
    NativeClientConnected(std::sync::mpsc::Sender<crate::native_messaging::NativeResponse>),
    NativeClientDisconnected,
    ApproveNativeRequest,
    RejectNativeRequest,
    /// (request_id, Ok(result_json) | Err((error_code, error_message)))
    NativeSignCompleted(Result<(String, serde_json::Value), (String, String, String)>),

    // Native host installation
    ExtensionIdChanged(String),
    InstallNativeHost,
    NativeHostInstalled(Result<Vec<std::path::PathBuf>, String>),

    // Permissions
    RevokeSitePermission(String),

    // Settings
    NetworkChanged(Network),
    HistoryLookbackChanged(u64),
    SettingsOldPasswordChanged(Zeroizing<String>),
    SettingsNewPasswordChanged(Zeroizing<String>),
    SettingsNewPasswordConfirmChanged(Zeroizing<String>),
    ChangePassword,
    ChangePasswordCompleted(Result<Zeroizing<Vec<u8>>, String>),
}
