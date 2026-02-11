use crate::messages::Message;
use crate::state::Screen;
use crate::{App, BORDER, MUTED};
use iced::widget::{button, column, container, row, text, Space};
use iced::{Color, Element, Fill, Length, Padding};
use iota_wallet_core::display::{format_balance, nanos_to_iota};
use iota_wallet_core::network::{TransactionDirection, TransactionSummary};

impl App {
    pub(crate) fn view_tx_table<'a>(&'a self, txs: &'a [TransactionSummary], expandable: bool) -> Element<'a, Message> {
        let header = row![
            text("Dir").size(11).width(Length::Fixed(35.0)),
            text("Sender").size(11).width(Length::Fixed(140.0)),
            text("Received").size(11).width(Length::Fixed(110.0)),
            text("Sent").size(11).width(Length::Fixed(110.0)),
            text("Digest").size(11),
        ]
        .spacing(8);

        let separator = container(Space::new().height(1))
            .width(Fill)
            .style(|_theme| container::Style {
                border: iced::Border {
                    color: BORDER,
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            });

        let mut tx_col = column![header, separator].spacing(2);

        for (i, tx) in txs.iter().enumerate() {
            let dir_label = match tx.direction {
                Some(TransactionDirection::In) => "in",
                Some(TransactionDirection::Out) => "out",
                None => "",
            };
            let dir_color = match tx.direction {
                Some(TransactionDirection::In) => Color::from_rgb(0.059, 0.757, 0.718),
                Some(TransactionDirection::Out) => Color::from_rgb(0.906, 0.192, 0.192),
                None => MUTED,
            };

            let sender_short = tx
                .sender
                .as_ref()
                .map(|s| {
                    if s.len() > 16 {
                        format!("{}...{}", &s[..8], &s[s.len() - 6..])
                    } else {
                        s.clone()
                    }
                })
                .unwrap_or_else(|| "-".into());

            let (received, sent) = match tx.direction {
                Some(TransactionDirection::In) => (
                    tx.amount
                        .map(|a| nanos_to_iota(a))
                        .unwrap_or_else(|| "-".into()),
                    "-".into(),
                ),
                Some(TransactionDirection::Out) => (
                    "-".into(),
                    tx.amount
                        .map(|a| nanos_to_iota(a))
                        .unwrap_or_else(|| "-".into()),
                ),
                None => ("-".into(), "-".into()),
            };

            let digest_short = if tx.digest.len() > 16 {
                format!("{}...{}", &tx.digest[..8], &tx.digest[tx.digest.len() - 6..])
            } else {
                tx.digest.clone()
            };

            let tx_row = button(
                row![
                    text(dir_label).size(12).color(dir_color).width(Length::Fixed(35.0)),
                    text(sender_short).size(12).width(Length::Fixed(140.0)),
                    text(received).size(12).width(Length::Fixed(110.0)),
                    text(sent).size(12).width(Length::Fixed(110.0)),
                    text(digest_short).size(12),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            )
            .width(Fill)
            .style(|theme, status| {
                let mut style = button::text(theme, status);
                style.background = None;
                style
            })
            .on_press(if expandable {
                Message::ToggleTxDetail(i)
            } else {
                Message::GoTo(Screen::History)
            });

            tx_col = tx_col.push(tx_row);

            // Expanded detail panel
            if expandable && self.expanded_tx == Some(i) {
                let detail_padding = Padding {
                    top: 4.0,
                    right: 0.0,
                    bottom: 8.0,
                    left: 40.0,
                };
                let mut detail = column![].spacing(3).padding(detail_padding);

                if let Some(ref sender) = tx.sender {
                    detail = detail.push(
                        row![
                            text("Sender:").size(11).width(Length::Fixed(60.0)),
                            text(sender.as_str()).size(11),
                        ]
                        .spacing(8),
                    );
                }

                if let Some(amount) = tx.amount {
                    detail = detail.push(
                        row![
                            text("Amount:").size(11).width(Length::Fixed(60.0)),
                            text(format_balance(amount)).size(11),
                        ]
                        .spacing(8),
                    );
                }

                if let Some(fee) = tx.fee {
                    detail = detail.push(
                        row![
                            text("Fee:").size(11).width(Length::Fixed(60.0)),
                            text(format_balance(fee)).size(11),
                        ]
                        .spacing(8),
                    );
                }

                detail = detail.push(
                    row![
                        text("Digest:").size(11).width(Length::Fixed(60.0)),
                        text(&tx.digest).size(11),
                    ]
                    .spacing(8),
                );

                detail = detail.push(
                    row![
                        text("Epoch:").size(11).width(Length::Fixed(60.0)),
                        text(format!("{}", tx.epoch)).size(11),
                    ]
                    .spacing(8),
                );

                let explorer = button(text("View in Explorer").size(11))
                    .on_press(Message::OpenExplorer(tx.digest.clone()));
                detail = detail.push(explorer);

                let detail_container = container(detail)
                    .width(Fill)
                    .style(|_theme| container::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(
                            0.114, 0.157, 0.227,
                        ))),
                        border: iced::Border {
                            color: BORDER,
                            width: 1.0,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    });
                tx_col = tx_col.push(detail_container);
            }
        }

        tx_col.into()
    }
}
