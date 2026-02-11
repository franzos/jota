use crate::messages::Message;
use crate::{App, BORDER, MUTED};
use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Color, Element, Fill, Length};
use iota_wallet_core::display::format_balance;
use iota_wallet_core::network::StakeStatus;

impl App {
    pub(crate) fn view_staking(&self) -> Element<Message> {
        let title = text("Staking").size(24);

        let mut col = column![
            title,
            Space::new().height(10),
        ]
        .spacing(5);

        // -- Active stakes --
        col = col.push(text("Active Stakes").size(18));

        let refresh = button(text("Refresh").size(14)).on_press(Message::RefreshStakes);
        col = col.push(refresh);

        if self.loading && self.stakes.is_empty() {
            col = col.push(text("Loading...").size(14));
        } else if self.stakes.is_empty() {
            col = col.push(text("No active stakes.").size(14));
        } else {
            let header = row![
                text("Principal").size(11).width(Length::Fixed(110.0)),
                text("Reward").size(11).width(Length::Fixed(110.0)),
                text("Epoch").size(11).width(Length::Fixed(60.0)),
                text("Status").size(11).width(Length::Fixed(70.0)),
                text("").size(11),
            ]
            .spacing(8);
            col = col.push(header);

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
            col = col.push(separator);

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
                    StakeStatus::Active => Color::from_rgb(0.059, 0.757, 0.718),
                    StakeStatus::Pending => Color::from_rgb(1.0, 0.757, 0.027),
                    StakeStatus::Unstaked => MUTED,
                };

                let mut stake_row = row![
                    text(format_balance(stake.principal)).size(12).width(Length::Fixed(110.0)),
                    text(reward_str).size(12).width(Length::Fixed(110.0)),
                    text(format!("{}", stake.stake_activation_epoch)).size(12).width(Length::Fixed(60.0)),
                    text(format!("{}", stake.status)).size(12).color(status_color).width(Length::Fixed(70.0)),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center);

                if stake.status != StakeStatus::Unstaked {
                    let mut unstake_btn = button(text("Unstake").size(11));
                    if !self.loading {
                        unstake_btn = unstake_btn
                            .on_press(Message::ConfirmUnstake(stake.object_id.to_string()));
                    }
                    stake_row = stake_row.push(unstake_btn);
                }

                stakes_col = stakes_col.push(stake_row);
            }
            col = col.push(stakes_col);

            col = col.push(Space::new().height(5));
            col = col.push(
                text(format!(
                    "Total: {}  rewards: {}",
                    format_balance(total_principal),
                    format_balance(total_reward),
                ))
                .size(13),
            );
        }

        // -- New stake form --
        col = col.push(Space::new().height(20));
        col = col.push(text("New Stake").size(18));

        let validator = text_input("Validator address (0x...)", &self.validator_address)
            .on_input(Message::ValidatorAddressChanged);
        let amount = text_input("Amount (IOTA)", &self.stake_amount)
            .on_input(Message::StakeAmountChanged)
            .on_submit(Message::ConfirmStake);

        let mut stake_btn = button(text("Stake").size(14)).style(button::primary);
        if !self.loading && !self.validator_address.is_empty() && !self.stake_amount.is_empty()
        {
            stake_btn = stake_btn.on_press(Message::ConfirmStake);
        }

        col = col.push(text("Validator").size(12));
        col = col.push(validator);
        col = col.push(Space::new().height(3));
        col = col.push(text("Amount").size(12));
        col = col.push(amount);
        col = col.push(Space::new().height(5));
        col = col.push(stake_btn);

        // Status messages
        if self.loading && !self.stakes.is_empty() {
            col = col.push(text("Processing...").size(14));
        }
        if let Some(msg) = &self.success_message {
            col = col.push(text(msg.as_str()).size(14).color([0.059, 0.757, 0.718]));
        }
        if let Some(err) = &self.error_message {
            col = col.push(text(err.as_str()).size(14).color([0.906, 0.192, 0.192]));
        }

        scrollable(col).into()
    }
}
