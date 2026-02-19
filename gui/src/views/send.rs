use crate::messages::Message;
use crate::{styles, App, TokenOption, MUTED};
use iced::widget::{button, column, container, pick_list, row, text, text_input, Space};
use iced::{Element, Fill, Length};
use jota_core::display::format_balance;

impl App {
    pub(crate) fn view_send(&self) -> Element<'_, Message> {
        if self.wallet_info.is_none() {
            return text("No wallet loaded").into();
        }

        let title = text("Send").size(24);

        let bal_label = match self.balance {
            Some(b) => format!("Available: {}", format_balance(b)),
            None => "Balance: loading...".into(),
        };

        // Token picker
        let token_options: Vec<TokenOption> = if self.token_balances.is_empty() {
            vec![TokenOption::iota(self.balance)]
        } else {
            self.token_balances
                .iter()
                .map(|tb| {
                    let meta = self.token_meta.iter().find(|m| m.coin_type == tb.coin_type);
                    TokenOption::from_token_balance(tb, meta)
                })
                .collect()
        };
        let selected = self
            .selected_token
            .clone()
            .unwrap_or_else(|| TokenOption::iota(self.balance));
        let token_picker = pick_list(token_options, Some(selected), Message::TokenSelected)
            .text_size(13)
            .width(Length::Fixed(280.0));

        let recipient = text_input("Recipient address or .iota name", &self.recipient)
            .on_input(Message::RecipientChanged);

        // Show resolved address or error below the input
        let resolved_hint: Option<Element<Message>> = match &self.resolved_recipient {
            Some(Ok(addr)) => Some(
                text(format!("Resolved: {addr}"))
                    .size(11)
                    .color(styles::ACCENT)
                    .into(),
            ),
            Some(Err(e)) => Some(text(e.as_str()).size(11).color(styles::DANGER).into()),
            None => None,
        };

        let token_symbol = self
            .selected_token
            .as_ref()
            .map(|t| t.symbol.as_str())
            .unwrap_or("IOTA");
        let amount_placeholder = format!("Amount ({token_symbol})");
        let amount = text_input(&amount_placeholder, &self.amount)
            .on_input(Message::AmountChanged)
            .on_submit(Message::ConfirmSend);

        let mut send = button(text("Send").size(14))
            .padding([10, 24])
            .style(styles::btn_primary);
        if self.loading == 0 && !self.recipient.is_empty() && !self.amount.is_empty() {
            send = send.on_press(Message::ConfirmSend);
        }

        let mut form = column![
            text(bal_label).size(14).font(styles::BOLD),
            Space::new().height(4),
            text("Token").size(12).color(MUTED),
            token_picker,
            Space::new().height(4),
            text("Recipient").size(12).color(MUTED),
            recipient,
        ]
        .spacing(4);
        if let Some(hint) = resolved_hint {
            form = form.push(hint);
        }
        form = form
            .push(Space::new().height(4))
            .push(text("Amount").size(12).color(MUTED))
            .push(amount)
            .push(Space::new().height(12))
            .push(send);

        let header = row![title, Space::new().width(Fill)].align_y(iced::Alignment::Center);

        let col = column![
            header,
            container(form).padding(24).width(Fill).style(styles::card),
        ]
        .spacing(16);

        self.push_status(col, "Sending...").into()
    }
}
