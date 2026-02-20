use crate::messages::Message;
use crate::{styles, App, MUTED};
use iced::widget::{button, column, container, row, table, text, text_input, Space};
use iced::{Element, Fill, Font, Length};

/// Pre-formatted row data for the contacts table.
#[derive(Clone)]
struct ContactRow {
    index: usize,
    name_display: String,
    address_short: String,
}

impl App {
    pub(crate) fn view_contacts(&self) -> Element<'_, Message> {
        let title = text("Contacts").size(24);

        let add_btn = button(text("+ Add Contact").size(13))
            .padding([8, 16])
            .style(styles::btn_primary)
            .on_press(Message::OpenContactForm);

        let header =
            row![title, Space::new().width(Fill), add_btn].align_y(iced::Alignment::Center);

        let mut col = column![header].spacing(16);

        if self.contacts.is_empty() {
            col = col.push(
                container(text("No contacts yet.").size(14).color(MUTED))
                    .padding(24)
                    .width(Fill)
                    .style(styles::card),
            );
        } else {
            let rows: Vec<ContactRow> = self
                .contacts
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let name_display = if let Some(ref iota_name) = c.iota_name {
                        format!("{} ({})", c.name, iota_name)
                    } else {
                        c.name.clone()
                    };

                    let address_short = if c.address.len() > 20 {
                        format!(
                            "{}...{}",
                            &c.address[..10],
                            &c.address[c.address.len() - 8..]
                        )
                    } else {
                        c.address.clone()
                    };

                    ContactRow {
                        index: i,
                        name_display,
                        address_short,
                    }
                })
                .collect();

            let tbl = table::table(
                [
                    table::column(
                        text("Name").size(12).color(MUTED),
                        |r: ContactRow| -> Element<'_, Message> {
                            text(r.name_display).size(13).into()
                        },
                    )
                    .width(Length::FillPortion(4)),
                    table::column(
                        text("Address").size(12).color(MUTED),
                        |r: ContactRow| -> Element<'_, Message> {
                            text(r.address_short)
                                .size(12)
                                .font(Font::MONOSPACE)
                                .color(MUTED)
                                .into()
                        },
                    )
                    .width(Length::FillPortion(6)),
                    table::column(text("").size(12), |r: ContactRow| -> Element<'_, Message> {
                        let edit_btn = button(text("Edit").size(11))
                            .padding([4, 8])
                            .style(styles::btn_secondary)
                            .on_press(Message::EditContact(r.index));
                        let del_btn = button(text("Delete").size(11))
                            .padding([4, 8])
                            .style(styles::btn_danger)
                            .on_press(Message::DeleteContact(r.index));
                        row![edit_btn, del_btn].spacing(4).into()
                    })
                    .width(Length::FillPortion(3)),
                ],
                rows,
            )
            .width(Fill)
            .padding_x(12)
            .padding_y(8)
            .separator_x(0)
            .separator_y(1);

            col = col.push(container(tbl).width(Fill).style(styles::card));
        }

        self.push_status(col, "Loading contacts...").into()
    }

    pub(crate) fn view_contact_modal(&self) -> Option<Element<'_, Message>> {
        if !self.contact_form_visible {
            return None;
        }

        let title = if self.contact_form_editing.is_some() {
            "Edit Contact"
        } else {
            "Add Contact"
        };

        let name_input =
            text_input("Name", &self.contact_form_name).on_input(Message::ContactNameChanged);

        let address_input = text_input("Address (0x...) or .iota name", &self.contact_form_address)
            .on_input(Message::ContactAddressChanged)
            .on_submit(Message::SaveContact);

        let mut save_btn = button(text("Save").size(14))
            .padding([10, 24])
            .style(styles::btn_primary);
        if !self.contact_form_name.is_empty() && !self.contact_form_address.is_empty() {
            save_btn = save_btn.on_press(Message::SaveContact);
        }

        let cancel_btn = button(text("Cancel").size(14))
            .padding([10, 24])
            .style(styles::btn_ghost)
            .on_press(Message::CloseContactForm);

        let detail = column![
            text(title).size(18).font(styles::BOLD),
            Space::new().height(8),
            text("Name").size(12).color(MUTED),
            name_input,
            Space::new().height(4),
            text("Address").size(12).color(MUTED),
            address_input,
            Space::new().height(12),
            row![save_btn, cancel_btn].spacing(8),
        ]
        .spacing(4);

        let card = container(detail)
            .padding(24)
            .max_width(480)
            .style(styles::card);

        Some(card.into())
    }
}
