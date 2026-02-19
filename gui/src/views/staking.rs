use crate::messages::Message;
use crate::{styles, App, MUTED};
use iced::widget::{button, column, container, row, table, text, text_input, Space};
use iced::{Element, Fill, Font, Length};
use jota_core::display::format_balance;
use jota_core::network::StakeStatus;

/// Pre-formatted row data for the active stakes table.
#[derive(Clone)]
struct StakeRow {
    validator_label: String,
    principal: String,
    reward: String,
    epoch: String,
    status_label: String,
    status_color: iced::Color,
    unstakeable: bool,
    object_id: String,
}

/// Pre-formatted row data for the validators table.
#[derive(Clone)]
struct ValidatorRow {
    index: usize,
    name: String,
    age: String,
    apy: String,
    commission: String,
    pool_balance: String,
}

impl App {
    pub(crate) fn view_staking(&self) -> Element<'_, Message> {
        let title = text("Staking").size(24);

        let header = row![
            title,
            Space::new().width(Fill),
            button(text("Refresh").size(13))
                .padding([8, 16])
                .style(styles::btn_secondary)
                .on_press(Message::RefreshStakes),
        ]
        .align_y(iced::Alignment::Center);

        let mut col = column![header].spacing(16);

        // -- Active stakes card --
        let mut stakes_content = column![text("Active Stakes").size(16)].spacing(12);

