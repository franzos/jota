use crate::messages::Message;
use crate::state::Screen;
use crate::{styles, App, MUTED};
use iced::widget::{button, column, row, svg, text, Space};
use iced::{Element, Fill, Length};
use jota_core::wallet::{HardwareKind, Network, WalletType};

impl App {
    pub(crate) fn view_wallet_select(&self) -> Element<'_, Message> {
        let logo = svg(svg::Handle::from_memory(include_bytes!(
            "../../assets/iota-logo.svg"
        )))
        .width(Length::Fixed(200.0));

        let net_btn = |label: &'static str, network: Network| -> Element<'_, Message> {
            let active = self.network_config.network == network;
            button(text(label).size(12))
                .padding([6, 12])
                .style(styles::toggle_btn(active))
                .on_press(Message::NetworkChanged(network))
                .into()
        };

        let network_row = row![
            text("Network:").size(14).color(MUTED),
            net_btn("Mainnet", Network::Mainnet),
            net_btn("Testnet", Network::Testnet),
            net_btn("Devnet", Network::Devnet),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center);

        let mut col = column![logo, network_row, Space::new().height(12)]
            .spacing(10)
            .max_width(400);

        if self.wallet_entries.is_empty() {
            col = col.push(text("No wallets found.").size(14).color(MUTED));
        } else {
            col = col.push(text("Select a wallet:").size(16));
            for entry in &self.wallet_entries {
                let n = entry.name.clone();
                let mut btn_row = row![text(&entry.name).size(14)]
                    .spacing(8)
                    .align_y(iced::Alignment::Center)
                    .width(Fill);
                if let WalletType::Hardware(kind) = entry.wallet_type {
                    btn_row = btn_row.push(Space::new().width(Fill));
                    btn_row = btn_row.push(hardware_icon(kind));
                }
                col = col.push(
                    button(btn_row)
                        .on_press(Message::WalletSelected(n))
                        .padding([10, 16])
                        .style(styles::btn_secondary)
                        .width(Fill),
                );
            }
        }

        col = col.push(Space::new().height(12));

        #[allow(unused_mut)]
        let mut action_row = row![
            button(text("Create New").size(14))
                .padding([10, 20])
                .style(styles::btn_primary)
                .on_press(Message::GoTo(Screen::Create)),
            button(text("Recover").size(14))
                .padding([10, 20])
                .style(styles::btn_secondary)
                .on_press(Message::GoTo(Screen::Recover)),
        ]
        .spacing(10);

        #[cfg(feature = "hardware-wallets")]
        {
            action_row = action_row.push(
                button(text("Hardware Wallet").size(14))
                    .padding([10, 20])
                    .style(styles::btn_secondary)
                    .on_press(Message::GoTo(Screen::HardwareConnect)),
            );
        }

        col = col.push(action_row);

        col.into()
    }
}

fn hardware_icon(kind: HardwareKind) -> Element<'static, Message> {
    match kind {
        HardwareKind::Ledger => svg(svg::Handle::from_memory(include_bytes!(
            "../../assets/ledger-logo.svg"
        )))
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .style(|_theme, _status| svg::Style {
            color: Some(iced::Color::WHITE),
        })
        .into(),
    }
}
