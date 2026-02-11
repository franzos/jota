use crate::messages::Message;
use crate::{App, BORDER, SURFACE};
use iced::widget::{button, column, container, text, Space};
use iced::{Element, Fill};

impl App {
    pub(crate) fn view_receive(&self) -> Element<Message> {
        let Some(info) = &self.wallet_info else {
            return text("No wallet loaded").into();
        };

        let title = text("Receive IOTA").size(24);

        let addr_container = container(
            text(&info.address_string).size(14),
        )
        .padding(15)
        .width(Fill)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                color: BORDER,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        });

        let copy = button(text("Copy Address").size(14)).on_press(Message::CopyAddress);

        let mut col = column![
            title,
            Space::new().height(10),
            text("Your address").size(12),
            addr_container,
            Space::new().height(10),
            copy,
        ]
        .spacing(5)
        .max_width(600);

        if let Some(msg) = &self.status_message {
            col = col.push(text(msg.as_str()).size(12).color([0.059, 0.757, 0.718]));
        }

        col.into()
    }
}
