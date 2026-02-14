use crate::messages::Message;
use crate::state::SignMode;
use crate::{styles, App, MUTED};
use iced::widget::{button, column, container, row, text, text_input, Space};
use iced::{Element, Fill, Font};

impl App {
    pub(crate) fn view_sign(&self) -> Element<Message> {
        if self.wallet_info.is_none() {
            return text("No wallet loaded").into();
        }

        let title = text("Sign & Verify").size(24);

        // Mode toggle
        let sign_btn = button(text("Sign").size(13))
            .padding([8, 18])
            .style(styles::toggle_btn(self.sign_mode == SignMode::Sign))
            .on_press(Message::SignModeChanged(SignMode::Sign));
        let verify_btn = button(text("Verify").size(13))
            .padding([8, 18])
            .style(styles::toggle_btn(self.sign_mode == SignMode::Verify))
            .on_press(Message::SignModeChanged(SignMode::Verify));
        let notarize_btn = button(text("Notarize").size(13))
            .padding([8, 18])
            .style(styles::toggle_btn(self.sign_mode == SignMode::Notarize))
            .on_press(Message::SignModeChanged(SignMode::Notarize));
        let toggle_row = row![sign_btn, verify_btn, notarize_btn].spacing(4);

        let header = row![title, Space::new().width(Fill), toggle_row]
            .align_y(iced::Alignment::Center);

        let form: Element<Message> = match self.sign_mode {
            SignMode::Sign => self.view_sign_mode(),
            SignMode::Verify => self.view_verify_mode(),
            SignMode::Notarize => self.view_notarize_mode(),
        };

        let mut col = column![
            header,
            container(form).padding(24).width(Fill).style(styles::card),
        ]
        .spacing(16);

        if self.loading > 0 {
            col = col.push(text("Processing...").size(13).color(MUTED));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(13).color(styles::DANGER));
        }
        if let Some(msg) = &self.success_message {
            col = col.push(text(msg.as_str()).size(13).color(styles::ACCENT));
        }

