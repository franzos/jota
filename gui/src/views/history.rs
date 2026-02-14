use crate::messages::Message;
use crate::{styles, App, MUTED};
use iced::widget::{button, column, container, row, text, Space};
use iced::{Element, Fill};

impl App {
    pub(crate) fn view_history(&self) -> Element<Message> {
        let title = text("Transaction History").size(24);
        let refresh = button(text("Refresh").size(13))
            .padding([8, 16])
            .style(styles::btn_secondary)
            .on_press(Message::RefreshHistory);

        let header =
            row![title, Space::new().width(Fill), refresh].align_y(iced::Alignment::Center);

        let mut col = column![header].spacing(16);

        if self.transactions.is_empty() {
            col = col.push(
                container(text("No transactions yet.").size(14).color(MUTED))
                    .padding(24)
                    .width(Fill)
                    .style(styles::card),
            );
        } else {
            let mut card_content =
                column![self.view_tx_table(&self.transactions, true),].spacing(12);

            // Pagination
            let page_num = self.history_page + 1;
            let total_pages = (self.history_total + 24) / 25;

            let mut nav = row![].spacing(8).align_y(iced::Alignment::Center);

            let mut prev = button(text("← Prev").size(12))
                .padding([6, 12])
                .style(styles::btn_secondary);
            if self.history_page > 0 {
                prev = prev.on_press(Message::HistoryPrevPage);
            }
            nav = nav.push(prev);

            nav = nav.push(
                text(format!("Page {page_num} of {total_pages}"))
                    .size(12)
                    .color(MUTED),
            );

            let mut next = button(text("Next →").size(12))
                .padding([6, 12])
                .style(styles::btn_secondary);
            if (self.history_page + 1) * 25 < self.history_total {
                next = next.on_press(Message::HistoryNextPage);
            }
            nav = nav.push(next);

            card_content = card_content.push(styles::separator());
            card_content = card_content.push(nav);

            col = col.push(
                container(card_content)
                    .padding(20)
                    .width(Fill)
                    .style(styles::card),
            );
        }

        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(13).color(styles::DANGER));
        }

        col.into()
    }
}
