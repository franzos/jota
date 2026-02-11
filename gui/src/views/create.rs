use crate::messages::Message;
use crate::state::Screen;
use crate::App;
use iced::widget::{button, column, row, text, text_input, Space};
use iced::Element;

impl App {
    pub(crate) fn view_create(&self) -> Element<Message> {
        // After creation — show mnemonic
        if let Some(mnemonic) = &self.created_mnemonic {
            return self.view_mnemonic_display(mnemonic);
        }

        let title = text("Create New Wallet").size(24);

        let name = text_input("Wallet name", &self.wallet_name)
            .on_input(Message::WalletNameChanged);
        let pw = text_input("Password", &self.password)
            .on_input(Message::PasswordChanged)
            .secure(true);
        let pw2 = text_input("Confirm password", &self.password_confirm)
            .on_input(Message::PasswordConfirmChanged)
            .on_submit(Message::CreateWallet)
            .secure(true);

        let mut create = button(text("Create").size(14)).style(button::primary);
        if !self.loading && self.validate_create_form().is_none() {
            create = create.on_press(Message::CreateWallet);
        }
        let back = button(text("Back").size(14)).on_press(Message::GoTo(Screen::WalletSelect));

        let mut col = column![
            title,
            Space::new().height(10),
            name,
            pw,
            pw2,
            Space::new().height(10),
            row![back, create].spacing(10),
        ]
        .spacing(5)
        .max_width(400);

        if self.loading {
            col = col.push(text("Creating wallet...").size(14));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(14).color([0.906, 0.192, 0.192]));
        }

        col.into()
    }

    fn view_mnemonic_display(&self, mnemonic: &str) -> Element<Message> {
        let title = text("Write Down Your Mnemonic").size(24);
        let warning = text(
            "Save these 24 words in a safe place. You will need them to recover your wallet.",
        )
        .size(14)
        .color([1.0, 0.757, 0.027]);

        let words: Vec<&str> = mnemonic.split_whitespace().collect();

        // Two-column layout: 1-12 left, 13-24 right
        let mut left = column![].spacing(4);
        let mut right = column![].spacing(4);
        for (i, word) in words.iter().enumerate() {
            let label = text(format!("{:>2}. {}", i + 1, word)).size(14);
            if i < 12 {
                left = left.push(label);
            } else {
                right = right.push(label);
            }
        }
        let word_grid = row![left, Space::new().width(30), right];

        let confirm = button(text("I've saved my mnemonic").size(14))
            .on_press(Message::MnemonicConfirmed);

        column![
            title,
            Space::new().height(10),
            warning,
            Space::new().height(10),
            word_grid,
            Space::new().height(20),
            confirm,
        ]
        .spacing(5)
        .max_width(500)
        .into()
    }
}
