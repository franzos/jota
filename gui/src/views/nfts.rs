use crate::messages::Message;
use crate::{styles, App, MUTED};
use iced::widget::{button, column, container, row, text, text_input, Space};
use iced::{Element, Fill};

impl App {
    pub(crate) fn view_nfts(&self) -> Element<'_, Message> {
        let title = text("NFTs").size(24);

        let header = row![
            title,
            Space::new().width(Fill),
            button(text("Refresh").size(13))
                .padding([8, 16])
                .style(styles::btn_secondary)
                .on_press(Message::RefreshNfts),
        ]
        .align_y(iced::Alignment::Center);

        let mut col = column![header].spacing(16);

        if self.loading > 0 && self.nfts.is_empty() {
            col = col.push(text("Loading...").size(14).color(MUTED));
        } else if self.nfts.is_empty() {
            col = col.push(
                container(text("No NFTs found.").size(14).color(MUTED))
                    .padding(20)
                    .width(Fill)
                    .style(styles::card),
            );
        } else {
            for nft in &self.nfts {
                let id_str = nft.object_id.to_string();
                let id_short = if id_str.len() > 20 {
                    format!("{}...{}", &id_str[..10], &id_str[id_str.len() - 8..])
                } else {
                    id_str.clone()
                };

                let name = nft.name.as_deref().unwrap_or("(unnamed)");
                let type_short = nft
                    .object_type
                    .split("::")
                    .last()
                    .unwrap_or(&nft.object_type);

                let mut card_content = column![
                    text(name).size(16),
                    text(type_short).size(11).color(MUTED),
                    text(id_short).size(11).color(MUTED),
                ]
                .spacing(4);

                if let Some(desc) = &nft.description {
                    if !desc.is_empty() {
                        card_content = card_content.push(text(desc.as_str()).size(12).color(MUTED));
                    }
                }

                // Check if this NFT is selected for sending
                let is_selected = self.send_nft_object_id.as_deref() == Some(&id_str);

                if is_selected {
                    let recipient_input =
                        text_input("Recipient address or .iota name", &self.send_nft_recipient)
                            .on_input(Message::SendNftRecipientChanged)
                            .on_submit(Message::ConfirmSendNft)
                            .size(13);

                    let mut send_btn = button(text("Confirm").size(12))
                        .padding([6, 14])
                        .style(styles::btn_primary);
                    if self.loading == 0 && !self.send_nft_recipient.is_empty() {
                        send_btn = send_btn.on_press(Message::ConfirmSendNft);
                    }

                    let cancel_btn = button(text("Cancel").size(12))
                        .padding([6, 14])
                        .style(styles::btn_ghost)
                        .on_press(Message::CancelSendNft);

                    card_content = card_content
                        .push(Space::new().height(8))
                        .push(text("Send to").size(12).color(MUTED))
                        .push(recipient_input)
                        .push(row![send_btn, cancel_btn].spacing(8));
                } else {
                    let mut send_btn = button(text("Send").size(12))
                        .padding([6, 14])
                        .style(styles::btn_secondary);
                    if self.loading == 0 {
                        send_btn = send_btn.on_press(Message::SendNftSelected(id_str));
                    }
                    card_content = card_content.push(Space::new().height(4)).push(send_btn);
                }

                col = col.push(
                    container(card_content)
                        .padding(16)
                        .width(Fill)
                        .style(styles::card),
                );
            }
        }

        // Status messages
        if self.loading > 0 && !self.nfts.is_empty() {
            col = col.push(text("Processing...").size(13).color(MUTED));
        }
        if let Some(msg) = &self.success_message {
            col = col.push(text(msg.as_str()).size(13).color(styles::ACCENT));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(13).color(styles::DANGER));
        }

        col.into()
    }
}
