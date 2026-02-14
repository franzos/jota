use crate::messages::Message;
use crate::state::Screen;
use crate::{styles, App, MUTED};
use iced::widget::{button, column, row, text, text_input, Space};
use iced::Element;
use zeroize::Zeroizing;

impl App {
    pub(crate) fn view_recover(&self) -> Element<Message> {
        let title = text("Recover Wallet").size(24);

        let name =
            text_input("Wallet name", &self.wallet_name).on_input(Message::WalletNameChanged);
        let mnemonic = text_input("24-word mnemonic phrase", &self.mnemonic_input)
            .on_input(|s| Message::MnemonicInputChanged(Zeroizing::new(s)))
            .secure(true);
        let pw = text_input("Password", &self.password)
            .on_input(|s| Message::PasswordChanged(Zeroizing::new(s)))
            .secure(true);
        let pw2 = text_input("Confirm password", &self.password_confirm)
            .on_input(|s| Message::PasswordConfirmChanged(Zeroizing::new(s)))
            .on_submit(Message::RecoverWallet)
            .secure(true);

        let form_error = self.validate_create_form();
        let can_submit =
            self.loading == 0 && form_error.is_none() && !self.mnemonic_input.trim().is_empty();
        let mut recover = button(text("Recover").size(14))
            .padding([10, 20])
            .style(styles::btn_primary);
        if can_submit {
            recover = recover.on_press(Message::RecoverWallet);
        }
        let back = button(text("Back").size(14))
            .padding([10, 20])
            .style(styles::btn_secondary)
            .on_press(Message::GoTo(Screen::WalletSelect));

        let mut col = column![title, Space::new().height(8), name, mnemonic, pw, pw2,]
            .spacing(5)
            .max_width(400);

        if let Some(hint) = &form_error {
            col = col.push(Space::new().height(4));
            col = col.push(text(hint.clone()).size(12).color(styles::WARNING));
        }
        if let Some(warn) = self.password_warning() {
            col = col.push(text(warn).size(12).color(styles::WARNING));
        }

        col = col.push(Space::new().height(8));
        col = col.push(row![back, recover].spacing(10));

        if self.loading > 0 {
            col = col.push(text("Recovering wallet...").size(13).color(MUTED));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(13).color(styles::DANGER));
        }

        col.into()
    }
}
