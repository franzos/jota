mod chart;
mod helpers;
mod messages;
mod state;
mod styles;
mod update;
mod views;

use iced::theme::Palette;
use iced::widget::{
    button, column, container, pick_list, qr_code, row, scrollable, text, text_input, Space,
};
use iced::{Color, Element, Fill, Font, Length, Task, Theme};

use std::path::PathBuf;
use zeroize::Zeroizing;

use iota_wallet_core::display::{format_balance, format_balance_with_symbol};
use iota_wallet_core::network::{
    CoinMeta, NftSummary, StakedIotaSummary, TokenBalance, TransactionSummary,
};
use iota_wallet_core::wallet::{Network, NetworkConfig};
use iota_wallet_core::SignedMessage;
use iota_wallet_core::{list_wallets, WalletEntry};

use chart::BalanceChart;
use messages::Message;
use state::{Screen, SignMode, WalletInfo};

// IOTA Explorer dark-mode palette (iota2.darkmode)
const BG: Color = Color::from_rgb(0.051, 0.067, 0.090); // #0d1117
const SIDEBAR: Color = Color::from_rgb(0.024, 0.039, 0.063); // #060a10
const SURFACE: Color = Color::from_rgb(0.114, 0.157, 0.227); // #1d283a (iota2-gray-800)
const BORDER: Color = Color::from_rgb(0.204, 0.259, 0.337); // #344256 (iota2-gray-700)
const ACTIVE: Color = Color::from_rgb(0.086, 0.137, 0.251); // #162340
const MUTED: Color = Color::from_rgb(0.396, 0.459, 0.545); // #65758b (iota2-gray-500)
const PRIMARY: Color = Color::from_rgb(0.145, 0.349, 0.961); // #2559f5 (iota2-blue-600)

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("IOTA Wallet")
        .theme(App::theme)
        .run()
}

// -- App state --

struct App {
    screen: Screen,
    wallet_dir: PathBuf,
    wallet_entries: Vec<WalletEntry>,
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
    account_transactions: Vec<TransactionSummary>,
    epoch_deltas: Vec<(u64, i64)>,
    balance_chart: BalanceChart,

    // History
    expanded_tx: Option<usize>,
    history_page: u32,
    history_total: u32,

    // Name resolution
    resolved_recipient: Option<Result<String, String>>,
    resolved_validator: Option<Result<String, String>>,

    // Staking
    stakes: Vec<StakedIotaSummary>,
    validator_address: String,
    stake_amount: String,

    // Sign / Verify / Notarize
    sign_message_input: String,
    sign_mode: SignMode,
    signed_result: Option<SignedMessage>,
    verify_message_input: String,
    verify_signature_input: String,
    verify_public_key_input: String,
    verify_result: Option<bool>,
    notarize_description: String,
    notarize_result: Option<String>,

    // Settings — password change
    settings_old_password: Zeroizing<String>,
    settings_new_password: Zeroizing<String>,
    settings_new_password_confirm: Zeroizing<String>,

    // UI state
    loading: u32,
    error_message: Option<String>,
    success_message: Option<String>,
    status_message: Option<String>,

    // Account switching
    account_input: String,
    session_password: Zeroizing<Vec<u8>>,

    // Token balances and metadata
    token_balances: Vec<TokenBalance>,
    token_meta: Vec<CoinMeta>,
    selected_token: Option<TokenOption>,

    // NFTs
    nfts: Vec<NftSummary>,
    send_nft_object_id: Option<String>,
    send_nft_recipient: String,

    // QR code for receive screen
    qr_data: Option<qr_code::Data>,

    // Persistent clipboard (Linux requires the instance to stay alive)
    clipboard: Option<arboard::Clipboard>,

    // Cached theme (avoids re-allocating every frame)
    theme: Theme,
}

/// Display type for the account picker dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AccountOption(u64);

impl std::fmt::Display for AccountOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Display type for the token picker dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TokenOption {
    coin_type: String,
    symbol: String,
    balance_display: String,
}

impl TokenOption {
    fn iota(balance: Option<u64>) -> Self {
        Self {
            coin_type: "0x2::iota::IOTA".to_string(),
            symbol: "IOTA".to_string(),
            balance_display: match balance {
                Some(b) => format_balance(b),
                None => "IOTA".to_string(),
            },
        }
    }

