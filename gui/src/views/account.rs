use crate::messages::Message;
use crate::state::Screen;
use crate::{styles, App, MUTED};
use iced::widget::{button, canvas, column, container, row, text, Space};
use iced::{Element, Fill, Length};
use iota_wallet_core::display::format_balance_with_symbol;
use iota_wallet_core::wallet::Network;

impl App {
    pub(crate) fn view_account(&self) -> Element<Message> {
        let Some(info) = &self.wallet_info else {
            return text("No wallet loaded").into();
        };

        let title = text("Account").size(24);

        let mut actions = row![
            button(text("Refresh").size(13))
                .padding([8, 16])
                .style(styles::btn_secondary)
                .on_press(Message::RefreshBalance),
        ]
        .spacing(8);

        if !info.is_mainnet && info.network_config.network != Network::Custom {
            let mut faucet = button(text("Faucet").size(13))
                .padding([8, 16])
                .style(styles::btn_secondary);
            if self.loading == 0 {
                faucet = faucet.on_press(Message::RequestFaucet);
            }
            actions = actions.push(faucet);
        }

        let header = row![title, Space::new().width(Fill), actions]
            .align_y(iced::Alignment::Center);

        let mut col = column![header].spacing(16);

        // Status messages
        if self.loading > 0 {
            col = col.push(text("Loading...").size(13).color(MUTED));
        }
        if let Some(msg) = &self.status_message {
            col = col.push(text(msg.as_str()).size(13).color(styles::ACCENT));
        }
        if let Some(msg) = &self.success_message {
            col = col.push(text(msg.as_str()).size(13).color(styles::ACCENT));
        }
        if let Some(err) = &self.error_message {
            let mut error_row = row![text(err.as_str()).size(13).color(styles::DANGER)]
                .spacing(8)
                .align_y(iced::Alignment::Center);
            #[cfg(feature = "hardware-wallets")]
            if info.is_hardware && self.loading == 0 {
                error_row = error_row.push(
                    button(text("Reconnect Device").size(12))
                        .padding([4, 10])
                        .style(styles::btn_secondary)
                        .on_press(Message::HardwareReconnect),
                );
            }
            col = col.push(error_row);
        }

        // Balance chart card
        if !self.balance_chart.data.is_empty() {
            let chart_content = column![
                text("Balance History").size(16),
                canvas::Canvas::new(&self.balance_chart)
                    .width(Fill)
                    .height(Length::Fixed(200.0)),
            ]
            .spacing(12);

            col = col.push(
                container(chart_content)
                    .padding(20)
                    .width(Fill)
                    .style(styles::card),
            );
        }

        // Token balances card (non-IOTA tokens only)
        let non_iota_tokens: Vec<_> = self.token_balances.iter()
            .filter(|b| b.coin_type != "0x2::iota::IOTA")
            .collect();
        if !non_iota_tokens.is_empty() {
            let mut token_content = column![
                text("Token Balances").size(16),
            ]
            .spacing(8);

            for tb in &non_iota_tokens {
                let meta = self.token_meta.iter().find(|m| m.coin_type == tb.coin_type);
                let symbol = meta.map(|m| m.symbol.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| tb.coin_type.split("::").last().unwrap_or(&tb.coin_type));
                let balance_str = match meta {
                    Some(m) => format_balance_with_symbol(tb.total_balance, m.decimals, symbol),
                    None => format!("{} {}", tb.total_balance, symbol),
                };
                let objects = if tb.coin_object_count == 1 { "1 object" } else { &format!("{} objects", tb.coin_object_count) };
                token_content = token_content.push(
                    row![
                        text(symbol).size(14).font(styles::BOLD).width(Length::Fixed(100.0)),
                        text(balance_str).size(14),
                        text(format!("({})", objects)).size(12).color(MUTED),
                    ]
                    .spacing(12)
                    .align_y(iced::Alignment::Center),
                );
            }

            col = col.push(
                container(token_content)
                    .padding(20)
                    .width(Fill)
                    .style(styles::card),
            );
        }

        // Recent transactions card
        let mut tx_content = column![
            text("Recent Transactions").size(16),
        ]
        .spacing(12);

        if self.account_transactions.is_empty() {
            tx_content = tx_content.push(text("No transactions yet.").size(14).color(MUTED));
        } else {
            let count = self.account_transactions.len().min(5);
            tx_content =
                tx_content.push(self.view_tx_table(&self.account_transactions[..count], false));
            if self.account_transactions.len() > 5 {
                tx_content = tx_content.push(
                    button(text("View all transactions →").size(12))
                        .style(styles::btn_ghost)
                        .on_press(Message::GoTo(Screen::History)),
                );
            }
        }

        col = col.push(
            container(tx_content)
                .padding(20)
                .width(Fill)
                .style(styles::card),
        );

        col.into()
    }
}
