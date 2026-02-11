use crate::messages::Message;
use crate::state::Screen;
use crate::{App, ACTIVE};
use iced::widget::{button, column, row, svg, text, Space};
use iced::{Element, Fill, Length};
use iota_wallet_core::wallet::Network;

impl App {
    pub(crate) fn view_wallet_select(&self) -> Element<Message> {
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
}
