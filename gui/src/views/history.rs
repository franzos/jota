use crate::messages::Message;
use crate::App;
use iced::widget::{button, column, row, scrollable, text, Space};
use iced::Element;

impl App {
    pub(crate) fn view_history(&self) -> Element<Message> {
        let title = text("Transaction History").size(24);
        let refresh = button(text("Refresh").size(14)).on_press(Message::RefreshHistory);

        let mut col = column![
            row![title, Space::new().width(10), refresh].align_y(iced::Alignment::Center),
            Space::new().height(10),
        ].spacing(5);

        if self.transactions.is_empty() {
            col = col.push(text("No transactions yet.").size(14));
        } else {
            col = col.push(self.view_tx_table(&self.transactions, true));

            // Pagination controls
            let page_num = self.history_page + 1;
            let total_pages = (self.history_total + 24) / 25; // ceil div

            let mut nav = row![].spacing(10).align_y(iced::Alignment::Center);

            let mut prev = button(text("Prev").size(12));
            if self.history_page > 0 {
                prev = prev.on_press(Message::HistoryPrevPage);
            }
            nav = nav.push(prev);

            nav = nav.push(text(format!("Page {page_num} of {total_pages}")).size(12));

            let mut next = button(text("Next").size(12));
            if (self.history_page + 1) * 25 < self.history_total {
                next = next.on_press(Message::HistoryNextPage);
            }
            nav = nav.push(next);

            col = col.push(Space::new().height(10));
            col = col.push(nav);
        }

        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(14).color([0.906, 0.192, 0.192]));
        }

        scrollable(col).into()
    }
}
