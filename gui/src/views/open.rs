use crate::messages::Message;
use crate::state::Screen;
use crate::{styles, App, MUTED};
use iced::widget::{button, column, row, text, text_input, Space};
use iced::Element;
use zeroize::Zeroizing;

impl App {
    pub(crate) fn view_unlock(&self) -> Element<Message> {
        let name = self.selected_wallet.as_deref().unwrap_or("unknown");
        let title = text(format!("Unlock: {name}")).size(24);

        let pw = text_input("Password", &self.password)
            .on_input(|s| Message::PasswordChanged(Zeroizing::new(s)))
            .on_submit(Message::UnlockWallet)
            .secure(true);

        let mut unlock = button(text("Unlock").size(14))
            .padding([10, 20])
            .style(styles::btn_primary);
        if self.loading == 0 {
            unlock = unlock.on_press(Message::UnlockWallet);
        }

        let back = button(text("Back").size(14))
            .padding([10, 20])
            .style(styles::btn_secondary)
            .on_press(Message::GoTo(Screen::WalletSelect));

        let mut col = column![
            title,
            Space::new().height(8),
            pw,
            Space::new().height(8),
            row![back, unlock].spacing(10),
        ]
        .spacing(5)
        .max_width(400);

        if self.loading > 0 {
            col = col.push(text("Unlocking...").size(13).color(MUTED));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(13).color(styles::DANGER));
        }

        col.into()
    }
}