        col.into()
    }

    fn view_sign_mode(&self) -> Element<Message> {
        let is_hardware = self.wallet_info.as_ref().map(|i| i.is_hardware).unwrap_or(false);

        let input = text_input("Message to sign", &self.sign_message_input)
            .on_input(Message::SignMessageInputChanged)
            .on_submit(Message::ConfirmSign);

        let msg_len = self.sign_message_input.len();
        let too_long = is_hardware && msg_len > 2048;

        let mut sign_btn = button(text("Sign Message").size(14))
            .padding([10, 24])
            .style(styles::btn_primary);
        if self.loading == 0 && !self.sign_message_input.is_empty() && !too_long {
            sign_btn = sign_btn.on_press(Message::ConfirmSign);
        }

        let mut form = column![
            text("Message").size(12).color(MUTED),
            input,
        ]
        .spacing(4);

        if is_hardware {
            let color = if too_long { styles::DANGER } else { MUTED };
            form = form.push(
                text(format!(
                    "{msg_len} / 2048 bytes — Nano S/X: 2 KB, other Ledger devices: 4 KB"
                ))
                .size(11)
                .color(color),
            );
        }

        form = form
            .push(Space::new().height(8))
            .push(sign_btn);

        // Show result card
        if let Some(signed) = &self.signed_result {
            let copy_sig = button(text("Copy").size(11))
                .padding([4, 10])
                .style(styles::btn_secondary)
                .on_press(Message::CopySignature);
            let copy_pk = button(text("Copy").size(11))
                .padding([4, 10])
                .style(styles::btn_secondary)
                .on_press(Message::CopyPublicKey);

            let sig_short = if signed.signature.len() > 24 {
                format!("{}...", &signed.signature[..24])
            } else {
                signed.signature.clone()
            };
            let pk_short = if signed.public_key.len() > 24 {
                format!("{}...", &signed.public_key[..24])
            } else {
                signed.public_key.clone()
            };

            let result_card = container(
                column![
                    text("Signed Result").size(14).font(styles::BOLD),
                    Space::new().height(4),
                    text(format!("Message: {}", signed.message)).size(12),
                    row![
                        text(format!("Signature: {sig_short}")).size(12).font(Font::MONOSPACE),
                        copy_sig,
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                    row![
                        text(format!("Public key: {pk_short}")).size(12).font(Font::MONOSPACE),
                        copy_pk,
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                    text(format!("Address: {}", signed.address)).size(12).font(Font::MONOSPACE),
                ]
                .spacing(4),
            )
            .padding(16)
            .width(Fill)
            .style(styles::card_flat);

            form = form
                .push(Space::new().height(12))
                .push(result_card);
        }

        form.into()
    }

    fn view_verify_mode(&self) -> Element<Message> {
        let msg_input = text_input("Message", &self.verify_message_input)
            .on_input(Message::VerifyMessageInputChanged);
        let sig_input = text_input("Signature (base64)", &self.verify_signature_input)
            .on_input(Message::VerifySignatureInputChanged);
        let pk_input = text_input("Public key (base64)", &self.verify_public_key_input)
            .on_input(Message::VerifyPublicKeyInputChanged)
            .on_submit(Message::ConfirmVerify);

        let ready = !self.verify_message_input.is_empty()
            && !self.verify_signature_input.is_empty()
            && !self.verify_public_key_input.is_empty();

        let mut verify_btn = button(text("Verify").size(14))
            .padding([10, 24])
            .style(styles::btn_primary);
        if self.loading == 0 && ready {
            verify_btn = verify_btn.on_press(Message::ConfirmVerify);
        }

        let mut form = column![
            text("Message").size(12).color(MUTED),
            msg_input,
            Space::new().height(4),
            text("Signature (base64)").size(12).color(MUTED),
            sig_input,
            Space::new().height(4),
            text("Public key (base64)").size(12).color(MUTED),
            pk_input,
            Space::new().height(8),
            verify_btn,
        ]
        .spacing(4);

        if let Some(valid) = self.verify_result {
            let (label, color) = if valid {
                ("VALID", styles::ACCENT)
            } else {
                ("INVALID", styles::DANGER)
            };
            let badge = container(
                text(label).size(14).font(styles::BOLD).color(color),
            )
            .padding([8, 16])
            .style(styles::card_flat);
            form = form.push(Space::new().height(8)).push(badge);
        }

        form.into()
    }

    fn view_notarize_mode(&self) -> Element<Message> {
        let has_package = self
            .wallet_info
            .as_ref()
            .and_then(|i| i.notarization_package)
            .is_some();

        let msg_input = text_input("Message to notarize", &self.sign_message_input)
            .on_input(Message::SignMessageInputChanged)
            .on_submit(Message::ConfirmNotarize);
        let desc_input = text_input("Description (optional)", &self.notarize_description)
            .on_input(Message::NotarizeDescriptionChanged);

        let mut notarize_btn = button(text("Notarize On-Chain").size(14))
            .padding([10, 24])
            .style(styles::btn_primary);
        if self.loading == 0 && !self.sign_message_input.is_empty() && has_package {
            notarize_btn = notarize_btn.on_press(Message::ConfirmNotarize);
        }

        let mut form = column![
            text("Message").size(12).color(MUTED),
            msg_input,
            Space::new().height(4),
            text("Description").size(12).color(MUTED),
            desc_input,
            Space::new().height(4),
            text("This will post a timestamped record on-chain. Gas fees apply.")
                .size(11)
                .color(MUTED),
            Space::new().height(8),
            notarize_btn,
        ]
        .spacing(4);

        if !has_package {
            form = form.push(Space::new().height(4));
            form = form.push(
                text("Set IOTA_NOTARIZATION_PKG_ID to enable notarization.")
                    .size(12)
                    .color(styles::DANGER),
            );
        }

        if let Some(digest) = &self.notarize_result {
            let explorer_btn = button(text("View in Explorer →").size(11))
                .padding([4, 10])
                .style(styles::btn_secondary)
                .on_press(Message::OpenExplorer(digest.clone()));
            let result_card = container(
                column![
                    text("Notarized").size(14).font(styles::BOLD),
                    Space::new().height(4),
                    text(format!("Digest: {digest}")).size(12).font(Font::MONOSPACE),
                    Space::new().height(4),
                    explorer_btn,
                ]
                .spacing(4),
            )
            .padding(16)
            .width(Fill)
            .style(styles::card_flat);
            form = form
                .push(Space::new().height(12))
                .push(result_card);
        }

        form.into()
    }
}
