use base64ct::Encoding;

use crate::helpers::truncate_str;
use crate::messages::Message;
use crate::{styles, App, MUTED};
use iced::widget::{button, column, container, row, scrollable, table, text, text_input, Space};
use iced::{Element, Fill, Font, Length};
use jota_core::display::format_balance;
use jota_core::multisig::ProposalStatus;

/// Pre-formatted row data for the multisig addresses table.
#[derive(Clone)]
struct ConfigRow {
    index: usize,
    name: String,
    address_short: String,
    threshold: String,
    members: String,
}

/// Pre-formatted row data for the proposals table.
#[derive(Clone)]
struct ProposalRow {
    index: usize,
    digest_short: String,
    multisig_name: String,
    action: String,
    status_label: String,
    status_color: iced::Color,
}

fn scheme_name(key: &jota_core::MultisigMemberPublicKey) -> &'static str {
    use jota_core::MultisigMemberPublicKey;
    match key {
        MultisigMemberPublicKey::Ed25519(_) => "Ed25519",
        MultisigMemberPublicKey::Secp256k1(_) => "Secp256k1",
        MultisigMemberPublicKey::Secp256r1(_) => "Secp256r1",
        _ => "Unknown",
    }
}

fn status_label(status: &ProposalStatus) -> String {
    match status {
        ProposalStatus::Pending => "Pending".into(),
        ProposalStatus::Ready => "Ready".into(),
        ProposalStatus::Submitted { digest } => {
            format!("Submitted ({})", truncate_str(digest, 6, 4))
        }
        ProposalStatus::Failed { reason } => format!("Failed: {reason}"),
        ProposalStatus::Stale { reason } => format!("Stale: {reason}"),
        ProposalStatus::Cancelled => "Cancelled".into(),
    }
}

fn status_color(status: &ProposalStatus) -> iced::Color {
    match status {
        ProposalStatus::Pending => styles::WARNING,
        ProposalStatus::Ready => styles::ACCENT,
        ProposalStatus::Submitted { .. } => styles::ACCENT,
        ProposalStatus::Failed { .. } => styles::DANGER,
        ProposalStatus::Stale { .. } => styles::DANGER,
        ProposalStatus::Cancelled => MUTED,
    }
}

impl App {
    pub(crate) fn view_multisig(&self) -> Element<'_, Message> {
        let title = text("Multisig").size(24);

        let refresh_btn = button(text("Refresh").size(13))
            .padding([8, 16])
            .style(styles::btn_secondary)
            .on_press(Message::RefreshMultisig);

        let import_btn = button(text("Import").size(13))
            .padding([8, 16])
            .style(styles::btn_secondary)
            .on_press(Message::MultisigImportFile);

        let create_btn = button(text("+ Create").size(13))
            .padding([8, 16])
            .style(styles::btn_primary)
            .on_press(Message::MultisigOpenCreate);

        let sign_btn = button(text("Sign Proposal").size(13))
            .padding([8, 16])
            .style(styles::btn_secondary)
            .on_press(Message::MultisigSignExternalOpen);

