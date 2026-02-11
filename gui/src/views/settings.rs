use crate::messages::Message;
use crate::{App, ACTIVE, MUTED};
use iced::widget::{button, column, row, text, text_input, Space};
use iced::Element;
use iota_wallet_core::wallet::Network;

impl App {
    pub(crate) fn view_settings(&self) -> Element<Message> {
        let title = text("Settings").size(24);

        let active_network = self
            .wallet_info
            .as_ref()
            .map(|i| &i.network_config.network)
            .unwrap_or(&self.network_config.network);

        let net_btn = |label: &'static str, network: Network| -> Element<Message> {
            let active = *active_network == network;
            let btn = button(text(label).size(14));
            let btn = if active {
                btn.style(|theme, status| {
                    let mut style = button::primary(theme, status);
                    style.background =
                        Some(iced::Background::Color(ACTIVE));
                    style
                })
            } else {
                btn.style(button::text)
            };
            btn.on_press(Message::NetworkChanged(network)).into()
        };

        let network_row = row![
            net_btn("Mainnet", Network::Mainnet),
            net_btn("Testnet", Network::Testnet),
            net_btn("Devnet", Network::Devnet),
        ]
        .spacing(8);

        let mut col = column![
            title,
            Space::new().height(15),
            text("Network").size(16),
            Space::new().height(5),
            network_row,
        ]
        .spacing(5)
        .max_width(500);

        if self.wallet_info.is_some() {
            col = col.push(
                text("Changing network applies to the current session only.")
                    .size(12)
                    .color(MUTED),
            );

            // -- Change password --
            col = col.push(Space::new().height(20));
            col = col.push(text("Change Password").size(16));
            col = col.push(Space::new().height(5));

            let old_pw = text_input("Current password", &self.settings_old_password)
                .on_input(Message::SettingsOldPasswordChanged)
                .secure(true);
            let new_pw = text_input("New password", &self.settings_new_password)
                .on_input(Message::SettingsNewPasswordChanged)
                .secure(true);
            let new_pw2 = text_input("Confirm new password", &self.settings_new_password_confirm)
                .on_input(Message::SettingsNewPasswordConfirmChanged)
                .on_submit(Message::ChangePassword)
                .secure(true);

            let can_submit = !self.loading
                && !self.settings_old_password.is_empty()
                && !self.settings_new_password.is_empty()
                && *self.settings_new_password == *self.settings_new_password_confirm;
            let mut change_btn = button(text("Change Password").size(14));
            if can_submit {
                change_btn = change_btn.on_press(Message::ChangePassword);
            }

            col = col.push(old_pw);
            col = col.push(new_pw);
            col = col.push(new_pw2);
            col = col.push(Space::new().height(5));
            col = col.push(change_btn);

            if self.loading {
                col = col.push(text("Changing password...").size(14));
            }
            if let Some(msg) = &self.success_message {
                col = col.push(text(msg.as_str()).size(14).color([0.059, 0.757, 0.718]));
            }
            if let Some(err) = &self.error_message {
                col = col.push(text(err.as_str()).size(14).color([0.906, 0.192, 0.192]));
            }
        }

        col.into()
    }
}