        if self.loading > 0 && self.stakes.is_empty() {
            stakes_content = stakes_content.push(text("Loading...").size(14).color(MUTED));
        } else if self.stakes.is_empty() {
            stakes_content = stakes_content.push(text("No active stakes.").size(14).color(MUTED));
        } else {
            let mut total_principal: u64 = 0;
            let mut total_reward: u64 = 0;

            let rows: Vec<StakeRow> = self
                .stakes
                .iter()
                .map(|stake| {
                    total_principal = total_principal.saturating_add(stake.principal);

                    let reward = match stake.estimated_reward {
                        Some(r) => {
                            total_reward = total_reward.saturating_add(r);
                            format_balance(r)
                        }
                        None => "-".into(),
                    };

                    let status_color = match stake.status {
                        StakeStatus::Active => styles::ACCENT,
                        StakeStatus::Pending => styles::WARNING,
                        StakeStatus::Unstaked => MUTED,
                    };

                    let validator_label = match &stake.validator_name {
                        Some(name) => name.clone(),
                        None => {
                            let id = stake.pool_id.to_string();
                            if id.len() > 10 {
                                format!("{}..{}", &id[..6], &id[id.len() - 4..])
                            } else {
                                id
                            }
                        }
                    };

                    StakeRow {
                        validator_label,
                        principal: format_balance(stake.principal),
                        reward,
                        epoch: format!("{}", stake.stake_activation_epoch),
                        status_label: format!("{}", stake.status),
                        status_color,
                        unstakeable: stake.status != StakeStatus::Unstaked,
                        object_id: stake.object_id.to_string(),
                    }
                })
                .collect();

            let loading = self.loading > 0;
            let tbl = table::table(
                [
                    table::column(
                        text("Validator").size(12).color(MUTED),
                        |r: StakeRow| -> Element<'_, Message> {
                            text(r.validator_label).size(13).into()
                        },
                    )
                    .width(Length::FillPortion(5)),
                    table::column(
                        text("Principal").size(12).color(MUTED),
                        |r: StakeRow| -> Element<'_, Message> { text(r.principal).size(13).into() },
                    )
                    .width(Length::FillPortion(4)),
                    table::column(
                        text("Reward").size(12).color(MUTED),
                        |r: StakeRow| -> Element<'_, Message> { text(r.reward).size(13).into() },
                    )
                    .width(Length::FillPortion(4)),
                    table::column(
                        text("Epoch").size(12).color(MUTED),
                        |r: StakeRow| -> Element<'_, Message> { text(r.epoch).size(13).into() },
                    )
                    .width(Length::FillPortion(2)),
                    table::column(
                        text("Status").size(12).color(MUTED),
                        |r: StakeRow| -> Element<'_, Message> {
                            text(r.status_label).size(13).color(r.status_color).into()
                        },
                    )
                    .width(Length::FillPortion(3)),
                    table::column(
                        text("").size(12),
                        move |r: StakeRow| -> Element<'_, Message> {
                            if r.unstakeable {
                                let mut btn = button(text("Unstake").size(12))
                                    .padding([6, 12])
                                    .style(styles::btn_danger);
                                if !loading {
                                    btn = btn.on_press(Message::ConfirmUnstake(r.object_id));
                                }
                                btn.into()
                            } else {
                                Space::new().into()
                            }
                        },
                    )
                    .width(Length::FillPortion(3)),
                ],
                rows,
            )
            .width(Fill)
            .padding_x(12)
            .padding_y(8)
            .separator_x(0)
            .separator_y(1);

            stakes_content = stakes_content.push(tbl);

            stakes_content = stakes_content.push(styles::separator());
            stakes_content = stakes_content.push(
                text(format!(
                    "Total: {}  ·  Rewards: {}",
                    format_balance(total_principal),
                    format_balance(total_reward),
                ))
                .size(13)
                .font(styles::BOLD),
            );
        }

        col = col.push(
            container(stakes_content)
                .padding(20)
                .width(Fill)
                .style(styles::card),
        );

        // -- Validators card (replaces old "New Stake" form) --
        let mut validators_content = column![text("Validators").size(16)].spacing(12);

        if self.loading > 0 && self.validators.is_empty() {
            validators_content =
                validators_content.push(text("Loading validators...").size(14).color(MUTED));
        } else if self.validators.is_empty() {
            validators_content =
                validators_content.push(text("No validators found.").size(14).color(MUTED));
        } else {
            let rows: Vec<ValidatorRow> = self
                .validators
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let name = if v.name.len() > 24 {
                        format!("{}...", &v.name[..22])
                    } else {
                        v.name.clone()
                    };
                    let age = format!("{}", v.age_epochs);
                    let apy = format!("{:.2}%", v.apy as f64 / 100.0);
                    let commission = format!("{:.1}%", v.commission_rate as f64 / 100.0);
                    let pool_balance = format_balance(v.staking_pool_iota_balance);

                    ValidatorRow {
                        index: i,
                        name,
                        age,
                        apy,
                        commission,
                        pool_balance,
                    }
                })
                .collect();

            let tbl = table::table(
                [
                    table::column(
                        text("Name").size(12).color(MUTED),
                        |r: ValidatorRow| -> Element<'_, Message> {
                            button(text(r.name).size(13))
                                .padding([4, 8])
                                .style(styles::btn_ghost)
                                .on_press(Message::SelectValidator(r.index))
                                .into()
                        },
                    )
                    .width(Length::FillPortion(5)),
                    table::column(
                        text("Age (Epochs)").size(12).color(MUTED),
                        |r: ValidatorRow| -> Element<'_, Message> { text(r.age).size(13).into() },
                    )
                    .width(Length::FillPortion(2)),
                    table::column(
                        text("APY").size(12).color(MUTED),
                        |r: ValidatorRow| -> Element<'_, Message> { text(r.apy).size(13).into() },
                    )
                    .width(Length::FillPortion(2)),
                    table::column(
                        text("Commission").size(12).color(MUTED),
                        |r: ValidatorRow| -> Element<'_, Message> {
                            text(r.commission).size(13).into()
                        },
                    )
                    .width(Length::FillPortion(3)),
                    table::column(
                        text("Pool Balance").size(12).color(MUTED),
                        |r: ValidatorRow| -> Element<'_, Message> {
                            text(r.pool_balance).size(13).into()
                        },
                    )
                    .width(Length::FillPortion(4)),
                ],
                rows,
            )
            .width(Fill)
            .padding_x(12)
            .padding_y(8)
            .separator_x(0)
            .separator_y(1);

            validators_content = validators_content.push(tbl);
        }

        col = col.push(
            container(validators_content)
                .padding(20)
                .width(Fill)
                .style(styles::card),
        );

        // Status messages
        if self.loading > 0 && !self.stakes.is_empty() {
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

    /// Build the validator detail modal overlay.
    pub(crate) fn view_validator_modal(&self) -> Option<Element<'_, Message>> {
        let idx = self.selected_validator?;
        let v = self.validators.get(idx)?;

        let mut detail = column![].spacing(8);

        // Validator name
        detail = detail.push(text(v.name.as_str()).size(18).font(styles::BOLD));

        // Address (truncated)
        let addr_display = if v.address.len() > 30 {
            format!(
                "{}...{}",
                &v.address[..14],
                &v.address[v.address.len() - 14..]
            )
        } else {
            v.address.clone()
        };
        detail = detail.push(
            text(addr_display)
                .size(12)
                .font(Font::MONOSPACE)
                .color(MUTED),
        );

        detail = detail.push(Space::new().height(4));

        // Info rows
        let age_display = format!("{} epochs", v.age_epochs);
        let apy_display = format!("{:.2}%", v.apy as f64 / 100.0);
        let commission_display = format!("{:.1}%", v.commission_rate as f64 / 100.0);
        let pool_display = format_balance(v.staking_pool_iota_balance);

        detail = detail.push(
            row![
                text("Age")
                    .size(12)
                    .color(MUTED)
                    .width(Length::Fixed(100.0)),
                text(age_display).size(13),
            ]
            .spacing(8),
        );
        detail = detail.push(
            row![
                text("APY")
                    .size(12)
                    .color(MUTED)
                    .width(Length::Fixed(100.0)),
                text(apy_display).size(13),
            ]
            .spacing(8),
        );
        detail = detail.push(
            row![
                text("Commission")
                    .size(12)
                    .color(MUTED)
                    .width(Length::Fixed(100.0)),
                text(commission_display).size(13),
            ]
            .spacing(8),
        );
        detail = detail.push(
            row![
                text("Pool Balance")
                    .size(12)
                    .color(MUTED)
                    .width(Length::Fixed(100.0)),
                text(pool_display).size(13),
            ]
            .spacing(8),
        );

        detail = detail.push(Space::new().height(4));

        // Explorer link
        detail = detail.push(
            button(text("View in Explorer →").size(12))
                .padding([6, 12])
                .style(styles::btn_secondary)
                .on_press(Message::OpenExplorerAddress(v.address.clone())),
        );

        detail = detail.push(styles::separator());

        // Amount input
        detail = detail.push(text("Amount").size(12).color(MUTED));
        let amount = text_input("Amount (IOTA)", &self.stake_amount)
            .on_input(Message::StakeAmountChanged)
            .on_submit(Message::ConfirmStake);
        detail = detail.push(amount);

        detail = detail.push(Space::new().height(4));

        // Action buttons
        let mut stake_btn = button(text("Stake").size(14))
            .padding([10, 24])
            .style(styles::btn_primary);
        if self.loading == 0 && !self.stake_amount.is_empty() {
            stake_btn = stake_btn.on_press(Message::ConfirmStake);
        }

        let close_btn = button(text("Close").size(14))
            .padding([10, 24])
            .style(styles::btn_ghost)
            .on_press(Message::SelectValidator(idx));

        detail = detail.push(row![stake_btn, close_btn].spacing(8));

        // Status messages inside the modal
        if self.loading > 0 {
            detail = detail.push(text("Processing...").size(13).color(MUTED));
        }
        if let Some(err) = &self.error_message {
            detail = detail.push(text(err.as_str()).size(13).color(styles::DANGER));
        }
        if let Some(msg) = &self.success_message {
            detail = detail.push(text(msg.as_str()).size(13).color(styles::ACCENT));
        }

        let card = container(detail)
            .padding(24)
            .max_width(480)
            .style(styles::card);

        Some(card.into())
    }
}
