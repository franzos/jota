use crate::messages::Message;
use crate::{styles, App, BG, BORDER, MUTED};
use iced::widget::{button, column, container, row, text, qr_code, Space};
use iced::{Element, Fill, Font};

impl App {
    pub(crate) fn view_receive(&self) -> Element<Message> {
        let Some(info) = &self.wallet_info else {
            return text("No wallet loaded").into();
        };

        let title = text("Receive IOTA").size(24);

        let addr_container = container(
            text(&info.address_string).size(14).font(Font::MONOSPACE),
        )
        .padding(15)
        .width(Fill)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(BG)),
            border: iced::Border {
                color: BORDER,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });

        let copy = button(text("Copy Address").size(14))
            .padding([10, 20])
            .style(styles::btn_primary)
            .on_press(Message::CopyAddress);

        let mut card_content = column![text("Your Address").size(12).color(MUTED),]
            .spacing(8);

        if let Some(data) = &self.qr_data {
            card_content = card_content
                .push(container(qr_code(data).cell_size(6)).center_x(Fill));
        }

        card_content = card_content
            .push(addr_container)
            .push(Space::new().height(8))
            .push(copy);

        let header = row![title, Space::new().width(Fill)]
            .align_y(iced::Alignment::Center);

        let mut col = column![
            header,
            container(card_content)
                .padding(24)
                .width(Fill)
                .style(styles::card),
        ]
        .spacing(16);

        if let Some(msg) = &self.status_message {
            col = col.push(text(msg.as_str()).size(13).color(styles::ACCENT));
        }

        col.into()
    }
}
