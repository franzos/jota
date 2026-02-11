use crate::messages::Message;
use crate::App;
use iced::widget::{button, column, text, text_input, Space};
use iced::Element;
use iota_wallet_core::display::format_balance;

impl App {
    pub(crate) fn view_send(&self) -> Element<Message> {
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
            col = col.push(text(err.as_str()).size(14).color([0.906, 0.192, 0.192]));
        }
        if let Some(msg) = &self.success_message {
            col = col.push(text(msg.as_str()).size(14).color([0.059, 0.757, 0.718]));
        }

        col.into()
    }
}
