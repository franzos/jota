use crate::messages::Message;
use crate::state::Screen;
use crate::{styles, App, MUTED};
use iced::widget::{button, column, row, text, text_input, Space};
use iced::Element;
use zeroize::Zeroizing;

impl App {
    pub(crate) fn view_create(&self) -> Element<'_, Message> {
        // After creation — show mnemonic
        if let Some(mnemonic) = &self.created_mnemonic {
            return self.view_mnemonic_display(mnemonic);
        }

        let title = text("Create New Wallet").size(24);

        let name =
            text_input("Wallet name", &self.wallet_name).on_input(Message::WalletNameChanged);
        let pw = text_input("Password", &self.password)
            .on_input(|s| Message::PasswordChanged(Zeroizing::new(s)))
            .secure(true);
        let pw2 = text_input("Confirm password", &self.password_confirm)
            .on_input(|s| Message::PasswordConfirmChanged(Zeroizing::new(s)))
            .on_submit(Message::CreateWallet)
            .secure(true);

        let form_error = self.validate_create_form();
        let mut create = button(text("Create").size(14))
            .padding([10, 20])
            .style(styles::btn_primary);
        if self.loading == 0 && form_error.is_none() {
            create = create.on_press(Message::CreateWallet);
        }
        let back = button(text("Back").size(14))
            .padding([10, 20])
            .style(styles::btn_secondary)
            .on_press(Message::GoTo(Screen::WalletSelect));

        let mut col = column![title, Space::new().height(8), name, pw, pw2,]
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
        col = col.push(row![back, create].spacing(10));

        if self.loading > 0 {
            col = col.push(text("Creating wallet...").size(13).color(MUTED));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(13).color(styles::DANGER));
        }

        col.into()
    }

    fn view_mnemonic_display(&self, mnemonic: &str) -> Element<'_, Message> {
        let title = text("Write Down Your Mnemonic").size(24);
        let warning =
            text("Save these 24 words in a safe place. You will need them to recover your wallet.")
                .size(14)
                .color(styles::WARNING);

        let words: Vec<String> = mnemonic.split_whitespace().map(String::from).collect();

        let mut left = column![].spacing(4);
        let mut right = column![].spacing(4);
        for (i, word) in words.iter().enumerate() {
            let num = text(format!("{:>2}.", i + 1)).size(14).color(MUTED);
            let w = text(word.clone()).size(14).font(styles::BOLD);
            let label = row![num, w].spacing(6);
            if i < 12 {
                left = left.push(label);
            } else {
                right = right.push(label);
            }
        }
        let word_grid = row![left, Space::new().width(40), right];

        let confirm = button(text("I've saved my mnemonic").size(14))
            .padding([10, 20])
            .style(styles::btn_primary)
            .on_press(Message::MnemonicConfirmed);

        column![
            title,
            Space::new().height(8),
            warning,
            Space::new().height(12),
            word_grid,
            Space::new().height(20),
            confirm,
        ]
        .spacing(4)
        .max_width(500)
        .into()
    }
}
