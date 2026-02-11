use crate::messages::Message;
use crate::state::Screen;
use crate::App;
use iced::widget::{button, column, row, text, text_input, Space};
use iced::Element;

impl App {
    pub(crate) fn view_unlock(&self) -> Element<Message> {
        let name = self.selected_wallet.as_deref().unwrap_or("unknown");
        let title = text(format!("Unlock: {name}")).size(24);

        let pw = text_input("Password", &self.password)
            .on_input(Message::PasswordChanged)
            .on_submit(Message::UnlockWallet)
            .secure(true);

        let mut unlock = button(text("Unlock").size(14)).style(button::primary);
        if !self.loading {
            unlock = unlock.on_press(Message::UnlockWallet);
        }

        let back = button(text("Back").size(14)).on_press(Message::GoTo(Screen::WalletSelect));

        let mut col = column![
            title,
            Space::new().height(10),
            pw,
            Space::new().height(10),
            row![back, unlock].spacing(10),
        ]
        .spacing(5)
        .max_width(400);

        if self.loading {
            col = col.push(text("Unlocking...").size(14));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(14).color([0.906, 0.192, 0.192]));
        }

        col.into()
    }
}