    fn from_token_balance(tb: &TokenBalance, meta: Option<&CoinMeta>) -> Self {
        if tb.coin_type == "0x2::iota::IOTA" {
            return Self {
                coin_type: tb.coin_type.clone(),
                symbol: "IOTA".to_string(),
                balance_display: format_balance(tb.total_balance),
            };
        }

        let (symbol, balance_str) = match meta {
            Some(m) => {
                let sym = if m.symbol.is_empty() {
                    tb.coin_type
                        .split("::")
                        .last()
                        .unwrap_or(&tb.coin_type)
                        .to_string()
                } else {
                    m.symbol.clone()
                };
                let display = format_balance_with_symbol(tb.total_balance, m.decimals, &sym);
                (sym, display)
            }
            None => {
                let sym = tb
                    .coin_type
                    .split("::")
                    .last()
                    .unwrap_or(&tb.coin_type)
                    .to_string();
                (sym.clone(), format!("{} {}", tb.total_balance, sym))
            }
        };

        Self {
            coin_type: tb.coin_type.clone(),
            symbol,
            balance_display: balance_str,
        }
    }

    fn is_iota(&self) -> bool {
        self.coin_type == "0x2::iota::IOTA"
    }
}

impl std::fmt::Display for TokenOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.balance_display)
    }
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let network_config = Self::parse_network_from_args();
        let wallet_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".iota-wallet");
        let wallet_entries = list_wallets(&wallet_dir);

        let app = Self {
            screen: Screen::WalletSelect,
            wallet_dir,
            wallet_entries,
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
            account_transactions: Vec::new(),
            epoch_deltas: Vec::new(),
            balance_chart: BalanceChart::new(),
            expanded_tx: None,
            history_page: 0,
            history_total: 0,
            resolved_recipient: None,
            resolved_validator: None,
            stakes: Vec::new(),
            validator_address: String::new(),
            stake_amount: String::new(),
            sign_message_input: String::new(),
            sign_mode: SignMode::Sign,
            signed_result: None,
            verify_message_input: String::new(),
            verify_signature_input: String::new(),
            verify_public_key_input: String::new(),
            verify_result: None,
            notarize_description: String::new(),
            notarize_result: None,
            settings_old_password: Zeroizing::new(String::new()),
            settings_new_password: Zeroizing::new(String::new()),
            settings_new_password_confirm: Zeroizing::new(String::new()),
            loading: 0,
            account_input: String::new(),
            session_password: Zeroizing::new(Vec::new()),
            token_balances: Vec::new(),
            token_meta: Vec::new(),
            selected_token: None,
            nfts: Vec::new(),
            send_nft_object_id: None,
            send_nft_recipient: String::new(),
            qr_data: None,
            clipboard: arboard::Clipboard::new()
                .map_err(|e| eprintln!("clipboard init failed: {e}"))
                .ok(),
            error_message: None,
            success_message: None,
            status_message: None,
            theme: Theme::custom(
                "IOTA".to_string(),
                Palette {
                    background: BG,
                    text: Color::from_rgb(0.988, 0.988, 0.988),
                    primary: PRIMARY,
                    success: Color::from_rgb(0.059, 0.757, 0.718),
                    warning: Color::from_rgb(1.0, 0.757, 0.027),
                    danger: Color::from_rgb(0.906, 0.192, 0.192),
                },
            ),
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
        self.theme.clone()
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
                container(container(content).padding(32).style(styles::card))
                    .center_x(Fill)
                    .center_y(Fill)
                    .padding(20)
                    .into()
            }
            #[cfg(feature = "hardware-wallets")]
            Screen::HardwareConnect => {
                let content = self.view_hardware_connect();
                container(container(content).padding(32).style(styles::card))
                    .center_x(Fill)
                    .center_y(Fill)
                    .padding(20)
                    .into()
            }
            Screen::Account
            | Screen::Send
            | Screen::Receive
            | Screen::History
            | Screen::Staking
            | Screen::Nfts
            | Screen::Sign
            | Screen::Settings => self.view_main(),
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
            Screen::Nfts => self.view_nfts(),
            Screen::Sign => self.view_sign(),
            Screen::Settings => self.view_settings(),
            _ => unreachable!(),
        };

        let right = column![
            header,
            styles::separator(),
            scrollable(container(content).padding(24).width(Fill)).height(Fill),
        ]
        .width(Fill);

        row![sidebar, right].into()
    }

    fn view_sidebar(&self) -> Element<Message> {
        let nav_btn =
            |icon: &'static str, label: &'static str, target: Screen| -> Element<Message> {
                let active = self.screen == target;
                button(
                    row![
                        text(icon).size(18).width(Length::Fixed(26.0)),
                        text(label).size(14),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                )
                .width(Fill)
                .padding([10, 14])
                .style(styles::nav_btn(active))
                .on_press(Message::GoTo(target))
                .into()
            };

        let nav = column![
            nav_btn("◉", "Account", Screen::Account),
            nav_btn("↗", "Send", Screen::Send),
            nav_btn("↙", "Receive", Screen::Receive),
            nav_btn("≡", "History", Screen::History),
            nav_btn("◆", "Staking", Screen::Staking),
            nav_btn("▣", "NFTs", Screen::Nfts),
            nav_btn("✎", "Sign", Screen::Sign),
        ]
        .spacing(2);

        let settings = nav_btn("⚙", "Settings", Screen::Settings);

        let close = button(
            row![
                text("✕")
                    .size(18)
                    .width(Length::Fixed(26.0))
                    .color(styles::DANGER),
                text("Close Wallet").size(14),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .width(Fill)
        .padding([10, 14])
        .style(styles::btn_ghost)
        .on_press(Message::GoTo(Screen::WalletSelect));

        let col = column![
            nav,
            Space::new().height(Fill),
            styles::separator(),
            Space::new().height(8),
            settings,
            close,
        ]
        .spacing(2)
        .padding(12)
        .width(Length::Fixed(210.0));

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

        let wallet_name = self.selected_wallet.as_deref().unwrap_or("Wallet");
        let name_label = text(wallet_name).size(13).color(MUTED);

        let bal = match self.balance {
            Some(b) => format_balance(b),
            None => "Loading...".into(),
        };
        let balance_display = text(bal).size(30).font(styles::BOLD);

        let addr_short = if info.address_string.len() > 20 {
            format!(
                "{}...{}",
                &info.address_string[..10],
                &info.address_string[info.address_string.len() - 8..]
            )
        } else {
            info.address_string.clone()
        };
        let addr_row = row![
            text(addr_short).size(12).font(Font::MONOSPACE).color(MUTED),
            button(text("Copy").size(11))
                .padding([4, 10])
                .style(styles::btn_secondary)
                .on_press(Message::CopyAddress),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let network_label = format!("{}", info.network_config.network);
        let network_badge = container(text(network_label).size(11))
            .padding([3, 10])
            .style(styles::pill);

        let left = column![name_label, balance_display].spacing(2);
        let right = column![network_badge, addr_row]
            .spacing(6)
            .align_x(iced::Alignment::End);

        let main_row = row![left, Space::new().width(Fill), right]
            .padding(15)
            .align_y(iced::Alignment::Center);

        // -- Account toolbar --
        let account_idx = info.account_index;
        let acct_label = if let Some(kind) = info.hardware_kind {
            format!("Account {wallet_name} #{account_idx} ({kind})")
        } else {
            format!("Account {wallet_name} #{account_idx}")
        };
        let label = text(acct_label).size(12);
        let divider = text("·").size(12).color(MUTED);

        let go_label = text("Jump to:").size(11).color(MUTED);
        let go_input = text_input("#", &self.account_input)
            .on_input(Message::AccountInputChanged)
            .on_submit(Message::AccountGoPressed)
            .size(11)
            .width(Length::Fixed(48.0));
        let go_btn = button(text("Go").size(11))
            .padding([4, 10])
            .style(styles::btn_secondary)
            .on_press(Message::AccountGoPressed);

        let divider2 = text("·").size(12).color(MUTED);

        let select_label = text("Select").size(11).color(MUTED);
        let options: Vec<AccountOption> = info
            .known_accounts
            .iter()
            .map(|a| AccountOption(a.index))
            .collect();
        let selected = Some(AccountOption(account_idx));
        let dropdown = pick_list(options, selected, |opt| Message::AccountIndexChanged(opt.0))
            .text_size(11)
            .width(Length::Fixed(72.0));

        let toolbar = row![
            label,
            divider,
            go_label,
            go_input,
            go_btn,
            divider2,
            select_label,
            dropdown,
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let toolbar_bar = container(toolbar)
            .width(Fill)
            .padding([6, 15])
            .style(|_theme| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                ..Default::default()
            });

        column![main_row, toolbar_bar].into()
    }
}
