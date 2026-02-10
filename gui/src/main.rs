use iced::widget::{button, column, container, row, scrollable, svg, text, text_input, Space};
use iced::theme::Palette;
use iced::{Color, Element, Fill, Length, Padding, Task, Theme};

// IOTA brand palette
const BG:      Color = Color::from_rgb(0.04, 0.055, 0.10);
const SIDEBAR: Color = Color::from_rgb(0.03, 0.047, 0.094);
const SURFACE: Color = Color::from_rgb(0.063, 0.094, 0.157);
const BORDER:  Color = Color::from_rgb(0.102, 0.157, 0.282);
const ACTIVE:  Color = Color::from_rgb(0.047, 0.125, 0.314);
const MUTED:   Color = Color::from_rgb(0.314, 0.408, 0.533);
const PRIMARY: Color = Color::from_rgb(0.0, 0.44, 0.94);
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, Zeroizing};

use iota_sdk::crypto::ed25519::Ed25519PrivateKey;
use iota_sdk::crypto::FromMnemonic;
use iota_sdk::types::{Address, ObjectId};
use std::fmt;

use iota_wallet_core::display::{format_balance, nanos_to_iota, parse_iota_amount};
use iota_wallet_core::{list_wallets, validate_wallet_name};
use iota_wallet_core::network::{
    NetworkClient, StakeStatus, StakedIotaSummary, TransactionDirection, TransactionFilter,
    TransactionSummary,
};
use iota_wallet_core::wallet::{Network, NetworkConfig, Wallet};

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("IOTA Wallet")
        .theme(App::theme)
        .run()
}

// -- Cloneable wallet info extracted after open/create --

#[derive(Clone)]
struct WalletInfo {
    address: Address,
    address_string: String,
    network_config: NetworkConfig,
    private_key: Arc<Ed25519PrivateKey>,
    is_mainnet: bool,
}

impl fmt::Debug for WalletInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WalletInfo")
            .field("address", &self.address)
            .field("is_mainnet", &self.is_mainnet)
            .finish_non_exhaustive()
    }
}

impl WalletInfo {
    fn from_wallet(wallet: &Wallet) -> Self {
        let private_key = Ed25519PrivateKey::from_mnemonic(wallet.mnemonic(), None, None)
            .expect("wallet mnemonic already validated");
        Self {
            address: *wallet.address(),
            address_string: wallet.address().to_string(),
            network_config: wallet.network_config().clone(),
            private_key: Arc::new(private_key),
            is_mainnet: wallet.is_mainnet(),
        }
    }
}

// -- Screens --

#[derive(Debug, Clone, PartialEq)]
enum Screen {
    // Wizard phase (no wallet loaded)
    WalletSelect,
    Unlock,
    Create,
    Recover,
    // Main phase (wallet loaded)
    Account,
    Send,
    Receive,
    History,
    Staking,
    Settings,
}

// -- Messages --

#[derive(Debug, Clone)]
enum Message {
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
    TransactionsLoaded(Result<Vec<TransactionSummary>, String>),

    // Send
    ConfirmSend,
    SendCompleted(Result<String, String>),

    // History
    ToggleTxDetail(usize),
    OpenExplorer(String),

    // Staking
    ValidatorAddressChanged(String),
    StakeAmountChanged(String),
    ConfirmStake,
    StakeCompleted(Result<String, String>),
    ConfirmUnstake(String),
    UnstakeCompleted(Result<String, String>),
    StakesLoaded(Result<Vec<StakedIotaSummary>, String>),
    RefreshStakes,

    // Settings
    NetworkChanged(Network),
    SettingsOldPasswordChanged(String),
    SettingsNewPasswordChanged(String),
    SettingsNewPasswordConfirmChanged(String),
    ChangePassword,
    ChangePasswordCompleted(Result<(), String>),
}

// -- App state --

struct App {
    screen: Screen,
    wallet_dir: PathBuf,
    wallet_names: Vec<String>,
    selected_wallet: Option<String>,
    wallet_info: Option<WalletInfo>,
    network_config: NetworkConfig,

    // Form fields
    password: Zeroizing<String>,
    password_confirm: Zeroizing<String>,
    wallet_name: String,
    mnemonic_input: Zeroizing<String>,
    recipient: String,
    amount: String,

    // Create screen — mnemonic display
    created_mnemonic: Option<Zeroizing<String>>,

    // Dashboard
    balance: Option<u64>,
    transactions: Vec<TransactionSummary>,

    // History
    expanded_tx: Option<usize>,

    // Staking
    stakes: Vec<StakedIotaSummary>,
    validator_address: String,
    stake_amount: String,

    // Settings — password change
    settings_old_password: Zeroizing<String>,
    settings_new_password: Zeroizing<String>,
    settings_new_password_confirm: Zeroizing<String>,

