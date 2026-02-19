use crate::messages::Message;
use crate::state::Screen;
use crate::{styles, App, MUTED};
use iced::widget::{button, column, container, row, table, text, Column, Space};
use iced::{Element, Fill, Font, Length};
use jota_core::display::{format_balance, nanos_to_iota};
use jota_core::network::{TransactionDirection, TransactionSummary};

/// Pre-formatted row data passed to the table column view closures.
#[derive(Clone)]
struct TxRow {
    index: usize,
    dir_label: &'static str,
    dir_color: iced::Color,
    sender_short: String,
    received: String,
    sent: String,
    digest_short: String,
    expandable: bool,
}

impl App {
    pub(crate) fn view_tx_table<'a>(
        &'a self,
        txs: &'a [TransactionSummary],
        expandable: bool,
    ) -> Element<'a, Message> {
        let rows: Vec<TxRow> = txs
            .iter()
            .enumerate()
            .map(|(i, tx)| {
                let dir_label = match tx.direction {
                    Some(TransactionDirection::In) => "↙ in",
                    Some(TransactionDirection::Out) => "↗ out",
                    None => "  —",
                };
                let dir_color = match tx.direction {
                    Some(TransactionDirection::In) => styles::ACCENT,
                    Some(TransactionDirection::Out) => styles::DANGER,
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
                        tx.amount.map(nanos_to_iota).unwrap_or_else(|| "-".into()),
                        "-".into(),
                    ),
                    Some(TransactionDirection::Out) => (
                        "-".into(),
                        tx.amount.map(nanos_to_iota).unwrap_or_else(|| "-".into()),
                    ),
                    None => ("-".into(), "-".into()),
                };

                let digest_short = if tx.digest.len() > 16 {
                    format!(
                        "{}...{}",
                        &tx.digest[..8],
                        &tx.digest[tx.digest.len() - 6..]
                    )
                } else {
                    tx.digest.clone()
                };

                TxRow {
                    index: i,
                    dir_label,
                    dir_color,
                    sender_short,
                    received,
                    sent,
                    digest_short,
                    expandable,
                }
            })
            .collect();

        let tbl = table::table(
            [
                table::column(
                    text("Dir").size(12).color(MUTED),
                    |r: TxRow| -> Element<'_, Message> {
                        text(r.dir_label).size(13).color(r.dir_color).into()
                    },
                )
                .width(Length::FillPortion(2)),
                table::column(
                    text("Sender").size(12).color(MUTED),
                    |r: TxRow| -> Element<'_, Message> { text(r.sender_short).size(13).into() },
                )
                .width(Length::FillPortion(6)),
                table::column(
                    text("Received").size(12).color(MUTED),
                    |r: TxRow| -> Element<'_, Message> { text(r.received).size(13).into() },
                )
                .width(Length::FillPortion(5)),
                table::column(
                    text("Sent").size(12).color(MUTED),
                    |r: TxRow| -> Element<'_, Message> { text(r.sent).size(13).into() },
                )
                .width(Length::FillPortion(5)),
                table::column(
                    text("Digest").size(12).color(MUTED),
                    move |r: TxRow| -> Element<'_, Message> {
                        let msg = if r.expandable {
                            Message::ToggleTxDetail(r.index)
                        } else {
                            Message::GoTo(Screen::History)
                        };
                        button(text(r.digest_short).size(13).font(Font::MONOSPACE))
                            .padding([4, 8])
                            .style(styles::btn_ghost)
                            .on_press(msg)
                            .into()
                    },
                )
                .width(Length::FillPortion(7)),
            ],
            rows,
        )
        .width(Fill)
        .padding_x(12)
        .padding_y(8)
        .separator_x(0)
        .separator_y(1);

        tbl.into()
    }

    /// Build the detail overlay content for an expanded transaction.
    pub(crate) fn view_tx_detail_overlay(&self) -> Option<Element<'_, Message>> {
        let idx = self.expanded_tx?;
        let tx = self.transactions.get(idx)?;

        let mut detail = column![].spacing(8);

        // Direction
        let (dir_label, dir_color) = match tx.direction {
            Some(TransactionDirection::In) => ("↙ Incoming", styles::ACCENT),
            Some(TransactionDirection::Out) => ("↗ Outgoing", styles::DANGER),
            None => ("— Unknown", MUTED),
        };
        detail = detail.push(text(dir_label).size(16).font(styles::BOLD).color(dir_color));

        if let Some(ref sender) = tx.sender {
            let sender_display = if sender.len() > 30 {
                format!("{}...{}", &sender[..14], &sender[sender.len() - 14..])
            } else {
                sender.clone()
            };
            detail = detail.push(
                row![
                    text("Sender:")
                        .size(12)
                        .color(MUTED)
                        .width(Length::Fixed(70.0)),
                    text(sender_display).size(12).font(Font::MONOSPACE),
                ]
                .spacing(8),
            );
        }

        if let Some(amount) = tx.amount {
            detail = detail.push(
                row![
                    text("Amount:")
                        .size(12)
                        .color(MUTED)
                        .width(Length::Fixed(70.0)),
                    text(format_balance(amount)).size(13).font(styles::BOLD),
                ]
                .spacing(8),
            );
        }

        if let Some(fee) = tx.fee {
            detail = detail.push(
                row![
                    text("Fee:")
                        .size(12)
                        .color(MUTED)
                        .width(Length::Fixed(70.0)),
                    text(format_balance(fee)).size(12),
                ]
                .spacing(8),
            );
        }

        let digest_display = if tx.digest.len() > 30 {
            format!(
                "{}...{}",
                &tx.digest[..14],
                &tx.digest[tx.digest.len() - 14..]
            )
        } else {
            tx.digest.clone()
        };
        detail = detail.push(
            row![
                text("Digest:")
                    .size(12)
                    .color(MUTED)
                    .width(Length::Fixed(70.0)),
                text(digest_display).size(12).font(Font::MONOSPACE),
            ]
            .spacing(8),
        );

        detail = detail.push(
            row![
                text("Epoch:")
                    .size(12)
                    .color(MUTED)
                    .width(Length::Fixed(70.0)),
                text(format!("{}", tx.epoch)).size(12),
            ]
            .spacing(8),
        );

        detail = detail.push(Space::new().height(4));

        let mut actions = row![].spacing(8);
        actions = actions.push(
            button(text("View in Explorer →").size(12))
                .padding([6, 12])
                .style(styles::btn_secondary)
                .on_press(Message::OpenExplorer(tx.digest.clone())),
        );
        actions = actions.push(
            button(text("Close").size(12))
                .padding([6, 12])
                .style(styles::btn_ghost)
                .on_press(Message::ToggleTxDetail(idx)),
        );
        detail = detail.push(actions);

        let card = container(detail)
            .padding(24)
            .max_width(520)
            .style(styles::card);

        Some(card.into())
    }

    /// Append loading, error, and success status messages to a column.
    pub(crate) fn push_status<'a>(
        &'a self,
        col: Column<'a, Message>,
        loading_text: &'a str,
    ) -> Column<'a, Message> {
        let mut col = col;
        if self.loading > 0 {
            col = col.push(text(loading_text).size(13).color(MUTED));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(13).color(styles::DANGER));
        }
        if let Some(msg) = &self.success_message {
            col = col.push(text(msg.as_str()).size(13).color(styles::ACCENT));
        }
        col
    }

    /// Build the file path for the currently selected wallet.
    pub(crate) fn wallet_path(&self) -> std::path::PathBuf {
        let name = self.selected_wallet.as_deref().unwrap_or("");
        self.wallet_dir.join(format!("{name}.wallet"))
    }
}
