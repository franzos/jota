use base64ct::Encoding;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Element, Font, Length};

use crate::messages::Message;
use crate::styles;
use crate::{App, MUTED};

impl App {
    /// Returns an approval modal overlay when a native messaging request
    /// is pending. Follows the same overlay pattern as `view_validator_modal()`.
    pub(crate) fn view_approval_modal(&self) -> Option<Element<'_, Message>> {
        let approval = self.pending_approval.as_ref()?;
        let is_connect = approval.method == "connect";

        let mut detail = column![].spacing(8);

        // Title
        let title = if is_connect {
            "Connection Request"
        } else {
            "Signing Request"
        };
        detail = detail.push(text(title).size(18).font(styles::BOLD));

        detail = detail.push(Space::new().height(4));

        // Origin
        detail = detail.push(
            row![
                text("Origin")
                    .size(12)
                    .color(MUTED)
                    .width(Length::Fixed(100.0)),
                text(&approval.origin).size(13),
            ]
            .spacing(8),
        );

        if is_connect {
            // Simplified connect view
            detail = detail.push(
                row![
                    text("Action")
                        .size(12)
                        .color(MUTED)
                        .width(Length::Fixed(100.0)),
                    text("Wants to connect to your wallet").size(13),
                ]
                .spacing(8),
            );
        } else {
            // Method
            let method_label = match approval.method.as_str() {
                "signTransaction" => "Sign Transaction",
                "signAndExecuteTransaction" => "Sign & Execute Transaction",
                "signPersonalMessage" => "Sign Personal Message",
                other => other,
            };
            detail = detail.push(
                row![
                    text("Method")
                        .size(12)
                        .color(MUTED)
                        .width(Length::Fixed(100.0)),
                    text(method_label).size(13),
                ]
                .spacing(8),
            );

            // Summary
            if let Some(summary) = &approval.summary {
                detail = detail.push(
                    row![
                        text("Action")
                            .size(12)
                            .color(MUTED)
                            .width(Length::Fixed(100.0)),
                        text(summary.as_str()).size(13),
                    ]
                    .spacing(8),
                );
            }

            // Show transaction/message preview (truncated)
            if let Some(tx_b64) = approval.params.get("transaction").and_then(|v| v.as_str()) {
                let preview = if tx_b64.len() > 60 {
                    format!("{}...", &tx_b64[..60])
                } else {
                    tx_b64.to_string()
                };
                detail = detail.push(
                    row![
                        text("Data")
                            .size(12)
                            .color(MUTED)
                            .width(Length::Fixed(100.0)),
                        text(preview).size(11).font(Font::MONOSPACE),
                    ]
                    .spacing(8),
                );
            } else if let Some(msg_b64) = approval.params.get("message").and_then(|v| v.as_str()) {
                // Try to decode and show as text
                let preview = base64ct::Base64::decode_vec(msg_b64)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok())
                    .unwrap_or_else(|| {
                        if msg_b64.len() > 60 {
                            format!("{}...", &msg_b64[..60])
                        } else {
                            msg_b64.to_string()
                        }
                    });
                let preview = if preview.chars().count() > 120 {
                    let truncated: String = preview.chars().take(120).collect();
                    format!("{truncated}...")
                } else {
                    preview
                };
                detail = detail.push(
                    row![
                        text("Message")
                            .size(12)
                            .color(MUTED)
                            .width(Length::Fixed(100.0)),
                        text(preview).size(12),
                    ]
                    .spacing(8),
                );
            }

            // Chain
            if let Some(chain) = approval.params.get("chain").and_then(|v| v.as_str()) {
                detail = detail.push(
                    row![
                        text("Chain")
                            .size(12)
                            .color(MUTED)
                            .width(Length::Fixed(100.0)),
                        text(chain).size(13),
                    ]
                    .spacing(8),
                );
            }
        }

        detail = detail.push(Space::new().height(8));
        detail = detail.push(styles::separator());
        detail = detail.push(Space::new().height(4));

        // Approve / Reject buttons
        let approve_label = if is_connect { "Connect" } else { "Approve" };
        let approve_btn = button(text(approve_label).size(14))
            .padding([10, 24])
            .style(styles::btn_primary)
            .on_press(Message::ApproveNativeRequest);

        let reject_btn = button(text("Reject").size(14))
            .padding([10, 24])
            .style(styles::btn_ghost)
            .on_press(Message::RejectNativeRequest);

        detail = detail.push(
            row![reject_btn, Space::new().width(Length::Fill), approve_btn]
                .spacing(12)
                .align_y(iced::Alignment::Center),
        );

        let card = container(detail)
            .padding(24)
            .width(Length::Fixed(420.0))
            .style(styles::card);

        Some(card.into())
    }
}
