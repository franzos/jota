use crate::messages::Message;
use crate::{styles, App, MUTED};
use iced::widget::{button, column, container, row, text, text_input, Space};
use iced::{Element, Fill, Length};
use iota_wallet_core::display::format_balance;
use iota_wallet_core::network::StakeStatus;

impl App {
    pub(crate) fn view_staking(&self) -> Element<Message> {
        let title = text("Staking").size(24);

        let header = row![title, Space::new().width(Fill)].align_y(iced::Alignment::Center);

        let mut col = column![header].spacing(16);

        // -- Active stakes card --
        let mut stakes_content = column![row![
            text("Active Stakes").size(16),
            Space::new().width(Fill),
            button(text("Refresh").size(13))
                .padding([8, 16])
                .style(styles::btn_secondary)
                .on_press(Message::RefreshStakes),
        ]
        .align_y(iced::Alignment::Center),]
        .spacing(12);

        if self.loading > 0 && self.stakes.is_empty() {
            stakes_content = stakes_content.push(text("Loading...").size(14).color(MUTED));
        } else if self.stakes.is_empty() {
            stakes_content = stakes_content.push(text("No active stakes.").size(14).color(MUTED));
        } else {
            let header = row![
                text("Principal")
                    .size(11)
                    .color(MUTED)
                    .width(Length::Fixed(110.0)),
                text("Reward")
                    .size(11)
                    .color(MUTED)
                    .width(Length::Fixed(110.0)),
                text("Epoch")
                    .size(11)
                    .color(MUTED)
                    .width(Length::Fixed(60.0)),
                text("Status")
                    .size(11)
                    .color(MUTED)
                    .width(Length::Fixed(70.0)),
                text("").size(11),
            ]
            .spacing(8);
            stakes_content = stakes_content.push(header);
            stakes_content = stakes_content.push(styles::separator());

            let mut total_principal: u64 = 0;
            let mut total_reward: u64 = 0;

            let mut stakes_col = column![].spacing(4);
            for stake in &self.stakes {
                total_principal = total_principal.saturating_add(stake.principal);

                let reward_str = match stake.estimated_reward {
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

                let mut stake_row = row![
                    text(format_balance(stake.principal))
                        .size(12)
                        .width(Length::Fixed(110.0)),
                    text(reward_str).size(12).width(Length::Fixed(110.0)),
                    text(format!("{}", stake.stake_activation_epoch))
                        .size(12)
                        .width(Length::Fixed(60.0)),
                    text(format!("{}", stake.status))
                        .size(12)
                        .color(status_color)
                        .width(Length::Fixed(70.0)),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center);

                if stake.status != StakeStatus::Unstaked {
                    let mut unstake_btn = button(text("Unstake").size(11))
                        .padding([4, 10])
                        .style(styles::btn_danger);
                    if self.loading == 0 {
                        unstake_btn = unstake_btn
                            .on_press(Message::ConfirmUnstake(stake.object_id.to_string()));
                    }
                    stake_row = stake_row.push(unstake_btn);
                }

                stakes_col = stakes_col.push(stake_row);
            }
            stakes_content = stakes_content.push(stakes_col);

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

        // -- New stake form card --
        let validator = text_input("Validator address or .iota name", &self.validator_address)
            .on_input(Message::ValidatorAddressChanged);

        // Show resolved address or error below the input
        let resolved_hint: Option<Element<Message>> = match &self.resolved_validator {
            Some(Ok(addr)) => Some(
                text(format!("Resolved: {addr}"))
                    .size(11)
                    .color(styles::ACCENT)
                    .into(),
            ),
            Some(Err(e)) => Some(text(e.as_str()).size(11).color(styles::DANGER).into()),
            None => None,
        };

        let amount = text_input("Amount (IOTA)", &self.stake_amount)
            .on_input(Message::StakeAmountChanged)
            .on_submit(Message::ConfirmStake);

        let mut stake_btn = button(text("Stake").size(14))
            .padding([10, 24])
            .style(styles::btn_primary);
        if self.loading == 0 && !self.validator_address.is_empty() && !self.stake_amount.is_empty()
        {
            stake_btn = stake_btn.on_press(Message::ConfirmStake);
        }

        let mut form_content = column![
            text("New Stake").size(16),
            Space::new().height(4),
            text("Validator").size(12).color(MUTED),
            validator,
        ]
        .spacing(4);
        if let Some(hint) = resolved_hint {
            form_content = form_content.push(hint);
        }
        form_content = form_content
            .push(Space::new().height(4))
            .push(text("Amount").size(12).color(MUTED))
            .push(amount)
            .push(Space::new().height(8))
            .push(stake_btn);

        col = col.push(
            container(form_content)
                .padding(24)
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
}
