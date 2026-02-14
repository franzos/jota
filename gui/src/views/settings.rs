use crate::messages::Message;
use crate::{styles, App, MUTED};
use iced::widget::{button, column, container, row, text, text_input, Space};
use iced::{Element, Fill};
use iota_wallet_core::wallet::Network;
use zeroize::Zeroizing;

impl App {
    pub(crate) fn view_settings(&self) -> Element<'_, Message> {
        let title = text("Settings").size(24);

        let active_network = self
            .wallet_info
            .as_ref()
            .map(|i| &i.network_config.network)
            .unwrap_or(&self.network_config.network);

        let net_btn = |label: &'static str, network: Network| -> Element<Message> {
            let active = *active_network == network;
            button(text(label).size(13))
                .padding([8, 16])
                .style(styles::toggle_btn(active))
                .on_press(Message::NetworkChanged(network))
                .into()
        };

        let network_row = row![
            net_btn("Mainnet", Network::Mainnet),
            net_btn("Testnet", Network::Testnet),
            net_btn("Devnet", Network::Devnet),
        ]
        .spacing(8);

        let mut network_content = column![
            text("Network").size(16),
            Space::new().height(4),
            network_row,
        ]
        .spacing(4);

        if self.wallet_info.is_some() {
            network_content = network_content.push(
                text("Changing network applies to the current session only.")
                    .size(12)
                    .color(MUTED),
            );
        }

        let header = row![title, Space::new().width(Fill)].align_y(iced::Alignment::Center);

        let mut col = column![
            header,
            container(network_content)
                .padding(24)
                .max_width(500)
                .style(styles::card),
        ]
        .spacing(16);

        if self.wallet_info.is_some() {
            // Change password card
            let old_pw = text_input("Current password", &self.settings_old_password)
                .on_input(|s| Message::SettingsOldPasswordChanged(Zeroizing::new(s)))
                .secure(true);
            let new_pw = text_input("New password", &self.settings_new_password)
                .on_input(|s| Message::SettingsNewPasswordChanged(Zeroizing::new(s)))
                .secure(true);
            let new_pw2 = text_input("Confirm new password", &self.settings_new_password_confirm)
                .on_input(|s| Message::SettingsNewPasswordConfirmChanged(Zeroizing::new(s)))
                .on_submit(Message::ChangePassword)
                .secure(true);

            let can_submit = self.loading == 0
                && !self.settings_old_password.is_empty()
                && !self.settings_new_password.is_empty()
                && *self.settings_new_password == *self.settings_new_password_confirm;
            let mut change_btn = button(text("Change Password").size(14))
                .padding([10, 24])
                .style(styles::btn_primary);
            if can_submit {
                change_btn = change_btn.on_press(Message::ChangePassword);
            }

            let pw_content = column![
                text("Change Password").size(16),
                Space::new().height(4),
                old_pw,
                new_pw,
                new_pw2,
                Space::new().height(8),
                change_btn,
            ]
            .spacing(4);

            col = col.push(
                container(pw_content)
                    .padding(24)
                    .max_width(500)
                    .style(styles::card),
            );

            if self.loading > 0 {
                col = col.push(text("Changing password...").size(13).color(MUTED));
            }
            if let Some(msg) = &self.success_message {
                col = col.push(text(msg.as_str()).size(13).color(styles::ACCENT));
            }
            if let Some(err) = &self.error_message {
                col = col.push(text(err.as_str()).size(13).color(styles::DANGER));
            }
        }

        col.into()
    }
}
