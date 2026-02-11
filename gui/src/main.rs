mod chart;
mod helpers;
mod messages;
mod state;
mod update;
mod views;

use iced::widget::{button, column, container, row, text, Space};
use iced::theme::Palette;
use iced::{Color, Element, Fill, Length, Task, Theme};

use std::path::PathBuf;
use zeroize::Zeroizing;

use iota_wallet_core::display::format_balance;
use iota_wallet_core::list_wallets;
use iota_wallet_core::network::{StakedIotaSummary, TransactionSummary};
use iota_wallet_core::wallet::{Network, NetworkConfig};

use chart::BalanceChart;
use messages::Message;
use state::{Screen, WalletInfo};

// IOTA Explorer dark-mode palette (iota2.darkmode)
const BG:      Color = Color::from_rgb(0.051, 0.067, 0.090); // #0d1117
const SIDEBAR: Color = Color::from_rgb(0.024, 0.039, 0.063); // #060a10
const SURFACE: Color = Color::from_rgb(0.114, 0.157, 0.227); // #1d283a (iota2-gray-800)
const BORDER:  Color = Color::from_rgb(0.204, 0.259, 0.337); // #344256 (iota2-gray-700)
const ACTIVE:  Color = Color::from_rgb(0.086, 0.137, 0.251); // #162340
const MUTED:   Color = Color::from_rgb(0.396, 0.459, 0.545); // #65758b (iota2-gray-500)
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
    account_transactions: Vec<TransactionSummary>,
    epoch_deltas: Vec<(u64, i64)>,
    balance_chart: BalanceChart,

    // History
    expanded_tx: Option<usize>,
    history_page: u32,
    history_total: u32,

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

    // Cached theme (avoids re-allocating every frame)
    theme: Theme,
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
            account_transactions: Vec::new(),
            epoch_deltas: Vec::new(),
            balance_chart: BalanceChart::new(),
            expanded_tx: None,
            history_page: 0,
            history_total: 0,
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
            theme: Theme::custom("IOTA".to_string(), Palette {
                background: BG,
                text: Color::from_rgb(0.988, 0.988, 0.988),
                primary: PRIMARY,
                success: Color::from_rgb(0.059, 0.757, 0.718),
                warning: Color::from_rgb(1.0, 0.757, 0.027),
                danger: Color::from_rgb(0.906, 0.192, 0.192),
            }),
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
}