        let header = row![
            title,
            Space::new().width(Fill),
            import_btn,
            sign_btn,
            create_btn,
            refresh_btn
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let mut col = column![header].spacing(16);

        // Inline import path input
        if self.multisig_import_visible {
            let import_card = container(
                column![
                    text("Import Multisig Config").size(14).font(styles::BOLD),
                    row![
                        text_input("/path/to/file.jota-multisig", &self.multisig_import_path)
                            .on_input(Message::MultisigImportPathChanged)
                            .on_submit(Message::MultisigImportConfirm)
                            .size(13),
                        button(text("Browse").size(13))
                            .padding([8, 16])
                            .style(styles::btn_secondary)
                            .on_press(Message::MultisigBrowseImport),
                    ]
                    .spacing(8),
                    row![
                        button(text("Load").size(13))
                            .padding([8, 16])
                            .style(styles::btn_primary)
                            .on_press(Message::MultisigImportConfirm),
                        button(text("Cancel").size(13))
                            .padding([8, 16])
                            .style(styles::btn_ghost)
                            .on_press(Message::MultisigCloseImport),
                    ]
                    .spacing(8),
                ]
                .spacing(8),
            )
            .padding(16)
            .width(Fill)
            .style(styles::card);
            col = col.push(import_card);
        }

        // Your public key — needed for co-signers to add you to a multisig
        if let Some(pk_b64) = &self.multisig_my_public_key_b64 {
            col = col.push(
                container(
                    row![
                        text("Your public key").size(12).color(MUTED),
                        text(pk_b64).size(11).font(Font::MONOSPACE),
                        button(text("Copy").size(11))
                            .padding([4, 10])
                            .style(styles::btn_secondary)
                            .on_press(Message::MultisigCopyPublicKey),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                )
                .padding([8, 16])
                .width(Fill)
                .style(styles::card),
            );
        }

        if self.multisig_configs.is_empty() && self.multisig_proposals.is_empty() {
            col = col.push(
                container(
                    column![
                        text("No multisig addresses configured.").size(14).color(MUTED),
                        Space::new().height(4),
                        text("A multisig address requires multiple people to approve transactions. Create one if you're setting it up, or import a file (.jota-multisig) shared by a co-signer.")
                            .size(12)
                            .color(MUTED),
                        Space::new().height(8),
                        row![
                            button(text("Import File").size(13))
                                .padding([8, 16])
                                .style(styles::btn_secondary)
                                .on_press(Message::MultisigImportFile),
                            button(text("Create New").size(13))
                                .padding([8, 16])
                                .style(styles::btn_primary)
                                .on_press(Message::MultisigOpenCreate),
                        ]
                        .spacing(8),
                    ]
                    .spacing(4),
                )
                .padding(24)
                .width(Fill)
                .style(styles::card),
            );
        } else {
            // Addresses section
            if !self.multisig_configs.is_empty() {
                let mut addresses_content = column![text("Addresses").size(16)].spacing(12);

                let rows: Vec<ConfigRow> = self
                    .multisig_configs
                    .iter()
                    .enumerate()
                    .map(|(i, config)| {
                        let address = config.address().to_string();
                        ConfigRow {
                            index: i,
                            name: config.name.clone(),
                            address_short: truncate_str(&address, 10, 8),
                            threshold: format!(
                                "{}/{}",
                                config.committee.threshold(),
                                config.committee.members().len()
                            ),
                            members: format!("{}", config.committee.members().len()),
                        }
                    })
                    .collect();

                let tbl = table::table(
                    [
                        table::column(
                            text("Name").size(12).color(MUTED),
                            |r: ConfigRow| -> Element<'_, Message> {
                                button(text(r.name).size(13))
                                    .padding([4, 8])
                                    .style(styles::btn_ghost)
                                    .on_press(Message::MultisigSelectConfig(r.index))
                                    .into()
                            },
                        )
                        .width(Length::FillPortion(4)),
                        table::column(
                            text("Address").size(12).color(MUTED),
                            |r: ConfigRow| -> Element<'_, Message> {
                                text(r.address_short)
                                    .size(12)
                                    .font(Font::MONOSPACE)
                                    .color(MUTED)
                                    .into()
                            },
                        )
                        .width(Length::FillPortion(5)),
                        table::column(
                            text("Threshold").size(12).color(MUTED),
                            |r: ConfigRow| -> Element<'_, Message> {
                                text(r.threshold).size(13).into()
                            },
                        )
                        .width(Length::FillPortion(2)),
                        table::column(
                            text("Members").size(12).color(MUTED),
                            |r: ConfigRow| -> Element<'_, Message> {
                                text(r.members).size(13).into()
                            },
                        )
                        .width(Length::FillPortion(2)),
                    ],
                    rows,
                )
                .width(Fill)
                .padding_x(12)
                .padding_y(8)
                .separator_x(0)
                .separator_y(1);

                addresses_content = addresses_content.push(tbl);

