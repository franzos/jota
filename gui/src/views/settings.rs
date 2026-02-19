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

        // -- History lookback --
        let lookback_btn = |label: &'static str, epochs: u64| -> Element<Message> {
            let active = self.history_lookback == epochs;
            button(text(label).size(13))
                .padding([8, 16])
                .style(styles::toggle_btn(active))
                .on_press(Message::HistoryLookbackChanged(epochs))
                .into()
        };

        let lookback_row = row![
            lookback_btn("1 week", 7),
            lookback_btn("1 month", 30),
            lookback_btn("3 months", 90),
            lookback_btn("1 year", 365),
        ]
        .spacing(8);

        let history_content = column![
            text("History").size(16),
            Space::new().height(4),
            lookback_row,
            text("How far back to sync transaction history.")
                .size(12)
                .color(MUTED),
        ]
        .spacing(4);

        let header = row![title, Space::new().width(Fill)].align_y(iced::Alignment::Center);

        let mut col = column![
            header,
            container(network_content)
                .padding(24)
                .max_width(500)
                .style(styles::card),
            container(history_content)
                .padding(24)
                .max_width(500)
                .style(styles::card),
        ]
        .spacing(16);

        // Browser Extension bridge card
        {
            let ext_input = text_input("Chrome Extension ID", &self.extension_id)
                .on_input(Message::ExtensionIdChanged)
                .size(13);

            let can_install = !self.extension_id.trim().is_empty();
            let mut install_btn = button(text("Install Native Host").size(14))
                .padding([10, 24])
                .style(styles::btn_primary);
            if can_install {
                install_btn = install_btn.on_press(Message::InstallNativeHost);
            }

            let mut bridge_content =
                column![
                text("Browser Extension").size(16),
                Space::new().height(4),
                text("Connect dApps to this wallet via the browser extension.")
                    .size(12)
                    .color(MUTED),
                text("Click the extension icon in your browser to copy its ID, then paste it here.")
                    .size(12)
                    .color(MUTED),
                Space::new().height(4),
                ext_input,
                Space::new().height(8),
                install_btn,
            ]
                .spacing(4);

            if let Some(msg) = &self.success_message {
                if msg.contains("Native host") {
                    bridge_content =
                        bridge_content.push(text(msg.as_str()).size(12).color(styles::ACCENT));
                }
            }

            col = col.push(
                container(bridge_content)
                    .padding(24)
                    .max_width(500)
                    .style(styles::card),
            );
        }

        // Connected Sites card
        if let Some(info) = &self.wallet_info {
            let sites = self.permissions.connected_sites(&info.address_string);
            let mut sites_content =
                column![text("Connected Sites").size(16), Space::new().height(4),].spacing(4);

            if sites.is_empty() {
                sites_content =
                    sites_content.push(text("No connected sites").size(12).color(MUTED));
            } else {
                for origin in sites {
                    let revoke_btn = button(text("Revoke").size(11))
                        .padding([4, 12])
                        .style(styles::btn_ghost)
                        .on_press(Message::RevokeSitePermission(origin.clone()));
                    sites_content = sites_content.push(
                        row![text(origin).size(13), Space::new().width(Fill), revoke_btn,]
                            .spacing(8)
                            .align_y(iced::Alignment::Center),
                    );
                }
            }

            col = col.push(
                container(sites_content)
                    .padding(24)
                    .max_width(500)
                    .style(styles::card),
            );
        }

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
