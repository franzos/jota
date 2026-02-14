use crate::messages::Message;
use crate::state::Screen;
use crate::{styles, App, MUTED};
use iced::widget::{button, column, row, text, text_input, Space};
use iced::Element;
use zeroize::Zeroizing;

impl App {
    pub(crate) fn view_hardware_connect(&self) -> Element<Message> {
        let title = text("Connect Hardware Wallet").size(20);
        let back = button(text("Back").size(12))
            .padding([6, 14])
            .style(styles::btn_ghost)
            .on_press(Message::GoTo(Screen::WalletSelect));

        let name_input =
            text_input("Wallet name", &self.wallet_name).on_input(Message::WalletNameChanged);
        let pw_input = text_input("Password", &*self.password)
            .on_input(|s| Message::PasswordChanged(Zeroizing::new(s)))
            .secure(true);
        let pw_confirm = text_input("Confirm password", &*self.password_confirm)
            .on_input(|s| Message::PasswordConfirmChanged(Zeroizing::new(s)))
            .secure(true);

        let form_error = self.validate_create_form();
        let ready = self.loading == 0 && form_error.is_none();

        let mut connect_btn = button(text("Connect").size(14))
            .padding([10, 24])
            .style(styles::btn_primary);
        if ready {
            connect_btn = connect_btn.on_press(Message::HardwareConnect);
        }

        let mut col = column![
            row![back, title]
                .spacing(10)
                .align_y(iced::Alignment::Center),
            Space::new().height(8),
            text("Wallet name").size(12).color(MUTED),
            name_input,
            Space::new().height(4),
            text("Password").size(12).color(MUTED),
            pw_input,
            Space::new().height(4),
            text("Confirm password").size(12).color(MUTED),
            pw_confirm,
        ]
        .spacing(4)
        .max_width(400);

        if let Some(hint) = &form_error {
            col = col.push(Space::new().height(4));
            col = col.push(text(hint.clone()).size(12).color(styles::WARNING));
        }
        if let Some(warn) = self.password_warning() {
            col = col.push(text(warn).size(12).color(styles::WARNING));
        }

        col = col.push(Space::new().height(8));
        col = col.push(
            text("Make sure the wallet app is open on your device.")
                .size(12)
                .color(MUTED),
        );
        col = col.push(Space::new().height(8));
        col = col.push(connect_btn);

        if self.loading > 0 {
            col = col.push(text("Connecting to device...").size(13).color(MUTED));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(13).color(styles::DANGER));
        }

        col.into()
    }
}