                col = col.push(
                    container(addresses_content)
                        .padding(20)
                        .width(Fill)
                        .style(styles::card),
                );
            }

            // Proposals section
            if !self.multisig_proposals.is_empty() {
                let mut proposals_content = column![text("Proposals").size(16)].spacing(12);

                let rows: Vec<ProposalRow> = self
                    .multisig_proposals
                    .iter()
                    .enumerate()
                    .map(|(i, proposal)| {
                        let digest_short = if proposal.tx_digest.len() > 8 {
                            proposal.tx_digest[..8].to_string()
                        } else {
                            proposal.tx_digest.clone()
                        };

                        // Find the config name for this proposal's multisig address
                        let multisig_name = self
                            .multisig_configs
                            .iter()
                            .find(|c| {
                                c.address().to_string().to_lowercase()
                                    == proposal.multisig_address.to_lowercase()
                            })
                            .map(|c| c.name.clone())
                            .unwrap_or_else(|| truncate_str(&proposal.multisig_address, 8, 6));

                        // Describe the action from tx_bytes
                        let action =
                            match jota_core::multisig::describe_transaction(&proposal.tx_bytes) {
                                Ok(desc) => {
                                    if let (Some(amount), Some(ref recipient)) =
                                        (desc.amount, &desc.recipient)
                                    {
                                        format!(
                                            "Send {} to {}",
                                            format_balance(amount),
                                            truncate_str(recipient, 8, 6)
                                        )
                                    } else {
                                        "Transaction".into()
                                    }
                                }
                                Err(_) => "Unknown".into(),
                            };

                        ProposalRow {
                            index: i,
                            digest_short,
                            multisig_name,
                            action,
                            status_label: status_label(&proposal.status),
                            status_color: status_color(&proposal.status),
                        }
                    })
                    .collect();

                let tbl = table::table(
                    [
                        table::column(
                            text("ID").size(12).color(MUTED),
                            |r: ProposalRow| -> Element<'_, Message> {
                                button(text(r.digest_short).size(12).font(Font::MONOSPACE))
                                    .padding([4, 8])
                                    .style(styles::btn_ghost)
                                    .on_press(Message::MultisigSelectProposal(r.index))
                                    .into()
                            },
                        )
                        .width(Length::FillPortion(2)),
                        table::column(
                            text("Multisig").size(12).color(MUTED),
                            |r: ProposalRow| -> Element<'_, Message> {
                                text(r.multisig_name).size(13).into()
                            },
                        )
                        .width(Length::FillPortion(3)),
                        table::column(
                            text("Action").size(12).color(MUTED),
                            |r: ProposalRow| -> Element<'_, Message> {
                                text(r.action).size(13).into()
                            },
                        )
                        .width(Length::FillPortion(5)),
                        table::column(
                            text("Status").size(12).color(MUTED),
                            |r: ProposalRow| -> Element<'_, Message> {
                                text(r.status_label).size(13).color(r.status_color).into()
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

                proposals_content = proposals_content.push(tbl);

                col = col.push(
                    container(proposals_content)
                        .padding(20)
                        .width(Fill)
                        .style(styles::card),
                );
            }
        }

        self.push_status(col, "Loading multisig data...").into()
    }

    pub(crate) fn view_multisig_detail_modal(&self) -> Option<Element<'_, Message>> {
        let idx = self.multisig_selected?;
        let config = self.multisig_configs.get(idx)?;

        let address = config.address().to_string();
        let address_display = truncate_str(&address, 14, 14);

        let mut detail = column![].spacing(8);

        detail = detail.push(text(config.name.as_str()).size(18).font(styles::BOLD));

        detail = detail.push(
            row![
                text(address_display)
                    .size(12)
                    .font(Font::MONOSPACE)
                    .color(MUTED),
                button(text("Copy").size(11))
                    .padding([4, 10])
                    .style(styles::btn_secondary)
                    .on_press(Message::MultisigCopyAddress(address)),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );

        detail = detail.push(Space::new().height(4));

        // Network
        detail = detail.push(
            row![
                text("Network")
                    .size(12)
                    .color(MUTED)
                    .width(Length::Fixed(100.0)),
                text(format!("{}", config.network)).size(13),
            ]
            .spacing(8),
        );

        // Threshold
        let total_weight: u16 = config
            .committee
            .members()
            .iter()
            .map(|m| m.weight() as u16)
            .sum();
        detail = detail.push(
            row![
                text("Threshold")
                    .size(12)
                    .color(MUTED)
                    .width(Length::Fixed(100.0)),
                text(format!(
                    "{} of {} (total weight {})",
                    config.committee.threshold(),
                    config.committee.members().len(),
                    total_weight
                ))
                .size(13),
            ]
            .spacing(8),
        );

        detail = detail.push(Space::new().height(4));
        detail = detail.push(text("Members").size(14).font(styles::BOLD));

        // Member list
        for (i, member) in config.committee.members().iter().enumerate() {
            let label = config
                .labels
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("Member {}", i + 1));

            let is_me = config
                .my_key
                .as_ref()
                .map(|k| k == member.public_key())
                .unwrap_or(false);

            let suffix = if is_me { " (you)" } else { "" };

            let member_text = format!(
                "{}{}  ·  weight: {}  ·  {}",
                label,
                suffix,
                member.weight(),
                scheme_name(member.public_key()),
            );

            detail = detail.push(text(member_text).size(12));
        }

        detail = detail.push(Space::new().height(8));

        // Actions
        let is_active = self.active_multisig == Some(idx);
        let activate_btn = if is_active {
            button(text("Active").size(13))
                .padding([8, 16])
                .style(styles::btn_ghost)
        } else {
            button(text("Use Address").size(13))
                .padding([8, 16])
                .style(styles::btn_primary)
                .on_press(Message::MultisigActivate(idx))
        };

        let send_btn = button(text("Send").size(13))
            .padding([8, 16])
            .style(styles::btn_secondary)
            .on_press(Message::MultisigOpenSend(idx));

        let export_btn = button(text("Export").size(13))
            .padding([8, 16])
            .style(styles::btn_secondary)
            .on_press(Message::MultisigExportConfig(idx));

        let remove_btn = button(text("Remove").size(13))
            .padding([8, 16])
            .style(styles::btn_danger)
            .on_press(Message::MultisigRemoveConfig(config.name.clone()));

        let close_btn = button(text("Close").size(13))
            .padding([8, 16])
            .style(styles::btn_ghost)
            .on_press(Message::MultisigCloseDetail);

        detail =
            detail.push(row![activate_btn, send_btn, export_btn, remove_btn, close_btn].spacing(8));

        let card = container(detail)
            .padding(24)
            .max_width(560)
            .style(styles::card);

        Some(card.into())
    }

    pub(crate) fn view_multisig_proposal_modal(&self) -> Option<Element<'_, Message>> {
        let idx = self.multisig_proposal_selected?;
        let proposal = self.multisig_proposals.get(idx)?;

        let mut detail = column![].spacing(8);

        // Title
        let digest_short = if proposal.tx_digest.len() > 8 {
            &proposal.tx_digest[..8]
        } else {
            &proposal.tx_digest
        };
        detail = detail.push(
            text(format!("Proposal {digest_short}"))
                .size(18)
                .font(styles::BOLD),
        );

        // Full digest
        let digest_display = truncate_str(&proposal.tx_digest, 14, 14);
        detail = detail.push(
            row![
                text("Digest")
                    .size(12)
                    .color(MUTED)
                    .width(Length::Fixed(100.0)),
                text(digest_display).size(12).font(Font::MONOSPACE),
            ]
            .spacing(8),
        );

        // Multisig address
        let addr_display = truncate_str(&proposal.multisig_address, 14, 14);
        detail = detail.push(
            row![
                text("Multisig")
                    .size(12)
                    .color(MUTED)
                    .width(Length::Fixed(100.0)),
                text(addr_display).size(12).font(Font::MONOSPACE),
            ]
            .spacing(8),
        );

        // Decoded action
        if let Ok(desc) = jota_core::multisig::describe_transaction(&proposal.tx_bytes) {
            if let Some(amount) = desc.amount {
                detail = detail.push(
                    row![
                        text("Amount")
                            .size(12)
                            .color(MUTED)
                            .width(Length::Fixed(100.0)),
                        text(format_balance(amount)).size(13).font(styles::BOLD),
                    ]
                    .spacing(8),
                );
            }
            if let Some(ref recipient) = desc.recipient {
                let recip_display = truncate_str(recipient, 14, 14);
                detail = detail.push(
                    row![
                        text("Recipient")
                            .size(12)
                            .color(MUTED)
                            .width(Length::Fixed(100.0)),
                        text(recip_display).size(12).font(Font::MONOSPACE),
                    ]
                    .spacing(8),
                );
            }
            detail = detail.push(
                row![
                    text("Gas Budget")
                        .size(12)
                        .color(MUTED)
                        .width(Length::Fixed(100.0)),
                    text(format_balance(desc.gas_budget)).size(12),
                ]
                .spacing(8),
            );
        }

        // Status
        detail = detail.push(
            row![
                text("Status")
                    .size(12)
                    .color(MUTED)
                    .width(Length::Fixed(100.0)),
                text(status_label(&proposal.status))
                    .size(13)
                    .color(status_color(&proposal.status)),
            ]
            .spacing(8),
        );

        detail = detail.push(Space::new().height(4));

        // Signature checklist
        let config = self.multisig_configs.iter().find(|c| {
            c.address().to_string().to_lowercase() == proposal.multisig_address.to_lowercase()
        });

        if let Some(config) = config {
            detail = detail.push(text("Signatures").size(14).font(styles::BOLD));

            for (i, member) in config.committee.members().iter().enumerate() {
                let label = config
                    .labels
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("Member {}", i + 1));

                let signed = proposal
                    .signatures
                    .iter()
                    .any(|s| &s.member_key == member.public_key());

                let (icon, color) = if signed {
                    ("  [signed]", styles::ACCENT)
                } else {
                    ("  [pending]", MUTED)
                };

                detail = detail.push(
                    text(format!("{label} (w:{})  {icon}", member.weight()))
                        .size(12)
                        .color(color),
                );
            }
        }

        detail = detail.push(Space::new().height(8));

        // Action buttons
        let mut actions = row![].spacing(8);

        if proposal.status == ProposalStatus::Ready {
            let mut submit_btn = button(text("Submit").size(13))
                .padding([8, 16])
                .style(styles::btn_primary);
            if self.loading == 0 {
                submit_btn = submit_btn
                    .on_press(Message::MultisigSubmitProposal(proposal.tx_digest.clone()));
            }
            actions = actions.push(submit_btn);
        }

        if proposal.status == ProposalStatus::Pending {
            let import_sig_btn = button(text("Import Signature").size(13))
                .padding([8, 16])
                .style(styles::btn_secondary)
                .on_press(Message::MultisigImportSignature(proposal.tx_digest.clone()));
            actions = actions.push(import_sig_btn);

            let export_prop_btn = button(text("Export").size(13))
                .padding([8, 16])
                .style(styles::btn_secondary)
                .on_press(Message::MultisigExportProposal(proposal.tx_digest.clone()));
            actions = actions.push(export_prop_btn);

            let cancel_btn = button(text("Cancel Proposal").size(13))
                .padding([8, 16])
                .style(styles::btn_danger)
                .on_press(Message::MultisigCancelProposal(proposal.tx_digest.clone()));
            actions = actions.push(cancel_btn);
        }

        let close_btn = button(text("Close").size(13))
            .padding([8, 16])
            .style(styles::btn_ghost)
            .on_press(Message::MultisigCloseProposal);
        actions = actions.push(close_btn);

        detail = detail.push(actions);

        // Inline import signature path input
        if self.multisig_import_sig_visible {
            detail = detail.push(Space::new().height(4));
            detail = detail.push(
                container(
                    column![
                        text("Import Signature").size(13).font(styles::BOLD),
                        row![
                            text_input("/path/to/file.jota-sig", &self.multisig_import_sig_path,)
                                .on_input(Message::MultisigImportSigPathChanged)
                                .on_submit(Message::MultisigImportSigConfirm)
                                .size(13),
                            button(text("Browse").size(13))
                                .padding([8, 16])
                                .style(styles::btn_secondary)
                                .on_press(Message::MultisigBrowseImportSig),
                        ]
                        .spacing(8),
                        button(text("Load").size(13))
                            .padding([8, 16])
                            .style(styles::btn_primary)
                            .on_press(Message::MultisigImportSigConfirm),
                    ]
                    .spacing(8),
                )
                .padding(12)
                .width(Fill)
                .style(styles::card),
            );
        }

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
            .max_width(560)
            .style(styles::card);

        Some(card.into())
    }

    pub(crate) fn view_multisig_create_modal(&self) -> Option<Element<'_, Message>> {
        if !self.multisig_create_visible {
            return None;
        }

        let mut detail = column![].spacing(12);

        match self.multisig_create_step {
            // Step 0: Basics
            0 => {
                detail = detail.push(text("Create Multisig").size(18).font(styles::BOLD));

                detail = detail.push(
                    text("Set up a shared address that requires multiple approvals to send funds. You'll need the base64 public key from each co-signer.")
                        .size(12)
                        .color(MUTED),
                );

                detail = detail.push(
                    column![
                        text("Name").size(12).color(MUTED),
                        text_input("e.g. company-funds", &self.multisig_create_name)
                            .on_input(Message::MultisigCreateNameChanged)
                            .size(13),
                    ]
                    .spacing(4),
                );

                detail = detail.push(
                    column![
                        text("Total number of participants (including you)")
                            .size(12)
                            .color(MUTED),
                        text_input("2", &self.multisig_create_num_participants)
                            .on_input(Message::MultisigCreateNumParticipantsChanged)
                            .size(13),
                    ]
                    .spacing(4),
                );

                detail = detail.push(
                    column![
                        text("Threshold (how many must approve a transaction)")
                            .size(12)
                            .color(MUTED),
                        text_input(
                            &format!("default: {}", self.multisig_create_num_participants),
                            &self.multisig_create_threshold,
                        )
                        .on_input(Message::MultisigCreateThresholdChanged)
                        .size(13),
                    ]
                    .spacing(4),
                );

                detail = detail.push(Space::new().height(4));

                let mut actions = row![].spacing(8);
                actions = actions.push(
                    button(text("Cancel").size(13))
                        .padding([8, 16])
                        .style(styles::btn_ghost)
                        .on_press(Message::MultisigCloseCreate),
                );
                actions = actions.push(
                    button(text("Next").size(13))
                        .padding([8, 16])
                        .style(styles::btn_primary)
                        .on_press(Message::MultisigCreateNextStep),
                );
                detail = detail.push(actions);
            }
            // Step 1: Members
            1 => {
                detail = detail.push(text("Participants").size(18).font(styles::BOLD));

                detail = detail.push(
                    text("Each participant needs to share their public key (base64). Copy yours below and send it to your co-signers, then paste theirs into the fields below.")
                        .size(12)
                        .color(MUTED),
                );

                // Local user (participant 1) — with public key display
                let mut you_card = column![
                    text("Participant 1 (you)").size(13).font(styles::BOLD),
                    row![
                        column![
                            text("Label").size(11).color(MUTED),
                            text_input("me", &self.multisig_create_my_label)
                                .on_input(Message::MultisigCreateMyLabelChanged)
                                .size(13),
                        ]
                        .spacing(4)
                        .width(Fill),
                        column![
                            text("Weight").size(11).color(MUTED),
                            text_input("1", &self.multisig_create_my_weight)
                                .on_input(Message::MultisigCreateMyWeightChanged)
                                .size(13),
                        ]
                        .spacing(4)
                        .width(Length::Fixed(60.0)),
                    ]
                    .spacing(8),
                ]
                .spacing(8);

                // Show public key with copy button
                if let Some(pk_b64) = &self.multisig_my_public_key_b64 {
                    you_card = you_card.push(
                        column![
                            text("Your public key (share this with co-signers)")
                                .size(11)
                                .color(MUTED),
                            row![
                                text(pk_b64).size(11).font(Font::MONOSPACE),
                                button(text("Copy").size(11))
                                    .padding([4, 10])
                                    .style(styles::btn_secondary)
                                    .on_press(Message::MultisigCopyPublicKey),
                            ]
                            .spacing(8)
                            .align_y(iced::Alignment::Center),
                        ]
                        .spacing(4),
                    );
                }

                detail = detail.push(container(you_card).padding(12).style(styles::card));

                // Remote participants
                for (i, member) in self.multisig_create_members.iter().enumerate() {
                    let idx = i;
                    let participant_num = i + 2;
                    detail = detail.push(
                        container(
                            column![
                                text(format!("Participant {participant_num}"))
                                    .size(13)
                                    .font(styles::BOLD),
                                row![
                                    column![
                                        text("Label").size(11).color(MUTED),
                                        text_input("e.g. bob", &member.label)
                                            .on_input(move |v| {
                                                Message::MultisigCreateMemberLabelChanged(idx, v)
                                            })
                                            .size(13),
                                    ]
                                    .spacing(4)
                                    .width(Fill),
                                    column![
                                        text("Weight").size(11).color(MUTED),
                                        text_input("1", &member.weight)
                                            .on_input(move |v| {
                                                Message::MultisigCreateMemberWeightChanged(idx, v)
                                            })
                                            .size(13),
                                    ]
                                    .spacing(4)
                                    .width(Length::Fixed(60.0)),
                                ]
                                .spacing(8),
                                column![
                                    text("Public key (base64)").size(11).color(MUTED),
                                    text_input(
                                        "paste their base64 public key here",
                                        &member.public_key
                                    )
                                    .on_input(move |v| {
                                        Message::MultisigCreateMemberKeyChanged(idx, v)
                                    })
                                    .size(13),
                                ]
                                .spacing(4),
                                row![
                                    column![
                                        text("Scheme (for 33-byte keys)").size(11).color(MUTED),
                                        text_input("secp256k1", &member.scheme)
                                            .on_input(move |v| {
                                                Message::MultisigCreateMemberSchemeChanged(idx, v)
                                            })
                                            .size(13),
                                    ]
                                    .spacing(4)
                                    .width(Length::Fixed(200.0)),
                                    text(
                                        "32b = Ed25519, 33b = Secp256k1 (type 'r1' for Secp256r1)"
                                    )
                                    .size(11)
                                    .color(MUTED),
                                ]
                                .spacing(8)
                                .align_y(iced::Alignment::End),
                            ]
                            .spacing(8),
                        )
                        .padding(12)
                        .style(styles::card),
                    );
                }

                detail = detail.push(Space::new().height(4));

                let mut actions = row![].spacing(8);
                actions = actions.push(
                    button(text("Back").size(13))
                        .padding([8, 16])
                        .style(styles::btn_ghost)
                        .on_press(Message::MultisigCreatePrevStep),
                );
                actions = actions.push(
                    button(text("Next").size(13))
                        .padding([8, 16])
                        .style(styles::btn_primary)
                        .on_press(Message::MultisigCreateNextStep),
                );
                detail = detail.push(actions);
            }
            // Step 2: Confirm
            2 => {
                detail = detail.push(text("Confirm Multisig").size(18).font(styles::BOLD));

                detail = detail.push(
                    text("Review the setup below. After creating, share the exported .jota-multisig file with all participants so they can import the same address.")
                        .size(12)
                        .color(MUTED),
                );

                let n: usize = self.multisig_create_num_participants.parse().unwrap_or(2);
                let threshold: u16 = self.multisig_create_threshold.parse().unwrap_or(n as u16);
                let my_weight: u8 = self.multisig_create_my_weight.parse().unwrap_or(1);

                let total_weight: u16 = my_weight as u16
                    + self
                        .multisig_create_members
                        .iter()
                        .map(|m| m.weight.parse::<u8>().unwrap_or(1) as u16)
                        .sum::<u16>();

                detail = detail.push(
                    row![
                        text("Name")
                            .size(12)
                            .color(MUTED)
                            .width(Length::Fixed(100.0)),
                        text(&self.multisig_create_name).size(13),
                    ]
                    .spacing(8),
                );

                detail = detail.push(
                    row![
                        text("Threshold")
                            .size(12)
                            .color(MUTED)
                            .width(Length::Fixed(100.0)),
                        text(format!("{threshold} of {n} (total weight {total_weight})")).size(13),
                    ]
                    .spacing(8),
                );

                detail = detail.push(Space::new().height(4));
                detail = detail.push(text("Members").size(14).font(styles::BOLD));

                // Local user
                detail = detail.push(
                    text(format!(
                        "1. {} (you)  ·  weight: {}  ·  Ed25519",
                        self.multisig_create_my_label, self.multisig_create_my_weight,
                    ))
                    .size(12),
                );

                // Remote members
                for (i, m) in self.multisig_create_members.iter().enumerate() {
                    let w: u8 = m.weight.parse().unwrap_or(1);
                    let key_len = base64ct::Base64::decode_vec(m.public_key.trim())
                        .map(|b| b.len())
                        .unwrap_or(0);
                    let scheme = match key_len {
                        32 => "Ed25519",
                        33 => {
                            if m.scheme.to_lowercase() == "secp256r1"
                                || m.scheme.to_lowercase() == "r1"
                            {
                                "Secp256r1"
                            } else {
                                "Secp256k1"
                            }
                        }
                        _ => "Unknown",
                    };
                    detail = detail.push(
                        text(format!(
                            "{}. {}  ·  weight: {}  ·  {scheme}",
                            i + 2,
                            m.label,
                            w,
                        ))
                        .size(12),
                    );
                }

                detail = detail.push(Space::new().height(8));

                let mut actions = row![].spacing(8);
                actions = actions.push(
                    button(text("Back").size(13))
                        .padding([8, 16])
                        .style(styles::btn_ghost)
                        .on_press(Message::MultisigCreatePrevStep),
                );
                let mut confirm_btn = button(text("Create").size(13))
                    .padding([8, 16])
                    .style(styles::btn_primary);
                if self.loading == 0 {
                    confirm_btn = confirm_btn.on_press(Message::MultisigCreateConfirm);
                }
                actions = actions.push(confirm_btn);
                detail = detail.push(actions);
            }
            _ => {}
        }

        // Show wizard-specific errors
        if let Some(err) = &self.multisig_create_error {
            detail = detail.push(text(err.as_str()).size(13).color(styles::DANGER));
        }
        if self.loading > 0 {
            detail = detail.push(text("Processing...").size(13).color(MUTED));
        }

        let card = container(scrollable(detail).height(Length::Shrink))
            .padding(24)
            .max_width(560)
            .style(styles::card);

        Some(card.into())
    }

    pub(crate) fn view_multisig_send_modal(&self) -> Option<Element<'_, Message>> {
        if !self.multisig_send_visible {
            return None;
        }
        let idx = self.multisig_send_config_idx?;
        let config = self.multisig_configs.get(idx)?;

        let address = config.address().to_string();
        let address_display = truncate_str(&address, 14, 14);

        let mut detail = column![].spacing(12);

        detail = detail.push(
            text(format!("Send from {}", config.name))
                .size(18)
                .font(styles::BOLD),
        );
        detail = detail.push(
            text(address_display)
                .size(12)
                .font(Font::MONOSPACE)
                .color(MUTED),
        );

        detail = detail.push(Space::new().height(4));

        detail = detail.push(
            column![
                text("Recipient").size(12).color(MUTED),
                text_input("Address or .iota name", &self.multisig_send_recipient)
                    .on_input(Message::MultisigSendRecipientChanged)
                    .size(13),
            ]
            .spacing(4),
        );

        detail = detail.push(
            column![
                text("Amount (IOTA)").size(12).color(MUTED),
                text_input("e.g. 1.5", &self.multisig_send_amount)
                    .on_input(Message::MultisigSendAmountChanged)
                    .size(13),
            ]
            .spacing(4),
        );

        detail = detail.push(
            text("This creates a proposal. You'll sign immediately, then share the proposal file with other signers for their signatures.")
                .size(12)
                .color(MUTED),
        );

        if let Some(err) = &self.multisig_send_error {
            detail = detail.push(text(err.as_str()).size(13).color(styles::DANGER));
        }

        detail = detail.push(Space::new().height(4));

        let mut send_btn = button(text("Send").size(13))
            .padding([8, 16])
            .style(styles::btn_primary);
        if self.loading == 0 {
            send_btn = send_btn.on_press(Message::MultisigSendConfirm);
        }

        let cancel_btn = button(text("Cancel").size(13))
            .padding([8, 16])
            .style(styles::btn_ghost)
            .on_press(Message::MultisigCloseSend);

        detail = detail.push(row![send_btn, cancel_btn].spacing(8));

        if self.loading > 0 {
            detail = detail.push(text("Processing...").size(13).color(MUTED));
        }

        let card = container(detail)
            .padding(24)
            .max_width(560)
            .style(styles::card);

        Some(card.into())
    }

    pub(crate) fn view_multisig_sign_modal(&self) -> Option<Element<'_, Message>> {
        if !self.multisig_sign_visible {
            return None;
        }

        // If no proposal file loaded yet, show path input step
        let prop_file = match self.multisig_sign_proposal_file.as_ref() {
            Some(pf) => pf,
            None => {
                let mut detail = column![].spacing(8);
                detail = detail.push(text("Sign Proposal").size(18).font(styles::BOLD));
                detail = detail.push(
                    text("Enter the path to a .jota-proposal file to review and sign.")
                        .size(12)
                        .color(MUTED),
                );
                detail = detail.push(
                    row![
                        text_input(
                            "/path/to/file.jota-proposal",
                            &self.multisig_sign_external_path
                        )
                        .on_input(Message::MultisigSignExternalPathChanged)
                        .on_submit(Message::MultisigSignExternalLoadFile)
                        .size(13),
                        button(text("Browse").size(13))
                            .padding([8, 16])
                            .style(styles::btn_secondary)
                            .on_press(Message::MultisigBrowseSignExternal),
                    ]
                    .spacing(8),
                );
                if let Some(err) = &self.multisig_sign_error {
                    detail = detail.push(text(err.as_str()).size(13).color(styles::DANGER));
                }
                detail = detail.push(
                    row![
                        button(text("Load").size(13))
                            .padding([8, 16])
                            .style(styles::btn_primary)
                            .on_press(Message::MultisigSignExternalLoadFile),
                        button(text("Cancel").size(13))
                            .padding([8, 16])
                            .style(styles::btn_ghost)
                            .on_press(Message::MultisigSignExternalClose),
                    ]
                    .spacing(8),
                );
                let card = container(detail)
                    .padding(24)
                    .max_width(560)
                    .style(styles::card);
                return Some(card.into());
            }
        };

        let mut detail = column![].spacing(8);

        detail = detail.push(text("Sign Proposal").size(18).font(styles::BOLD));

        // Decode and describe the transaction
        if let Ok(tx_bytes) =
            <base64ct::Base64 as base64ct::Encoding>::decode_vec(&prop_file.tx_bytes)
        {
            if let Ok(desc) = jota_core::multisig::describe_transaction(&tx_bytes) {
                let sender_display = truncate_str(&desc.sender, 14, 14);
                detail = detail.push(
                    row![
                        text("From")
                            .size(12)
                            .color(MUTED)
                            .width(Length::Fixed(100.0)),
                        text(sender_display).size(12).font(Font::MONOSPACE),
                    ]
                    .spacing(8),
                );
                if let Some(amount) = desc.amount {
                    detail = detail.push(
                        row![
                            text("Amount")
                                .size(12)
                                .color(MUTED)
                                .width(Length::Fixed(100.0)),
                            text(format_balance(amount)).size(13).font(styles::BOLD),
                        ]
                        .spacing(8),
                    );
                }
                if let Some(ref recipient) = desc.recipient {
                    let recip_display = truncate_str(recipient, 14, 14);
                    detail = detail.push(
                        row![
                            text("Recipient")
                                .size(12)
                                .color(MUTED)
                                .width(Length::Fixed(100.0)),
                            text(recip_display).size(12).font(Font::MONOSPACE),
                        ]
                        .spacing(8),
                    );
                }
                detail = detail.push(
                    row![
                        text("Gas Budget")
                            .size(12)
                            .color(MUTED)
                            .width(Length::Fixed(100.0)),
                        text(format_balance(desc.gas_budget)).size(12),
                    ]
                    .spacing(8),
                );
            }
        }

        detail = detail.push(
            row![
                text("Network")
                    .size(12)
                    .color(MUTED)
                    .width(Length::Fixed(100.0)),
                text(&prop_file.network).size(13),
            ]
            .spacing(8),
        );

        detail = detail.push(Space::new().height(4));

        // Signature checklist
        detail = detail.push(text("Signatures").size(14).font(styles::BOLD));
        for member in &prop_file.multisig.members {
            let signed = prop_file
                .signatures
                .iter()
                .any(|s| s.public_key == member.public_key);
            let (icon, color) = if signed {
                ("  [signed]", styles::ACCENT)
            } else {
                ("  [pending]", MUTED)
            };
            detail = detail.push(
                text(format!("{} (w:{})  {icon}", member.label, member.weight))
                    .size(12)
                    .color(color),
            );
        }

        detail = detail.push(Space::new().height(8));

        if let Some(err) = &self.multisig_sign_error {
            detail = detail.push(text(err.as_str()).size(13).color(styles::DANGER));
        }

        let mut sign_btn = button(text("Sign & Export").size(13))
            .padding([8, 16])
            .style(styles::btn_primary);
        if self.loading == 0 {
            sign_btn = sign_btn.on_press(Message::MultisigSignExternalReviewed);
        }

        let cancel_btn = button(text("Cancel").size(13))
            .padding([8, 16])
            .style(styles::btn_ghost)
            .on_press(Message::MultisigSignExternalClose);

        detail = detail.push(row![sign_btn, cancel_btn].spacing(8));

        if self.loading > 0 {
            detail = detail.push(text("Processing...").size(13).color(MUTED));
        }

        let card = container(detail)
            .padding(24)
            .max_width(560)
            .style(styles::card);

        Some(card.into())
    }
}
