use crate::messages::Message;
use crate::state::Screen;
use crate::App;
use iced::widget::{button, canvas, column, row, scrollable, text, Space};
use iced::{Element, Fill, Length};
use iota_wallet_core::wallet::Network;

impl App {
    pub(crate) fn view_account(&self) -> Element<Message> {
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
            col = col.push(text(msg.as_str()).size(12).color([0.059, 0.757, 0.718]));
        }
        if let Some(msg) = &self.success_message {
            col = col.push(text(msg.as_str()).size(14).color([0.059, 0.757, 0.718]));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(14).color([0.906, 0.192, 0.192]));
        }

        // Balance chart
        if !self.balance_chart.data.is_empty() {
            col = col.push(Space::new().height(10));
            col = col.push(text("Balance History").size(18));
            col = col.push(
                canvas::Canvas::new(&self.balance_chart)
                    .width(Fill)
                    .height(Length::Fixed(200.0))
            );
        }

        // Recent transactions
        col = col.push(Space::new().height(10));
        col = col.push(text("Recent Transactions").size(18));

        if self.account_transactions.is_empty() {
            col = col.push(text("No transactions yet.").size(14));
        } else {
            let count = self.account_transactions.len().min(5);
            col = col.push(self.view_tx_table(&self.account_transactions[..count], false));
            if self.account_transactions.len() > 5 {
                col = col.push(
                    button(text("View all transactions").size(12))
                        .style(button::text)
                        .on_press(Message::GoTo(Screen::History)),
                );
            }
        }

        scrollable(col).into()
    }
}
