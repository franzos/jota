use crate::messages::Message;
use crate::state::Screen;
use crate::App;
use iced::widget::{button, column, row, text, text_input, Space};
use iced::Element;

impl App {
    pub(crate) fn view_recover(&self) -> Element<Message> {
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
            col = col.push(text(err.as_str()).size(14).color([0.906, 0.192, 0.192]));
        }

        col.into()
    }
}