    // UI state
    loading: bool,
    error_message: Option<String>,
    success_message: Option<String>,
    status_message: Option<String>,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let network_config = Self::parse_network_from_args();
        let wallet_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".iota-wallet");
        let wallet_names = list_wallets(&wallet_dir);

        let app = Self {
            screen: Screen::WalletSelect,
            wallet_dir,
            wallet_names,
            selected_wallet: None,
            wallet_info: None,
            network_config,
            password: Zeroizing::new(String::new()),
            password_confirm: Zeroizing::new(String::new()),
            wallet_name: String::from("default"),
            mnemonic_input: Zeroizing::new(String::new()),
            recipient: String::new(),
            amount: String::new(),
            created_mnemonic: None,
            balance: None,
            transactions: Vec::new(),
            expanded_tx: None,
            stakes: Vec::new(),
            validator_address: String::new(),
            stake_amount: String::new(),
            settings_old_password: Zeroizing::new(String::new()),
            settings_new_password: Zeroizing::new(String::new()),
            settings_new_password_confirm: Zeroizing::new(String::new()),
            loading: false,
            error_message: None,
            success_message: None,
            status_message: None,
        };
        (app, Task::none())
    }

    fn parse_network_from_args() -> NetworkConfig {
        let args: Vec<String> = std::env::args().collect();
        let network = if args.iter().any(|a| a == "--mainnet") {
            Network::Mainnet
        } else if args.iter().any(|a| a == "--devnet") {
            Network::Devnet
        } else {
            Network::Testnet
        };
        NetworkConfig {
            network,
            custom_url: None,
        }
    }

    fn theme(&self) -> Theme {
        Theme::custom("IOTA".to_string(), Palette {
            background: BG,
            text: Color::from_rgb(0.82, 0.86, 0.91),
            primary: PRIMARY,
            success: Color::from_rgb(0.0, 0.80, 0.53),
            warning: Color::from_rgb(0.94, 0.63, 0.19),
            danger: Color::from_rgb(0.88, 0.25, 0.31),
        })
    }

    fn clear_form(&mut self) {
        self.password.zeroize();
        self.password_confirm.zeroize();
        self.mnemonic_input.zeroize();
        self.recipient.clear();
        self.amount.clear();
        self.error_message = None;
        self.success_message = None;
        self.status_message = None;
        self.created_mnemonic = None;
        self.expanded_tx = None;
        self.validator_address.clear();
        self.stake_amount.clear();
        self.settings_old_password.zeroize();
        self.settings_new_password.zeroize();
        self.settings_new_password_confirm.zeroize();
    }

    // -- Update --

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::GoTo(screen) => {
                self.clear_form();
                if screen == Screen::WalletSelect {
                    self.wallet_names = list_wallets(&self.wallet_dir);
                    self.wallet_info = None;
                    self.balance = None;
                    self.transactions.clear();
                    self.stakes.clear();
                }
                let load_stakes = screen == Screen::Staking;
                self.screen = screen;
                if load_stakes {
                    return self.load_stakes();
                }
                Task::none()
            }

            Message::WalletSelected(name) => {
                self.selected_wallet = Some(name);
                self.clear_form();
                self.screen = Screen::Unlock;
                Task::none()
            }

            // Form inputs
            Message::PasswordChanged(v) => {
                self.password = Zeroizing::new(v);
                Task::none()
            }
            Message::PasswordConfirmChanged(v) => {
                self.password_confirm = Zeroizing::new(v);
                Task::none()
            }
            Message::WalletNameChanged(v) => {
                self.wallet_name = v;
                Task::none()
            }
            Message::MnemonicInputChanged(v) => {
                self.mnemonic_input = Zeroizing::new(v);
                Task::none()
            }
            Message::RecipientChanged(v) => {
                self.recipient = v;
                Task::none()
            }
            Message::AmountChanged(v) => {
                self.amount = v;
                Task::none()
            }

            // -- Unlock --
            Message::UnlockWallet => {
                let name = self.selected_wallet.clone().unwrap_or_default();
                let path = self.wallet_dir.join(format!("{name}.wallet"));
                let pw = Zeroizing::new(self.password.as_bytes().to_vec());
                self.loading = true;
                self.error_message = None;

                Task::perform(
                    async move {
                        let wallet = Wallet::open(&path, &pw)?;
                        Ok(WalletInfo::from_wallet(&wallet))
                    },
                    |r: Result<WalletInfo, anyhow::Error>| {
                        Message::WalletOpened(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::WalletOpened(result) => {
                self.loading = false;
                match result {
                    Ok(info) => {
                        self.wallet_info = Some(info);
                        self.clear_form();
                        self.screen = Screen::Account;
                        return self.refresh_dashboard();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Create --
            Message::CreateWallet => {
                if let Some(err) = self.validate_create_form() {
                    self.error_message = Some(err);
                    return Task::none();
                }
                let name = self.wallet_name.clone();
                if let Err(e) = validate_wallet_name(&name) {
                    self.error_message = Some(e.to_string());
                    return Task::none();
                }
                let path = self.wallet_dir.join(format!("{name}.wallet"));
                if path.exists() {
                    self.error_message = Some(format!("Wallet '{name}' already exists"));
                    return Task::none();
                }
                let pw = Zeroizing::new(self.password.as_bytes().to_vec());
                let config = self.network_config.clone();
                self.loading = true;
                self.error_message = None;

                Task::perform(
                    async move {
                        std::fs::create_dir_all(path.parent().unwrap())?;
                        let wallet = Wallet::create_new(path, &pw, config)?;
                        let mnemonic = Zeroizing::new(wallet.mnemonic().to_string());
                        let info = WalletInfo::from_wallet(&wallet);
                        Ok((info, mnemonic))
                    },
                    |r: Result<(WalletInfo, Zeroizing<String>), anyhow::Error>| {
                        Message::WalletCreated(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::WalletCreated(result) => {
                self.loading = false;
                match result {
                    Ok((info, mnemonic)) => {
                        self.wallet_info = Some(info);
                        self.created_mnemonic = Some(mnemonic);
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::MnemonicConfirmed => {
                self.created_mnemonic = None;
                self.clear_form();
                self.screen = Screen::Account;
                self.refresh_dashboard()
            }

            // -- Recover --
            Message::RecoverWallet => {
                if let Some(err) = self.validate_create_form() {
                    self.error_message = Some(err);
                    return Task::none();
                }
                if self.mnemonic_input.trim().is_empty() {
                    self.error_message = Some("Mnemonic is required".into());
                    return Task::none();
                }
                let name = self.wallet_name.clone();
                if let Err(e) = validate_wallet_name(&name) {
                    self.error_message = Some(e.to_string());
                    return Task::none();
                }
                let path = self.wallet_dir.join(format!("{name}.wallet"));
                if path.exists() {
                    self.error_message = Some(format!("Wallet '{name}' already exists"));
                    return Task::none();
                }
                let pw = Zeroizing::new(self.password.as_bytes().to_vec());
                let mnemonic = Zeroizing::new(self.mnemonic_input.trim().to_string());
                let config = self.network_config.clone();
                self.loading = true;
                self.error_message = None;

                Task::perform(
                    async move {
                        std::fs::create_dir_all(path.parent().unwrap())?;
                        let wallet =
                            Wallet::recover_from_mnemonic(path, &pw, &mnemonic, config)?;
                        Ok(WalletInfo::from_wallet(&wallet))
                    },
                    |r: Result<WalletInfo, anyhow::Error>| {
                        Message::WalletRecovered(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::WalletRecovered(result) => {
                self.loading = false;
                match result {
                    Ok(info) => {
                        self.wallet_info = Some(info);
                        self.clear_form();
                        self.screen = Screen::Account;
                        return self.refresh_dashboard();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Dashboard --
            Message::RefreshBalance => self.refresh_dashboard(),

            Message::BalanceUpdated(result) => {
                self.loading = false;
                match result {
                    Ok(b) => self.balance = Some(b),
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::RequestFaucet => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let address = info.address;
                let config = info.network_config.clone();
                self.loading = true;
                self.error_message = None;
                self.success_message = None;

                Task::perform(
                    async move {
                        let net = NetworkClient::new(&config, false)?;
                        net.faucet(&address).await?;
                        Ok(())
                    },
                    |r: Result<(), anyhow::Error>| {
                        Message::FaucetCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::FaucetCompleted(result) => {
                self.loading = false;
                match result {
                    Ok(()) => {
                        self.success_message = Some("Faucet tokens requested".into());
                        return self.refresh_dashboard();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::CopyAddress => {
                if let Some(info) = &self.wallet_info {
                    if let Ok(mut cb) = arboard::Clipboard::new() {
                        let _ = cb.set_text(&info.address_string);
                        self.status_message = Some("Address copied".into());
                    }
                }
                Task::none()
            }

            Message::TransactionsLoaded(result) => {
                match result {
                    Ok(txs) => self.transactions = txs,
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Send --
            Message::ConfirmSend => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let recipient_str = self.recipient.trim().to_string();
                if recipient_str.is_empty() {
                    self.error_message = Some("Recipient address is required".into());
                    return Task::none();
                }
                let amount = match parse_iota_amount(&self.amount) {
                    Ok(0) => {
                        self.error_message = Some("Amount must be greater than 0".into());
                        return Task::none();
                    }
                    Ok(a) => a,
                    Err(e) => {
                        self.error_message = Some(e);
                        return Task::none();
                    }
                };
                let sender = info.address;
                let config = info.network_config.clone();
                let pk = info.private_key.clone();
                self.loading = true;
                self.error_message = None;

                Task::perform(
                    async move {
                        let recipient = Address::from_hex(&recipient_str)
                            .map_err(|e| anyhow::anyhow!("Invalid recipient address: {e}"))?;
                        let net = NetworkClient::new(&config, false)?;
                        let result = net.send_iota(&pk, &sender, recipient, amount).await?;
                        Ok(result.digest)
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::SendCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::SendCompleted(result) => {
                self.loading = false;
                match result {
                    Ok(digest) => {
                        self.success_message = Some(format!("Sent! Digest: {digest}"));
                        self.recipient.clear();
                        self.amount.clear();
                        self.screen = Screen::Account;
                        return self.refresh_dashboard();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::ToggleTxDetail(idx) => {
                if self.expanded_tx == Some(idx) {
                    self.expanded_tx = None;
                } else {
                    self.expanded_tx = Some(idx);
                }
                Task::none()
            }

            Message::OpenExplorer(digest) => {
                let network = self
                    .wallet_info
                    .as_ref()
                    .map(|i| &i.network_config.network);
                let query = match network {
                    Some(Network::Mainnet) | None => "",
                    Some(Network::Testnet) => "?network=testnet",
                    Some(Network::Devnet) => "?network=devnet",
                    Some(Network::Custom) => "?network=testnet",
                };
                let url = format!("https://explorer.iota.org/txblock/{digest}{query}");
                let _ = open::that(&url);
                Task::none()
            }

            // -- Staking --
            Message::ValidatorAddressChanged(v) => {
                self.validator_address = v;
                Task::none()
            }
            Message::StakeAmountChanged(v) => {
                self.stake_amount = v;
                Task::none()
            }
            Message::RefreshStakes => self.load_stakes(),

            Message::ConfirmStake => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let validator_str = self.validator_address.trim().to_string();
                if validator_str.is_empty() {
                    self.error_message = Some("Validator address is required".into());
                    return Task::none();
                }
                let amount = match parse_iota_amount(&self.stake_amount) {
                    Ok(0) => {
                        self.error_message = Some("Amount must be greater than 0".into());
                        return Task::none();
                    }
                    Ok(a) => a,
                    Err(e) => {
                        self.error_message = Some(e);
                        return Task::none();
                    }
                };
                let sender = info.address;
                let config = info.network_config.clone();
                let pk = info.private_key.clone();
                self.loading = true;
                self.error_message = None;

                Task::perform(
                    async move {
                        let validator = Address::from_hex(&validator_str)
                            .map_err(|e| anyhow::anyhow!("Invalid validator address: {e}"))?;
                        let net = NetworkClient::new(&config, false)?;
                        let result = net.stake_iota(&pk, &sender, validator, amount).await?;
                        Ok(result.digest)
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::StakeCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::StakeCompleted(result) => {
                self.loading = false;
                match result {
                    Ok(digest) => {
                        self.success_message = Some(format!("Staked! Digest: {digest}"));
                        self.validator_address.clear();
                        self.stake_amount.clear();
                        return self.load_stakes();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::ConfirmUnstake(object_id_str) => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let sender = info.address;
                let config = info.network_config.clone();
                let pk = info.private_key.clone();
                self.loading = true;
                self.error_message = None;
                self.success_message = None;

                Task::perform(
                    async move {
                        let object_id = ObjectId::from_hex(&object_id_str)
                            .map_err(|e| anyhow::anyhow!("Invalid object ID: {e}"))?;
                        let net = NetworkClient::new(&config, false)?;
                        let result = net.unstake_iota(&pk, &sender, object_id).await?;
                        Ok(result.digest)
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::UnstakeCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::UnstakeCompleted(result) => {
                self.loading = false;
                match result {
                    Ok(digest) => {
                        self.success_message = Some(format!("Unstaked! Digest: {digest}"));
                        return self.load_stakes();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::StakesLoaded(result) => {
                self.loading = false;
                match result {
                    Ok(s) => self.stakes = s,
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::NetworkChanged(network) => {
                let config = NetworkConfig {
                    network,
                    custom_url: None,
                };
                self.network_config = config.clone();
                if let Some(info) = &mut self.wallet_info {
                    info.network_config = config;
                    info.is_mainnet = network == Network::Mainnet;
                    return self.refresh_dashboard();
                }
                Task::none()
            }

            Message::SettingsOldPasswordChanged(v) => {
                self.settings_old_password = Zeroizing::new(v);
                Task::none()
            }
            Message::SettingsNewPasswordChanged(v) => {
                self.settings_new_password = Zeroizing::new(v);
                Task::none()
            }
            Message::SettingsNewPasswordConfirmChanged(v) => {
                self.settings_new_password_confirm = Zeroizing::new(v);
                Task::none()
            }

            Message::ChangePassword => {
                if self.settings_old_password.is_empty() {
                    self.error_message = Some("Current password is required".into());
                    return Task::none();
                }
                if self.settings_new_password.is_empty() {
                    self.error_message = Some("New password is required".into());
                    return Task::none();
                }
                if *self.settings_new_password != *self.settings_new_password_confirm {
                    self.error_message = Some("New passwords don't match".into());
                    return Task::none();
                }
                let name = self.selected_wallet.clone().unwrap_or_default();
                let path = self.wallet_dir.join(format!("{name}.wallet"));
                let old_pw = Zeroizing::new(self.settings_old_password.as_bytes().to_vec());
                let new_pw = Zeroizing::new(self.settings_new_password.as_bytes().to_vec());
                self.loading = true;
                self.error_message = None;
                self.success_message = None;

                Task::perform(
                    async move {
                        Wallet::change_password(&path, &old_pw, &new_pw)?;
                        Ok(())
                    },
                    |r: Result<(), anyhow::Error>| {
                        Message::ChangePasswordCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::ChangePasswordCompleted(result) => {
                self.loading = false;
                match result {
                    Ok(()) => {
                        self.success_message = Some("Password changed".into());
                        self.settings_old_password.zeroize();
                        self.settings_new_password.zeroize();
                        self.settings_new_password_confirm.zeroize();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }
        }
    }

    // -- Helpers --

    fn validate_create_form(&self) -> Option<String> {
        if self.password.is_empty() {
            return Some("Password is required".into());
        }
        if self.password != self.password_confirm {
            return Some("Passwords don't match".into());
        }
        if self.wallet_name.trim().is_empty() {
            return Some("Wallet name is required".into());
        }
        None
    }

    fn refresh_dashboard(&mut self) -> Task<Message> {
        let Some(info) = &self.wallet_info else {
            return Task::none();
        };
        self.loading = true;

        let addr1 = info.address;
        let cfg1 = info.network_config.clone();
        let addr2 = info.address;
        let cfg2 = info.network_config.clone();

        Task::batch([
            Task::perform(
                async move {
                    let net = NetworkClient::new(&cfg1, false)?;
                    net.balance(&addr1).await
                },
                |r: Result<u64, anyhow::Error>| {
                    Message::BalanceUpdated(r.map_err(|e| e.to_string()))
                },
            ),
            Task::perform(
                async move {
                    let net = NetworkClient::new(&cfg2, false)?;
                    net.transactions(&addr2, TransactionFilter::All).await
                },
                |r: Result<Vec<TransactionSummary>, anyhow::Error>| {
                    Message::TransactionsLoaded(r.map_err(|e| e.to_string()))
                },
            ),
        ])
    }

    fn load_stakes(&mut self) -> Task<Message> {
        let Some(info) = &self.wallet_info else {
            return Task::none();
        };
        self.loading = true;
        let addr = info.address;
        let config = info.network_config.clone();

        Task::perform(
            async move {
                let net = NetworkClient::new(&config, false)?;
                net.get_stakes(&addr).await
            },
            |r: Result<Vec<StakedIotaSummary>, anyhow::Error>| {
                Message::StakesLoaded(r.map_err(|e| e.to_string()))
            },
        )
    }

    // -- Views --

    fn view(&self) -> Element<Message> {
        match self.screen {
            Screen::WalletSelect | Screen::Unlock | Screen::Create | Screen::Recover => {
                let content = match self.screen {
                    Screen::WalletSelect => self.view_wallet_select(),
                    Screen::Unlock => self.view_unlock(),
                    Screen::Create => self.view_create(),
                    Screen::Recover => self.view_recover(),
                    _ => unreachable!(),
                };
                container(content)
                    .center_x(Fill)
                    .center_y(Fill)
                    .padding(20)
                    .into()
            }
            Screen::Account | Screen::Send | Screen::Receive | Screen::History
            | Screen::Staking | Screen::Settings => self.view_main(),
        }
    }

    fn view_main(&self) -> Element<Message> {
        let sidebar = self.view_sidebar();
        let header = self.view_header();
        let content: Element<Message> = match self.screen {
            Screen::Account => self.view_account(),
            Screen::Send => self.view_send(),
            Screen::Receive => self.view_receive(),
            Screen::History => self.view_history(),
            Screen::Staking => self.view_staking(),
            Screen::Settings => self.view_settings(),
            _ => unreachable!(),
        };

        let separator = container(Space::new().height(1))
            .width(Fill)
            .style(|_theme| container::Style {
                border: iced::Border {
                    color: BORDER,
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            });

        let right = column![header, separator, container(content).padding(20)]
            .width(Fill);

        row![sidebar, right].into()
    }

    fn view_sidebar(&self) -> Element<Message> {
        let nav_btn = |label: &'static str, target: Screen| -> Element<Message> {
            let active = self.screen == target;
            let btn = button(text(label).size(14)).width(Fill);
            let btn = if active {
                btn.style(|theme, status| {
                    let mut style = button::primary(theme, status);
                    style.background =
                        Some(iced::Background::Color(ACTIVE));
                    style
                })
            } else {
                btn.style(button::text)
            };
            btn.on_press(Message::GoTo(target)).into()
        };

        let nav = column![
            nav_btn("Account", Screen::Account),
            nav_btn("Send", Screen::Send),
            nav_btn("Receive", Screen::Receive),
            nav_btn("History", Screen::History),
            nav_btn("Staking", Screen::Staking),
        ]
        .spacing(4);

        let settings = nav_btn("Settings", Screen::Settings);

        let close = button(text("Close Wallet").size(14))
            .width(Fill)
            .on_press(Message::GoTo(Screen::WalletSelect));

        let col = column![nav, Space::new().height(Fill), settings, close]
            .spacing(10)
            .padding(10)
            .width(Length::Fixed(200.0));

        container(col)
            .height(Fill)
            .style(|_theme| container::Style {
                background: Some(iced::Background::Color(SIDEBAR)),
                ..Default::default()
            })
            .into()
    }

    fn view_header(&self) -> Element<Message> {
        let Some(info) = &self.wallet_info else {
            return Space::new().into();
        };

        let wallet_name = self
            .selected_wallet
            .as_deref()
            .unwrap_or("Wallet");
        let name_label = text(wallet_name).size(14);

        let network_badge = text(format!("{}", info.network_config.network)).size(12);

        let bal = match self.balance {
            Some(b) => format_balance(b),
            None => "Loading...".into(),
        };
        let balance_display = text(bal).size(28);

        let addr_short = if info.address_string.len() > 20 {
            format!("{}...{}", &info.address_string[..10], &info.address_string[info.address_string.len() - 8..])
        } else {
            info.address_string.clone()
        };
        let addr_row = row![
            text(addr_short).size(12),
            button(text("Copy").size(11)).on_press(Message::CopyAddress),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let left = column![name_label, balance_display].spacing(2);
        let right = column![network_badge, addr_row].spacing(2).align_x(iced::Alignment::End);

        row![left, Space::new().width(Fill), right]
            .padding(15)
            .align_y(iced::Alignment::Center)
            .into()
    }

    fn view_wallet_select(&self) -> Element<Message> {
        let logo = svg(svg::Handle::from_path("gui/assets/iota-logo.svg"))
            .width(Length::Fixed(200.0));

        let net_btn = |label: &'static str, network: Network| -> Element<Message> {
            let active = self.network_config.network == network;
            let btn = button(text(label).size(12));
            let btn = if active {
                btn.style(|theme, status| {
                    let mut style = button::primary(theme, status);
                    style.background =
                        Some(iced::Background::Color(ACTIVE));
                    style
                })
            } else {
                btn.style(button::text)
            };
            btn.on_press(Message::NetworkChanged(network)).into()
        };

        let network_row = row![
            text("Network:").size(14),
            net_btn("Mainnet", Network::Mainnet),
            net_btn("Testnet", Network::Testnet),
            net_btn("Devnet", Network::Devnet),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center);

        let mut col = column![logo, network_row, Space::new().height(20)]
            .spacing(10)
            .max_width(400);

        if self.wallet_names.is_empty() {
            col = col.push(text("No wallets found.").size(14));
        } else {
            col = col.push(text("Select a wallet:").size(16));
            for name in &self.wallet_names {
                let n = name.clone();
                col = col.push(
                    button(text(name.as_str()).size(14))
                        .on_press(Message::WalletSelected(n))
                        .width(Fill),
                );
            }
        }

        col = col.push(Space::new().height(20));
        col = col.push(
            row![
                button(text("Create New").size(14)).on_press(Message::GoTo(Screen::Create)),
                button(text("Recover").size(14)).on_press(Message::GoTo(Screen::Recover)),
            ]
            .spacing(10),
        );

        col.into()
    }

    fn view_unlock(&self) -> Element<Message> {
        let name = self.selected_wallet.as_deref().unwrap_or("unknown");
        let title = text(format!("Unlock: {name}")).size(24);

        let pw = text_input("Password", &self.password)
            .on_input(Message::PasswordChanged)
            .on_submit(Message::UnlockWallet)
            .secure(true);

        let mut unlock = button(text("Unlock").size(14)).style(button::primary);
        if !self.loading {
            unlock = unlock.on_press(Message::UnlockWallet);
        }

        let back = button(text("Back").size(14)).on_press(Message::GoTo(Screen::WalletSelect));

        let mut col = column![
            title,
            Space::new().height(10),
            pw,
            Space::new().height(10),
            row![back, unlock].spacing(10),
        ]
        .spacing(5)
        .max_width(400);

        if self.loading {
            col = col.push(text("Unlocking...").size(14));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(14).color([0.88, 0.25, 0.31]));
        }

        col.into()
    }

    fn view_create(&self) -> Element<Message> {
        // After creation — show mnemonic
        if let Some(mnemonic) = &self.created_mnemonic {
            return self.view_mnemonic_display(mnemonic);
        }

        let title = text("Create New Wallet").size(24);

        let name = text_input("Wallet name", &self.wallet_name)
            .on_input(Message::WalletNameChanged);
        let pw = text_input("Password", &self.password)
            .on_input(Message::PasswordChanged)
            .secure(true);
        let pw2 = text_input("Confirm password", &self.password_confirm)
            .on_input(Message::PasswordConfirmChanged)
            .on_submit(Message::CreateWallet)
            .secure(true);

        let mut create = button(text("Create").size(14)).style(button::primary);
        if !self.loading && self.validate_create_form().is_none() {
            create = create.on_press(Message::CreateWallet);
        }
        let back = button(text("Back").size(14)).on_press(Message::GoTo(Screen::WalletSelect));

        let mut col = column![
            title,
            Space::new().height(10),
            name,
            pw,
            pw2,
            Space::new().height(10),
            row![back, create].spacing(10),
        ]
        .spacing(5)
        .max_width(400);

        if self.loading {
            col = col.push(text("Creating wallet...").size(14));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(14).color([0.88, 0.25, 0.31]));
        }

        col.into()
    }

    fn view_mnemonic_display(&self, mnemonic: &str) -> Element<Message> {
        let title = text("Write Down Your Mnemonic").size(24);
        let warning = text(
            "Save these 24 words in a safe place. You will need them to recover your wallet.",
        )
        .size(14)
        .color([0.94, 0.63, 0.19]);

        let words: Vec<&str> = mnemonic.split_whitespace().collect();

        // Two-column layout: 1-12 left, 13-24 right
        let mut left = column![].spacing(4);
        let mut right = column![].spacing(4);
        for (i, word) in words.iter().enumerate() {
            let label = text(format!("{:>2}. {}", i + 1, word)).size(14);
            if i < 12 {
                left = left.push(label);
            } else {
                right = right.push(label);
            }
        }
        let word_grid = row![left, Space::new().width(30), right];

        let confirm = button(text("I've saved my mnemonic").size(14))
            .on_press(Message::MnemonicConfirmed);

        column![
            title,
            Space::new().height(10),
            warning,
            Space::new().height(10),
            word_grid,
            Space::new().height(20),
            confirm,
        ]
        .spacing(5)
        .max_width(500)
        .into()
    }

    fn view_recover(&self) -> Element<Message> {
        let title = text("Recover Wallet").size(24);

        let name = text_input("Wallet name", &self.wallet_name)
            .on_input(Message::WalletNameChanged);
        let mnemonic = text_input("24-word mnemonic phrase", &self.mnemonic_input)
            .on_input(Message::MnemonicInputChanged)
            .secure(true);
        let pw = text_input("Password", &self.password)
            .on_input(Message::PasswordChanged)
            .secure(true);
        let pw2 = text_input("Confirm password", &self.password_confirm)
            .on_input(Message::PasswordConfirmChanged)
            .on_submit(Message::RecoverWallet)
            .secure(true);

        let can_submit = !self.loading
            && self.validate_create_form().is_none()
            && !self.mnemonic_input.trim().is_empty();
        let mut recover = button(text("Recover").size(14)).style(button::primary);
        if can_submit {
            recover = recover.on_press(Message::RecoverWallet);
        }
        let back = button(text("Back").size(14)).on_press(Message::GoTo(Screen::WalletSelect));

        let mut col = column![
            title,
            Space::new().height(10),
            name,
            mnemonic,
            pw,
            pw2,
            Space::new().height(10),
            row![back, recover].spacing(10),
        ]
        .spacing(5)
        .max_width(400);

        if self.loading {
            col = col.push(text("Recovering wallet...").size(14));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(14).color([0.88, 0.25, 0.31]));
        }

        col.into()
    }

    fn view_tx_table<'a>(&'a self, txs: &'a [TransactionSummary], expandable: bool) -> Element<'a, Message> {
        let header = row![
            text("Dir").size(11).width(Length::Fixed(35.0)),
            text("Sender").size(11).width(Length::Fixed(140.0)),
            text("Received").size(11).width(Length::Fixed(110.0)),
            text("Sent").size(11).width(Length::Fixed(110.0)),
            text("Digest").size(11),
        ]
        .spacing(8);

        let separator = container(Space::new().height(1))
            .width(Fill)
            .style(|_theme| container::Style {
                border: iced::Border {
                    color: BORDER,
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            });

        let mut tx_col = column![header, separator].spacing(2);

        for (i, tx) in txs.iter().enumerate() {
            let dir_label = match tx.direction {
                Some(TransactionDirection::In) => "in",
                Some(TransactionDirection::Out) => "out",
                None => "",
            };
            let dir_color = match tx.direction {
                Some(TransactionDirection::In) => Color::from_rgb(0.0, 0.80, 0.53),
                Some(TransactionDirection::Out) => Color::from_rgb(0.88, 0.25, 0.31),
                None => MUTED,
            };

            let sender_short = tx
                .sender
                .as_ref()
                .map(|s| {
                    if s.len() > 16 {
                        format!("{}...{}", &s[..8], &s[s.len() - 6..])
                    } else {
                        s.clone()
                    }
                })
                .unwrap_or_else(|| "-".into());

            let (received, sent) = match tx.direction {
                Some(TransactionDirection::In) => (
                    tx.amount
                        .map(|a| nanos_to_iota(a))
                        .unwrap_or_else(|| "-".into()),
                    "-".into(),
                ),
                Some(TransactionDirection::Out) => (
                    "-".into(),
                    tx.amount
                        .map(|a| nanos_to_iota(a))
                        .unwrap_or_else(|| "-".into()),
                ),
                None => ("-".into(), "-".into()),
            };

            let digest_short = if tx.digest.len() > 16 {
                format!("{}...{}", &tx.digest[..8], &tx.digest[tx.digest.len() - 6..])
            } else {
                tx.digest.clone()
            };

            let tx_row = button(
                row![
                    text(dir_label).size(12).color(dir_color).width(Length::Fixed(35.0)),
                    text(sender_short).size(12).width(Length::Fixed(140.0)),
                    text(received).size(12).width(Length::Fixed(110.0)),
                    text(sent).size(12).width(Length::Fixed(110.0)),
                    text(digest_short).size(12),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            )
            .width(Fill)
            .style(|theme, status| {
                let mut style = button::text(theme, status);
                style.background = None;
                style
            })
            .on_press(if expandable {
                Message::ToggleTxDetail(i)
            } else {
                Message::GoTo(Screen::History)
            });

            tx_col = tx_col.push(tx_row);

            // Expanded detail panel
            if expandable && self.expanded_tx == Some(i) {
                let detail_padding = Padding {
                    top: 4.0,
                    right: 0.0,
                    bottom: 8.0,
                    left: 40.0,
                };
                let mut detail = column![].spacing(3).padding(detail_padding);

                if let Some(ref sender) = tx.sender {
                    detail = detail.push(
                        row![
                            text("Sender:").size(11).width(Length::Fixed(60.0)),
                            text(sender.as_str()).size(11),
                        ]
                        .spacing(8),
                    );
                }

                if let Some(amount) = tx.amount {
                    detail = detail.push(
                        row![
                            text("Amount:").size(11).width(Length::Fixed(60.0)),
                            text(format_balance(amount)).size(11),
                        ]
                        .spacing(8),
                    );
                }

                if let Some(fee) = tx.fee {
                    detail = detail.push(
                        row![
                            text("Fee:").size(11).width(Length::Fixed(60.0)),
                            text(format_balance(fee)).size(11),
                        ]
                        .spacing(8),
                    );
                }

                detail = detail.push(
                    row![
                        text("Digest:").size(11).width(Length::Fixed(60.0)),
                        text(&tx.digest).size(11),
                    ]
                    .spacing(8),
                );

                detail = detail.push(
                    row![
                        text("Epoch:").size(11).width(Length::Fixed(60.0)),
                        text(format!("{}", tx.epoch)).size(11),
                    ]
                    .spacing(8),
                );

                let explorer = button(text("View in Explorer").size(11))
                    .on_press(Message::OpenExplorer(tx.digest.clone()));
                detail = detail.push(explorer);

                let detail_container = container(detail)
                    .width(Fill)
                    .style(|_theme| container::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(
                            0.12, 0.12, 0.12,
                        ))),
                        border: iced::Border {
                            color: BORDER,
                            width: 1.0,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    });
                tx_col = tx_col.push(detail_container);
            }
        }

        tx_col.into()
    }

    fn view_account(&self) -> Element<Message> {
        let Some(info) = &self.wallet_info else {
            return text("No wallet loaded").into();
        };

        let title = text("Account").size(24);

        // Actions
        let mut actions = row![
            button(text("Refresh").size(14)).on_press(Message::RefreshBalance),
        ]
        .spacing(10);

        if !info.is_mainnet && info.network_config.network != Network::Custom {
            let mut faucet = button(text("Faucet").size(14));
            if !self.loading {
                faucet = faucet.on_press(Message::RequestFaucet);
            }
            actions = actions.push(faucet);
        }

        let mut col = column![title, Space::new().height(10), actions]
            .spacing(5);

        // Status messages
        if self.loading {
            col = col.push(text("Loading...").size(14));
        }
        if let Some(msg) = &self.status_message {
            col = col.push(text(msg.as_str()).size(12).color([0.0, 0.80, 0.53]));
        }
        if let Some(msg) = &self.success_message {
            col = col.push(text(msg.as_str()).size(14).color([0.0, 0.80, 0.53]));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(14).color([0.88, 0.25, 0.31]));
        }

        // Recent transactions
        col = col.push(Space::new().height(10));
        col = col.push(text("Recent Transactions").size(18));

        if self.transactions.is_empty() {
            col = col.push(text("No transactions yet.").size(14));
        } else {
            let count = self.transactions.len().min(5);
            col = col.push(self.view_tx_table(&self.transactions[..count], false));
            if self.transactions.len() > 5 {
                col = col.push(
                    button(text("View all transactions").size(12))
                        .style(button::text)
                        .on_press(Message::GoTo(Screen::History)),
                );
            }
        }

        scrollable(col).into()
    }

    fn view_send(&self) -> Element<Message> {
        let title = text("Send IOTA").size(24);

        if self.wallet_info.is_none() {
            return text("No wallet loaded").into();
        }

        let bal_label = match self.balance {
            Some(b) => format!("Available: {}", format_balance(b)),
            None => "Balance: loading...".into(),
        };

        let recipient = text_input("Recipient address (0x...)", &self.recipient)
            .on_input(Message::RecipientChanged);
        let amount = text_input("Amount (IOTA)", &self.amount)
            .on_input(Message::AmountChanged)
            .on_submit(Message::ConfirmSend);

        let mut send = button(text("Send").size(14)).style(button::primary);
        if !self.loading && !self.recipient.is_empty() && !self.amount.is_empty() {
            send = send.on_press(Message::ConfirmSend);
        }

        let mut col = column![
            title,
            Space::new().height(10),
            text(bal_label).size(14),
            Space::new().height(10),
            text("Recipient").size(12),
            recipient,
            Space::new().height(5),
            text("Amount").size(12),
            amount,
            Space::new().height(10),
            send,
        ]
        .spacing(5)
        .max_width(500);

        if self.loading {
            col = col.push(text("Sending...").size(14));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(14).color([0.88, 0.25, 0.31]));
        }
        if let Some(msg) = &self.success_message {
            col = col.push(text(msg.as_str()).size(14).color([0.0, 0.80, 0.53]));
        }

        col.into()
    }

    fn view_receive(&self) -> Element<Message> {
        let Some(info) = &self.wallet_info else {
            return text("No wallet loaded").into();
        };

        let title = text("Receive IOTA").size(24);

        let addr_container = container(
            text(&info.address_string).size(14),
        )
        .padding(15)
        .width(Fill)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                color: BORDER,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        });

        let copy = button(text("Copy Address").size(14)).on_press(Message::CopyAddress);

        let mut col = column![
            title,
            Space::new().height(10),
            text("Your address").size(12),
            addr_container,
            Space::new().height(10),
            copy,
        ]
        .spacing(5)
        .max_width(600);

        if let Some(msg) = &self.status_message {
            col = col.push(text(msg.as_str()).size(12).color([0.0, 0.80, 0.53]));
        }

        col.into()
    }

    fn view_history(&self) -> Element<Message> {
        let title = text("Transaction History").size(24);

        let mut col = column![title, Space::new().height(10)].spacing(5);

        if self.transactions.is_empty() {
            col = col.push(text("No transactions yet.").size(14));
        } else {
            col = col.push(self.view_tx_table(&self.transactions, true));
        }

        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(14).color([0.88, 0.25, 0.31]));
        }

        scrollable(col).into()
    }

    fn view_staking(&self) -> Element<Message> {
        let title = text("Staking").size(24);

        let mut col = column![
            title,
            Space::new().height(10),
        ]
        .spacing(5);

        // -- Active stakes --
        col = col.push(text("Active Stakes").size(18));

        let refresh = button(text("Refresh").size(14)).on_press(Message::RefreshStakes);
        col = col.push(refresh);

        if self.loading && self.stakes.is_empty() {
            col = col.push(text("Loading...").size(14));
        } else if self.stakes.is_empty() {
            col = col.push(text("No active stakes.").size(14));
        } else {
            let header = row![
                text("Principal").size(11).width(Length::Fixed(110.0)),
                text("Reward").size(11).width(Length::Fixed(110.0)),
                text("Epoch").size(11).width(Length::Fixed(60.0)),
                text("Status").size(11).width(Length::Fixed(70.0)),
                text("").size(11),
            ]
            .spacing(8);
            col = col.push(header);

            let separator = container(Space::new().height(1))
                .width(Fill)
                .style(|_theme| container::Style {
                    border: iced::Border {
                        color: BORDER,
                        width: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                });
            col = col.push(separator);

            let mut total_principal: u64 = 0;
            let mut total_reward: u64 = 0;

            let mut stakes_col = column![].spacing(4);
            for stake in &self.stakes {
                total_principal = total_principal.saturating_add(stake.principal);

                let reward_str = match stake.estimated_reward {
                    Some(r) => {
                        total_reward = total_reward.saturating_add(r);
                        format_balance(r)
                    }
                    None => "-".into(),
                };

                let status_color = match stake.status {
                    StakeStatus::Active => Color::from_rgb(0.0, 0.80, 0.53),
                    StakeStatus::Pending => Color::from_rgb(0.94, 0.63, 0.19),
                    StakeStatus::Unstaked => MUTED,
                };

                let mut stake_row = row![
                    text(format_balance(stake.principal)).size(12).width(Length::Fixed(110.0)),
                    text(reward_str).size(12).width(Length::Fixed(110.0)),
                    text(format!("{}", stake.stake_activation_epoch)).size(12).width(Length::Fixed(60.0)),
                    text(format!("{}", stake.status)).size(12).color(status_color).width(Length::Fixed(70.0)),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center);

                if stake.status != StakeStatus::Unstaked {
                    let mut unstake_btn = button(text("Unstake").size(11));
                    if !self.loading {
                        unstake_btn = unstake_btn
                            .on_press(Message::ConfirmUnstake(stake.object_id.to_string()));
                    }
                    stake_row = stake_row.push(unstake_btn);
                }

                stakes_col = stakes_col.push(stake_row);
            }
            col = col.push(stakes_col);

            col = col.push(Space::new().height(5));
            col = col.push(
                text(format!(
                    "Total: {}  rewards: {}",
                    format_balance(total_principal),
                    format_balance(total_reward),
                ))
                .size(13),
            );
        }

        // -- New stake form --
        col = col.push(Space::new().height(20));
        col = col.push(text("New Stake").size(18));

        let validator = text_input("Validator address (0x...)", &self.validator_address)
            .on_input(Message::ValidatorAddressChanged);
        let amount = text_input("Amount (IOTA)", &self.stake_amount)
            .on_input(Message::StakeAmountChanged)
            .on_submit(Message::ConfirmStake);

        let mut stake_btn = button(text("Stake").size(14)).style(button::primary);
        if !self.loading && !self.validator_address.is_empty() && !self.stake_amount.is_empty()
        {
            stake_btn = stake_btn.on_press(Message::ConfirmStake);
        }

        col = col.push(text("Validator").size(12));
        col = col.push(validator);
        col = col.push(Space::new().height(3));
        col = col.push(text("Amount").size(12));
        col = col.push(amount);
        col = col.push(Space::new().height(5));
        col = col.push(stake_btn);

        // Status messages
        if self.loading && !self.stakes.is_empty() {
            col = col.push(text("Processing...").size(14));
        }
        if let Some(msg) = &self.success_message {
            col = col.push(text(msg.as_str()).size(14).color([0.0, 0.80, 0.53]));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(14).color([0.88, 0.25, 0.31]));
        }

        scrollable(col).into()
    }

    fn view_settings(&self) -> Element<Message> {
        let title = text("Settings").size(24);

        let active_network = self
            .wallet_info
            .as_ref()
            .map(|i| &i.network_config.network)
            .unwrap_or(&self.network_config.network);

        let net_btn = |label: &'static str, network: Network| -> Element<Message> {
            let active = *active_network == network;
            let btn = button(text(label).size(14));
            let btn = if active {
                btn.style(|theme, status| {
                    let mut style = button::primary(theme, status);
                    style.background =
                        Some(iced::Background::Color(ACTIVE));
                    style
                })
            } else {
                btn.style(button::text)
            };
            btn.on_press(Message::NetworkChanged(network)).into()
        };

        let network_row = row![
            net_btn("Mainnet", Network::Mainnet),
            net_btn("Testnet", Network::Testnet),
            net_btn("Devnet", Network::Devnet),
        ]
        .spacing(8);

        let mut col = column![
            title,
            Space::new().height(15),
            text("Network").size(16),
            Space::new().height(5),
            network_row,
        ]
        .spacing(5)
        .max_width(500);

        if self.wallet_info.is_some() {
            col = col.push(
                text("Changing network applies to the current session only.")
                    .size(12)
                    .color([0.314, 0.408, 0.533]),
            );

            // -- Change password --
            col = col.push(Space::new().height(20));
            col = col.push(text("Change Password").size(16));
            col = col.push(Space::new().height(5));

            let old_pw = text_input("Current password", &self.settings_old_password)
                .on_input(Message::SettingsOldPasswordChanged)
                .secure(true);
            let new_pw = text_input("New password", &self.settings_new_password)
                .on_input(Message::SettingsNewPasswordChanged)
                .secure(true);
            let new_pw2 = text_input("Confirm new password", &self.settings_new_password_confirm)
                .on_input(Message::SettingsNewPasswordConfirmChanged)
                .on_submit(Message::ChangePassword)
                .secure(true);

            let can_submit = !self.loading
                && !self.settings_old_password.is_empty()
                && !self.settings_new_password.is_empty()
                && *self.settings_new_password == *self.settings_new_password_confirm;
            let mut change_btn = button(text("Change Password").size(14));
            if can_submit {
                change_btn = change_btn.on_press(Message::ChangePassword);
            }

            col = col.push(old_pw);
            col = col.push(new_pw);
            col = col.push(new_pw2);
            col = col.push(Space::new().height(5));
            col = col.push(change_btn);

            if self.loading {
                col = col.push(text("Changing password...").size(14));
            }
            if let Some(msg) = &self.success_message {
                col = col.push(text(msg.as_str()).size(14).color([0.0, 0.80, 0.53]));
            }
            if let Some(err) = &self.error_message {
                col = col.push(text(err.as_str()).size(14).color([0.88, 0.25, 0.31]));
            }
        }

        col.into()
    }
}

