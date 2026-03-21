use base64ct::Encoding;

use crate::messages::Message;
use crate::native_messaging::NativeResponse;
use crate::state::{PendingApproval, Screen, WalletInfo};
use crate::App;
use iced::widget::qr_code;
use iced::Task;
use jota_core::cache::TransactionCache;
use jota_core::display::{parse_iota_amount, parse_token_amount};
use jota_core::network::{
    CoinMeta, NetworkClient, NftSummary, StakedIotaSummary, TokenBalance, TransactionFilter,
    ValidatorSummary,
};
use jota_core::service::WalletService;
use jota_core::wallet::{Network, NetworkConfig, Wallet};
use jota_core::{list_wallets, validate_wallet_name, Contact, ContactStore};
use jota_core::{verify_message, ObjectId, Recipient, SignedMessage};
use std::sync::Arc;
use zeroize::{Zeroize, Zeroizing};

impl App {
    // -- Update --

    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::GoTo(screen) => {
                self.clear_form();
                if screen == Screen::WalletSelect {
                    self.active_multisig = None;
                    self.wallet_entries = list_wallets(&self.wallet_dir);
                    self.wallet_info = None;
                    self.qr_data = None;
                    self.balance = None;
                    self.transactions.clear();
                    self.account_transactions.clear();
                    self.epoch_deltas.clear();
                    self.balance_chart.clear();
                    self.stakes.clear();
                    self.validators.clear();
                    self.nfts.clear();
                    self.multisig_configs.clear();
                    self.multisig_proposals.clear();
                    self.token_balances.clear();
                    self.token_meta.clear();
                    self.session_password.zeroize();
                }
                let load_contacts = screen == Screen::Contacts || screen == Screen::Send;
                let load_stakes = screen == Screen::Staking;
                let load_nfts = screen == Screen::Nfts;
                let load_multisig = screen == Screen::Multisig;
                self.screen = screen;
                if load_contacts {
                    return self.load_contacts();
                }
                if load_stakes {
                    return Task::batch([self.load_stakes(), self.load_validators()]);
                }
                if load_nfts {
                    return self.load_nfts();
                }
                if load_multisig {
                    // Compute public key for display on the multisig page
                    if self.multisig_my_public_key_b64.is_none() {
                        self.multisig_my_public_key_b64 =
                            self.wallet_info.as_ref().and_then(|info| {
                                info.service
                                    .signer()
                                    .public_key_bytes()
                                    .ok()
                                    .map(|bytes| base64ct::Base64::encode_string(&bytes))
                            });
                    }
                    return self.load_multisig();
                }
                Task::none()
            }

            Message::WalletSelected(name) => {
                self.selected_wallet = Some(name);
                self.clear_form();
                self.screen = Screen::Unlock;
                Task::none()
            }

            // Form inputs
            Message::PasswordChanged(v) => {
                self.password = v;
                Task::none()
            }
            Message::PasswordConfirmChanged(v) => {
                self.password_confirm = v;
                Task::none()
            }
            Message::WalletNameChanged(v) => {
                self.wallet_name = v;
                Task::none()
            }
            Message::MnemonicInputChanged(v) => {
                self.mnemonic_input = v;
                Task::none()
            }
            Message::RecipientChanged(v) => {
                self.recipient = v.clone();
                self.resolved_recipient = None;
                // Trigger async resolution for .iota names
                if v.ends_with(".iota") && v.len() > 5 {
                    if let Some(info) = &self.wallet_info {
                        let service = info.service.clone();
                        let name = v;
                        return Task::perform(
                            async move {
                                let r = Recipient::Name(name.to_lowercase());
                                let resolved = service.resolve_recipient(&r).await?;
                                Ok(resolved.address.to_string())
                            },
                            |r: Result<String, anyhow::Error>| {
                                Message::RecipientResolved(r.map_err(|e| e.to_string()))
                            },
                        );
                    }
                }
                Task::none()
            }
            Message::AmountChanged(v) => {
                self.amount = v;
                Task::none()
            }

            // -- Unlock --
            Message::UnlockWallet => {
                let path = self.wallet_path();
                let pw = Zeroizing::new(self.password.as_bytes().to_vec());
                self.session_password = pw.clone();
                self.loading += 1;
                self.error_message = None;

                Task::perform(
                    async move {
                        let wallet = Wallet::open(&path, &pw)?;

                        #[cfg(feature = "ledger")]
                        if wallet.is_hardware() {
                            use jota_core::ledger_signer::connect_and_verify;

                            let bip32_path = jota_core::bip32_path_for(
                                wallet.network_config().network,
                                wallet.account_index() as u32,
                            );

                            let stored_addr = *wallet.address();
                            let signer = tokio::task::spawn_blocking(move || {
                                connect_and_verify(bip32_path, &stored_addr)
                            })
                            .await
                            .map_err(|e| anyhow::anyhow!("Task failed: {e}"))??;

                            return WalletInfo::from_wallet_with_signer(&wallet, Arc::new(signer));
                        }

                        #[cfg(not(feature = "ledger"))]
                        if wallet.is_hardware() {
                            anyhow::bail!("Hardware wallet support not compiled in.");
                        }

                        WalletInfo::from_wallet(&wallet)
                    },
                    |r: Result<WalletInfo, anyhow::Error>| {
                        Message::WalletOpened(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::WalletOpened(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(mut info) => {
                        // Apply the network selected on the welcome screen
                        if info.network_config.network != self.network_config.network {
                            let config = self.network_config.clone();
                            info.network_config = config.clone();
                            info.is_mainnet = config.network == Network::Mainnet;
                            match NetworkClient::new(&config, false) {
                                Ok(client) => {
                                    let signer = info.service.signer().clone();
                                    let service = WalletService::new(client, signer)
                                        .with_notarization_package(
                                            info.notarization_package_config,
                                        );
                                    info.notarization_package = service.notarization_package();
                                    info.service = Arc::new(service);
                                }
                                Err(e) => {
                                    self.error_message =
                                        Some(format!("Failed to switch network: {e}"));
                                    return Task::none();
                                }
                            }
                        }
                        self.qr_data = qr_code::Data::new(&info.address_string).ok();
                        self.wallet_info = Some(info);
                        self.clear_form();
                        self.screen = Screen::Account;
                        let dashboard = self.refresh_dashboard();
                        return self.replay_buffered_native_requests(dashboard);
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Create --
            Message::CreateWallet => {
                if let Some(err) = self.validate_create_form() {
                    self.error_message = Some(err);
                    return Task::none();
                }
                let name = self.wallet_name.clone();
                if let Err(e) = validate_wallet_name(&name) {
                    self.error_message = Some(e.to_string());
                    return Task::none();
                }
                let path = self.wallet_dir.join(format!("{name}.wallet"));
                if path.exists() {
                    self.error_message = Some(format!("Wallet '{name}' already exists"));
                    return Task::none();
                }
                let pw = Zeroizing::new(self.password.as_bytes().to_vec());
                self.session_password = pw.clone();
                let config = self.network_config.clone();
                self.loading += 1;
                self.error_message = None;

                Task::perform(
                    async move {
                        if let Some(parent) = path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        let wallet = Wallet::create_new(path, &pw, config)?;
                        let mnemonic = Zeroizing::new(
                            wallet
                                .mnemonic()
                                .ok_or_else(|| anyhow::anyhow!("Wallet has no mnemonic"))?
                                .to_string(),
                        );
                        let info = WalletInfo::from_wallet(&wallet)?;
                        Ok((info, mnemonic))
                    },
                    |r: Result<(WalletInfo, Zeroizing<String>), anyhow::Error>| {
                        Message::WalletCreated(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::WalletCreated(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok((info, mnemonic)) => {
                        self.selected_wallet = Some(self.wallet_name.clone());
                        self.qr_data = qr_code::Data::new(&info.address_string).ok();
                        self.wallet_info = Some(info);
                        self.created_mnemonic = Some(mnemonic);
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::MnemonicConfirmed => {
                self.created_mnemonic = None;
                self.clear_form();
                self.screen = Screen::Account;
                self.refresh_dashboard()
            }

            // -- Recover --
            Message::RecoverWallet => {
                if let Some(err) = self.validate_create_form() {
                    self.error_message = Some(err);
                    return Task::none();
                }
                if self.mnemonic_input.trim().is_empty() {
                    self.error_message = Some("Mnemonic is required".into());
                    return Task::none();
                }
                let name = self.wallet_name.clone();
                if let Err(e) = validate_wallet_name(&name) {
                    self.error_message = Some(e.to_string());
                    return Task::none();
                }
                let path = self.wallet_dir.join(format!("{name}.wallet"));
                if path.exists() {
                    self.error_message = Some(format!("Wallet '{name}' already exists"));
                    return Task::none();
                }
                let pw = Zeroizing::new(self.password.as_bytes().to_vec());
                self.session_password = pw.clone();
                let mnemonic = Zeroizing::new(self.mnemonic_input.trim().to_string());
                let config = self.network_config.clone();
                self.loading += 1;
                self.error_message = None;

                Task::perform(
                    async move {
                        if let Some(parent) = path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        let wallet = Wallet::recover_from_mnemonic(path, &pw, &mnemonic, config)?;
                        WalletInfo::from_wallet(&wallet)
                    },
                    |r: Result<WalletInfo, anyhow::Error>| {
                        Message::WalletRecovered(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::WalletRecovered(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(info) => {
                        self.selected_wallet = Some(self.wallet_name.clone());
                        self.qr_data = qr_code::Data::new(&info.address_string).ok();
                        self.wallet_info = Some(info);
                        self.clear_form();
                        self.screen = Screen::Account;
                        return self.refresh_dashboard();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Hardware wallet --
            #[cfg(feature = "hardware-wallets")]
            Message::HardwareConnect => {
                if let Some(err) = self.validate_create_form() {
                    self.error_message = Some(err);
                    return Task::none();
                }
                let name = self.wallet_name.clone();
                if let Err(e) = validate_wallet_name(&name) {
                    self.error_message = Some(e.to_string());
                    return Task::none();
                }
                let path = self.wallet_dir.join(format!("{name}.wallet"));
                if path.exists() {
                    self.error_message = Some(format!("Wallet '{name}' already exists"));
                    return Task::none();
                }
                let pw = Zeroizing::new(self.password.as_bytes().to_vec());
                self.session_password = pw.clone();
                let config = self.network_config.clone();
                self.loading += 1;
                self.error_message = None;

                Task::perform(
                    async move {
                        #[cfg(feature = "ledger")]
                        {
                            use jota_core::ledger_signer::connect_with_verification;
                            use jota_core::Signer;

                            let bip32_path = jota_core::bip32_path_for(config.network, 0);

                            let signer = tokio::task::spawn_blocking(move || {
                                connect_with_verification(bip32_path)
                            })
                            .await
                            .map_err(|e| anyhow::anyhow!("Task failed: {e}"))??;

                            let address = *signer.address();
                            if let Some(parent) = path.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            let wallet = Wallet::create_hardware(
                                path,
                                &pw,
                                address,
                                config,
                                jota_core::HardwareKind::Ledger,
                            )?;
                            WalletInfo::from_wallet_with_signer(&wallet, Arc::new(signer))
                        }

                        #[cfg(not(feature = "ledger"))]
                        {
                            anyhow::bail!("No hardware wallet driver compiled in.");
                        }
                    },
                    |r: Result<WalletInfo, anyhow::Error>| {
                        Message::HardwareConnected(r.map_err(|e| e.to_string()))
                    },
                )
            }

            #[cfg(feature = "hardware-wallets")]
            Message::HardwareConnected(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(info) => {
                        self.selected_wallet = Some(self.wallet_name.clone());
                        self.qr_data = qr_code::Data::new(&info.address_string).ok();
                        self.wallet_info = Some(info);
                        self.clear_form();
                        self.screen = Screen::Account;
                        return self.refresh_dashboard();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Hardware wallet verify address --
            #[cfg(feature = "hardware-wallets")]
            Message::HardwareVerifyAddress => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let service = info.service.clone();
                self.loading += 1;
                self.error_message = None;
                self.status_message = None;

                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || service.verify_address())
                            .await
                            .map_err(|e| anyhow::anyhow!("Task failed: {e}"))?
                    },
                    |r: Result<(), _>| {
                        Message::HardwareVerifyAddressCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            #[cfg(feature = "hardware-wallets")]
            Message::HardwareVerifyAddressCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(()) => {
                        self.status_message = Some("Address verified on device".into());
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Hardware wallet reconnect --
            #[cfg(feature = "hardware-wallets")]
            Message::HardwareReconnect => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let service = info.service.clone();
                self.loading += 1;
                self.error_message = None;
                self.status_message = None;

                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || service.reconnect_signer())
                            .await
                            .map_err(|e| anyhow::anyhow!("Task failed: {e}"))?
                    },
                    |r: Result<(), _>| Message::HardwareReconnected(r.map_err(|e| e.to_string())),
                )
            }

            #[cfg(feature = "hardware-wallets")]
            Message::HardwareReconnected(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(()) => {
                        self.error_message = None;
                        self.status_message = Some("Device reconnected".into());
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Dashboard --
            Message::RefreshBalance => self.refresh_dashboard(),

            Message::BalanceUpdated(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(b) => {
                        self.balance = Some(b);
                        self.compute_balance_history();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::RequestFaucet => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let service = info.service.clone();
                let addr = self.active_address().unwrap_or(info.address);
                self.loading += 1;
                self.error_message = None;
                self.success_message = None;

                Task::perform(
                    async move {
                        service.network().faucet(&addr).await?;
                        Ok(())
                    },
                    |r: Result<(), anyhow::Error>| {
                        Message::FaucetCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::FaucetCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(()) => {
                        self.success_message = Some("Faucet tokens requested".into());
                        return self.refresh_dashboard();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::CopyAddress => {
                if self.wallet_info.is_some() {
                    let addr = self.active_address_string();
                    if let Some(cb) = &mut self.clipboard {
                        match cb.set_text(&addr) {
                            Ok(_) => self.status_message = Some("Address copied".into()),
                            Err(e) => self.error_message = Some(format!("Copy failed: {e}")),
                        }
                    } else {
                        self.error_message = Some("Clipboard not available".into());
                    }
                }
                Task::none()
            }

            Message::TransactionsLoaded(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok((txs, total, deltas)) => {
                        if self.history_page == 0 {
                            self.account_transactions = txs.clone();
                        }
                        self.transactions = txs;
                        self.history_total = total;
                        if !deltas.is_empty() {
                            self.epoch_deltas = deltas;
                            self.compute_balance_history();
                        }
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Send --
            Message::RecipientResolved(result) => {
                self.resolved_recipient = Some(result);
                Task::none()
            }

            Message::TokenSelected(option) => {
                self.selected_token = Some(option);
                Task::none()
            }

            Message::TokenBalancesLoaded(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok((balances, meta)) => {
                        self.token_balances = balances;
                        self.token_meta = meta;
                    }
                    Err(e) => {
                        // Non-fatal — token balances are supplementary
                        eprintln!("Failed to load token balances: {e}");
                    }
                }
                Task::none()
            }

            Message::ConfirmSend => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };

                // When multisig is active, create a proposal instead of a direct send
                if let Some(ms_idx) = self.active_multisig {
                    // Only IOTA transfers supported for multisig context sends
                    if self.selected_token.as_ref().is_some_and(|t| !t.is_iota()) {
                        self.error_message =
                            Some("Token transfers not yet supported for multisig.".into());
                        return Task::none();
                    }

                    let recipient_str = self.recipient.trim().to_string();
                    if recipient_str.is_empty() {
                        self.error_message = Some("Recipient is required".into());
                        return Task::none();
                    }
                    let recipient = match Recipient::parse(&recipient_str) {
                        Ok(r) => r,
                        Err(e) => {
                            self.error_message = Some(e.to_string());
                            return Task::none();
                        }
                    };
                    let amount = match parse_iota_amount(&self.amount) {
                        Ok(0) => {
                            self.error_message = Some("Amount must be greater than 0".into());
                            return Task::none();
                        }
                        Ok(a) => a,
                        Err(e) => {
                            self.error_message = Some(e.to_string());
                            return Task::none();
                        }
                    };

                    let Some(config) = self.multisig_configs.get(ms_idx).cloned() else {
                        self.error_message = Some("Multisig config not found.".into());
                        return Task::none();
                    };
                    let service = info.service.clone();
                    self.loading += 1;
                    self.error_message = None;

                    return Task::perform(
                        async move {
                            use anyhow::Context;
                            use jota_core::multisig::*;

                            let resolved = service.resolve_recipient(&recipient).await?;
                            let multisig_addr = config.address();

                            let balance = service.network().balance(&multisig_addr).await?;
                            if balance < amount {
                                anyhow::bail!(
                                    "Insufficient balance: {} available, {} needed.",
                                    jota_core::display::format_balance(balance),
                                    jota_core::display::format_balance(amount),
                                );
                            }

                            let tx = build_transfer(
                                service.network(),
                                &multisig_addr,
                                resolved.address,
                                amount,
                            )
                            .await?;
                            let tx_bytes = jota_core::bcs::to_bytes(&tx)
                                .context("Failed to serialize transaction")?;
                            let tx_digest = compute_tx_digest(&tx_bytes)?;
                            let short_id = &tx_digest[..8.min(tx_digest.len())];

                            let now = timestamp_now();

                            let proposer = config
                                .my_key
                                .as_ref()
                                .and_then(|mk| {
                                    config
                                        .committee
                                        .members()
                                        .iter()
                                        .enumerate()
                                        .find(|(_, m)| m.public_key() == mk)
                                        .and_then(|(i, _)| config.labels.get(i))
                                        .cloned()
                                })
                                .unwrap_or_else(|| "unknown".to_string());

                            let mut proposal = TransactionProposal {
                                tx_digest: tx_digest.clone(),
                                multisig_address: multisig_addr.to_string(),
                                tx_bytes: tx_bytes.clone(),
                                proposer,
                                created_at: now,
                                expires_at: Some(now + 7 * 24 * 3600),
                                signatures: Vec::new(),
                                status: ProposalStatus::Pending,
                            };

                            if config.my_key.is_some() {
                                let (member_key, member_sig) = sign_proposal(
                                    service.network(),
                                    service.signer().as_ref(),
                                    &tx_bytes,
                                )
                                .await?;
                                proposal.signatures.push(CollectedSignature {
                                    member_key,
                                    signature: member_sig,
                                    signed_at: now,
                                });
                                update_proposal_status(&config.committee, &mut proposal);
                            }

                            let store = MultisigStore::open()?;
                            store.save_proposal(&proposal)?;

                            Ok(short_id.to_string())
                        },
                        |r: Result<String, anyhow::Error>| {
                            Message::MultisigSendCompleted(r.map_err(|e| e.to_string()))
                        },
                    );
                }

                let recipient_str = self.recipient.trim().to_string();
                if recipient_str.is_empty() {
                    self.error_message = Some("Recipient is required".into());
                    return Task::none();
                }
                let recipient = match Recipient::parse(&recipient_str) {
                    Ok(r) => r,
                    Err(e) => {
                        self.error_message = Some(e.to_string());
                        return Task::none();
                    }
                };

                let token = self
                    .selected_token
                    .as_ref()
                    .filter(|t| !t.is_iota())
                    .cloned();

                if let Some(token) = token {
                    let amount_str = self.amount.clone();
                    let service = info.service.clone();
                    self.loading += 1;
                    self.error_message = None;

                    Task::perform(
                        async move {
                            let resolved = service.resolve_recipient(&recipient).await?;
                            let meta = service.resolve_coin_type(&token.coin_type).await?;
                            let raw = parse_token_amount(&amount_str, meta.decimals)?;
                            let amount = u64::try_from(raw)
                                .map_err(|_| anyhow::anyhow!("Amount too large"))?;
                            if amount == 0 {
                                anyhow::bail!("Amount must be greater than 0");
                            }
                            let result = service
                                .send_token(resolved.address, &meta.coin_type, amount)
                                .await?;
                            Ok(result.digest)
                        },
                        |r: Result<String, anyhow::Error>| {
                            Message::SendCompleted(r.map_err(|e| e.to_string()))
                        },
                    )
                } else {
                    let amount = match parse_iota_amount(&self.amount) {
                        Ok(0) => {
                            self.error_message = Some("Amount must be greater than 0".into());
                            return Task::none();
                        }
                        Ok(a) => a,
                        Err(e) => {
                            self.error_message = Some(e.to_string());
                            return Task::none();
                        }
                    };
                    let service = info.service.clone();
                    self.loading += 1;
                    self.error_message = None;

                    Task::perform(
                        async move {
                            let resolved = service.resolve_recipient(&recipient).await?;
                            let result = service.send(resolved.address, amount).await?;
                            Ok(result.digest)
                        },
                        |r: Result<String, anyhow::Error>| {
                            Message::SendCompleted(r.map_err(|e| e.to_string()))
                        },
                    )
                }
            }

            Message::SendCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(digest) => {
                        self.success_message = Some(format!("Sent! Digest: {digest}"));

                        // Check if the recipient is a known contact; offer to save if not
                        let sent_addr = self
                            .resolved_recipient
                            .as_ref()
                            .and_then(|r| r.as_ref().ok())
                            .cloned()
                            .unwrap_or_else(|| self.recipient.trim().to_lowercase());
                        self.save_contact_offer = None;
                        if sent_addr.starts_with("0x") {
                            let known = self
                                .contacts
                                .iter()
                                .any(|c| c.address.eq_ignore_ascii_case(&sent_addr));
                            if !known {
                                self.save_contact_offer = Some(sent_addr);
                            }
                        }

                        self.recipient.clear();
                        self.amount.clear();
                        self.screen = Screen::Account;
                        return self.refresh_dashboard();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::ToggleTxDetail(idx) => {
                if self.expanded_tx == Some(idx) {
                    self.expanded_tx = None;
                } else {
                    self.expanded_tx = Some(idx);
                }
                Task::none()
            }

            Message::RefreshHistory => self.refresh_dashboard(),

            Message::HistoryNextPage => {
                self.expanded_tx = None;
                self.history_page += 1;
                self.load_history_page()
            }

            Message::HistoryPrevPage => {
                self.expanded_tx = None;
                self.history_page = self.history_page.saturating_sub(1);
                self.load_history_page()
            }

            Message::OpenExplorer(digest) => {
                let network = self.wallet_info.as_ref().map(|i| &i.network_config.network);
                let query = match network {
                    Some(Network::Mainnet) | None => "",
                    Some(Network::Testnet) => "?network=testnet",
                    Some(Network::Devnet) => "?network=devnet",
                    Some(Network::Custom) => "?network=testnet",
                };
                let url = format!("https://explorer.iota.org/txblock/{digest}{query}");
                let _ = open::that(&url);
                Task::none()
            }

            Message::OpenExplorerAddress(addr) => {
                let network = self.wallet_info.as_ref().map(|i| &i.network_config.network);
                let query = match network {
                    Some(Network::Mainnet) | None => "",
                    Some(Network::Testnet) => "?network=testnet",
                    Some(Network::Devnet) => "?network=devnet",
                    Some(Network::Custom) => "?network=testnet",
                };
                let url = format!("https://explorer.iota.org/address/{addr}{query}");
                let _ = open::that(&url);
                Task::none()
            }

            // -- Contacts --
            Message::ContactsLoaded(result) => {
                match result {
                    Ok(c) => self.contacts = c,
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }
            Message::OpenContactForm => {
                self.contact_form_visible = true;
                self.contact_form_name.clear();
                self.contact_form_address.clear();
                self.contact_form_editing = None;
                self.error_message = None;
                Task::none()
            }
            Message::CloseContactForm => {
                self.contact_form_visible = false;
                self.contact_form_editing = None;
                self.error_message = None;
                Task::none()
            }
            Message::ContactNameChanged(v) => {
                self.contact_form_name = v;
                Task::none()
            }
            Message::ContactAddressChanged(v) => {
                self.contact_form_address = v;
                Task::none()
            }
            Message::EditContact(idx) => {
                if let Some(c) = self.contacts.get(idx) {
                    self.contact_form_name = c.name.clone();
                    self.contact_form_address = c.iota_name.clone().unwrap_or(c.address.clone());
                    self.contact_form_editing = Some(idx);
                    self.contact_form_visible = true;
                    self.error_message = None;
                }
                Task::none()
            }
            Message::SaveContact => {
                let name = self.contact_form_name.trim().to_string();
                let address = self.contact_form_address.trim().to_string();
                if name.is_empty() || address.is_empty() {
                    self.error_message = Some("Name and address are required".into());
                    return Task::none();
                }
                let editing = self.contact_form_editing;
                let service = self.wallet_info.as_ref().map(|i| i.service.clone());

                Task::perform(
                    async move {
                        let mut store = ContactStore::open()?;

                        // If editing, remove the old entry first
                        if let Some(idx) = editing {
                            if let Some(old) = store.list().get(idx) {
                                let old_name = old.name.clone();
                                store.remove(&old_name)?;
                            }
                        }

                        // Resolve .iota names if a service is available
                        let (addr, iota_name) = if address.ends_with(".iota") {
                            if let Some(svc) = service {
                                let r = Recipient::Name(address.to_lowercase());
                                let resolved = svc.resolve_recipient(&r).await?;
                                (resolved.address.to_string(), Some(address))
                            } else {
                                anyhow::bail!(
                                    "Cannot resolve .iota name without a wallet connection."
                                );
                            }
                        } else {
                            (address, None)
                        };

                        store.add(&name, &addr, iota_name.as_deref())?;
                        Ok(())
                    },
                    |r: Result<(), anyhow::Error>| {
                        Message::ContactSaved(r.map_err(|e| e.to_string()))
                    },
                )
            }
            Message::ContactSaved(result) => {
                match result {
                    Ok(()) => {
                        self.contact_form_visible = false;
                        self.contact_form_editing = None;
                        self.success_message = Some("Contact saved".into());
                        return self.load_contacts();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }
            Message::DeleteContact(idx) => {
                let name = match self.contacts.get(idx) {
                    Some(c) => c.name.clone(),
                    None => return Task::none(),
                };

                Task::perform(
                    async move {
                        let mut store = ContactStore::open()?;
                        store.remove(&name)?;
                        Ok(store.list().to_vec())
                    },
                    |r: Result<Vec<Contact>, anyhow::Error>| {
                        Message::ContactDeleted(r.map_err(|e| e.to_string()))
                    },
                )
            }
            Message::ContactDeleted(result) => {
                match result {
                    Ok(contacts) => {
                        self.contacts = contacts;
                        self.success_message = Some("Contact deleted".into());
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }
            Message::SelectContact(address) => {
                // Populate recipient field — if already on Send, just fill it;
                // otherwise this message comes from the contact list button.
                self.recipient = address;
                self.resolved_recipient = None;
                Task::none()
            }
            Message::SaveContactOffer => {
                if let Some(addr) = self.save_contact_offer.take() {
                    self.contact_form_address = addr;
                    self.contact_form_name.clear();
                    self.contact_form_editing = None;
                    self.contact_form_visible = true;
                    self.screen = Screen::Contacts;
                }
                Task::none()
            }
            Message::DismissContactOffer => {
                self.save_contact_offer = None;
                Task::none()
            }

            // -- Staking --
            Message::StakeAmountChanged(v) => {
                self.stake_amount = v;
                Task::none()
            }
            Message::RefreshStakes => Task::batch([self.load_stakes(), self.load_validators()]),

            Message::ConfirmStake => {
                if self.is_multisig_active() {
                    self.error_message =
                        Some("Staking is not available for multisig addresses.".into());
                    return Task::none();
                }
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let validator_str = if let Some(idx) = self.selected_validator {
                    match self.validators.get(idx) {
                        Some(v) => v.address.clone(),
                        None => {
                            self.error_message = Some("Invalid validator selection".into());
                            return Task::none();
                        }
                    }
                } else {
                    self.validator_address.trim().to_string()
                };
                if validator_str.is_empty() {
                    self.error_message = Some("Validator is required".into());
                    return Task::none();
                }
                let validator = match Recipient::parse(&validator_str) {
                    Ok(r) => r,
                    Err(e) => {
                        self.error_message = Some(e.to_string());
                        return Task::none();
                    }
                };
                let amount = match parse_iota_amount(&self.stake_amount) {
                    Ok(0) => {
                        self.error_message = Some("Amount must be greater than 0".into());
                        return Task::none();
                    }
                    Ok(a) => a,
                    Err(e) => {
                        self.error_message = Some(e.to_string());
                        return Task::none();
                    }
                };
                let service = info.service.clone();
                self.loading += 1;
                self.error_message = None;

                Task::perform(
                    async move {
                        let resolved = service.resolve_recipient(&validator).await?;
                        let result = service.stake(resolved.address, amount).await?;
                        Ok(result.digest)
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::StakeCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::StakeCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(digest) => {
                        self.success_message = Some(format!("Staked! Digest: {digest}"));
                        self.validator_address.clear();
                        self.stake_amount.clear();
                        self.selected_validator = None;
                        return self.load_stakes();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::ConfirmUnstake(object_id_str) => {
                if self.is_multisig_active() {
                    self.error_message =
                        Some("Unstaking is not available for multisig addresses.".into());
                    return Task::none();
                }
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let service = info.service.clone();
                self.loading += 1;
                self.error_message = None;
                self.success_message = None;

                Task::perform(
                    async move {
                        let object_id = ObjectId::from_hex(&object_id_str)
                            .map_err(|e| anyhow::anyhow!("Invalid object ID: {e}"))?;
                        let result = service.unstake(object_id).await?;
                        Ok(result.digest)
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::UnstakeCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::UnstakeCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(digest) => {
                        self.success_message = Some(format!("Unstaked! Digest: {digest}"));
                        return self.load_stakes();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::StakesLoaded(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(mut s) => {
                        Self::resolve_validator_names(&mut s, &self.validators);
                        self.stakes = s;
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::ValidatorsLoaded(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(v) => {
                        self.validators = v;
                        Self::resolve_validator_names(&mut self.stakes, &self.validators);
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::SelectValidator(idx) => {
                if self.selected_validator == Some(idx) {
                    self.selected_validator = None;
                } else {
                    self.selected_validator = Some(idx);
                    self.stake_amount.clear();
                }
                Task::none()
            }

            // -- NFTs --
            Message::NftsLoaded(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(nfts) => self.nfts = nfts,
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::RefreshNfts => self.load_nfts(),

            Message::SendNftSelected(object_id) => {
                self.send_nft_object_id = Some(object_id);
                self.send_nft_recipient.clear();
                self.error_message = None;
                self.success_message = None;
                Task::none()
            }

            Message::SendNftRecipientChanged(v) => {
                self.send_nft_recipient = v;
                Task::none()
            }

            Message::ConfirmSendNft => {
                if self.is_multisig_active() {
                    self.error_message =
                        Some("NFT transfers are not available for multisig addresses.".into());
                    return Task::none();
                }
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let Some(object_id_str) = &self.send_nft_object_id else {
                    return Task::none();
                };
                let recipient_str = self.send_nft_recipient.trim().to_string();
                if recipient_str.is_empty() {
                    self.error_message = Some("Recipient is required".into());
                    return Task::none();
                }
                let recipient = match Recipient::parse(&recipient_str) {
                    Ok(r) => r,
                    Err(e) => {
                        self.error_message = Some(e.to_string());
                        return Task::none();
                    }
                };
                let object_id = match ObjectId::from_hex(object_id_str) {
                    Ok(id) => id,
                    Err(e) => {
                        self.error_message = Some(format!("Invalid object ID: {e}"));
                        return Task::none();
                    }
                };
                let service = info.service.clone();
                self.loading += 1;
                self.error_message = None;

                Task::perform(
                    async move {
                        let resolved = service.resolve_recipient(&recipient).await?;
                        let result = service.send_nft(object_id, resolved.address).await?;
                        Ok(result.digest)
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::SendNftCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::SendNftCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(digest) => {
                        self.success_message = Some(format!("NFT sent! Digest: {digest}"));
                        self.send_nft_object_id = None;
                        self.send_nft_recipient.clear();
                        return self.load_nfts();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::CancelSendNft => {
                self.send_nft_object_id = None;
                self.send_nft_recipient.clear();
                self.error_message = None;
                Task::none()
            }

            // -- Multisig --
            Message::MultisigLoaded(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok((configs, proposals)) => {
                        // Deactivate if the active index is now out of bounds
                        if let Some(idx) = self.active_multisig {
                            if idx >= configs.len() {
                                self.active_multisig = None;
                            }
                        }
                        self.multisig_configs = configs;
                        self.multisig_proposals = proposals;
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::RefreshMultisig => self.load_multisig(),

            Message::MultisigSelectConfig(idx) => {
                self.multisig_selected = Some(idx);
                Task::none()
            }

            Message::MultisigCloseDetail => {
                self.multisig_selected = None;
                Task::none()
            }

            Message::MultisigSelectProposal(idx) => {
                self.multisig_proposal_selected = Some(idx);
                Task::none()
            }

            Message::MultisigCloseProposal => {
                self.multisig_proposal_selected = None;
                Task::none()
            }

            Message::MultisigRemoveConfig(name) => {
                self.loading += 1;
                Task::perform(
                    async move {
                        let store = jota_core::multisig::MultisigStore::open()?;
                        store.remove_config(&name)?;
                        Ok(())
                    },
                    |r: Result<(), anyhow::Error>| {
                        Message::MultisigConfigRemoved(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::MultisigConfigRemoved(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(()) => {
                        self.success_message = Some("Multisig config removed.".into());
                        self.multisig_selected = None;
                        // Deactivate if the removed config was active
                        self.active_multisig = None;
                        return self.load_multisig();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::MultisigActivate(idx) => {
                self.active_multisig = Some(idx);
                self.multisig_selected = None;
                self.screen = Screen::Account;
                self.balance = None;
                self.transactions.clear();
                self.account_transactions.clear();
                self.epoch_deltas.clear();
                self.balance_chart.clear();
                self.stakes.clear();
                self.nfts.clear();
                self.token_balances.clear();
                self.token_meta.clear();
                self.selected_token = None;
                self.qr_data = iced::widget::qr_code::Data::new(self.active_address_string()).ok();
                self.refresh_dashboard()
            }

            Message::MultisigDeactivate => {
                self.active_multisig = None;
                self.balance = None;
                self.transactions.clear();
                self.account_transactions.clear();
                self.epoch_deltas.clear();
                self.balance_chart.clear();
                self.stakes.clear();
                self.nfts.clear();
                self.token_balances.clear();
                self.token_meta.clear();
                self.selected_token = None;
                if let Some(info) = &self.wallet_info {
                    self.qr_data = iced::widget::qr_code::Data::new(&info.address_string).ok();
                }
                self.screen = Screen::Account;
                self.refresh_dashboard()
            }

            Message::MultisigSubmitProposal(digest) => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                self.loading += 1;
                let service = info.service.clone();
                let configs = self.multisig_configs.clone();

                Task::perform(
                    async move {
                        let store = jota_core::multisig::MultisigStore::open()?;
                        let mut proposal = store.find_proposal_by_prefix(&digest)?;
                        let config = configs
                            .iter()
                            .find(|c| {
                                c.address().to_string().to_lowercase()
                                    == proposal.multisig_address.to_lowercase()
                            })
                            .ok_or_else(|| {
                                anyhow::anyhow!("No multisig config found for this proposal")
                            })?;
                        let result = jota_core::multisig::aggregate_and_submit(
                            service.network(),
                            &config.committee,
                            &mut proposal,
                        )
                        .await?;
                        store.save_proposal(&proposal)?;
                        Ok(format!("Submitted! Digest: {}", result.digest))
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::MultisigSubmitCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::MultisigSubmitCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(msg) => {
                        self.success_message = Some(msg);
                        return self.load_multisig();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::MultisigCancelProposal(digest) => {
                self.loading += 1;
                Task::perform(
                    async move {
                        let store = jota_core::multisig::MultisigStore::open()?;
                        let mut proposal = store.find_proposal_by_prefix(&digest)?;
                        proposal.status = jota_core::multisig::ProposalStatus::Cancelled;
                        store.save_proposal(&proposal)?;
                        Ok(())
                    },
                    |r: Result<(), anyhow::Error>| {
                        Message::MultisigProposalCancelled(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::MultisigProposalCancelled(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(()) => {
                        self.success_message = Some("Proposal cancelled.".into());
                        self.multisig_proposal_selected = None;
                        return self.load_multisig();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Multisig Import --
            Message::MultisigImportFile => {
                self.multisig_import_visible = !self.multisig_import_visible;
                if !self.multisig_import_visible {
                    self.multisig_import_path.clear();
                }
                Task::none()
            }

            Message::MultisigImportPathChanged(v) => {
                self.multisig_import_path = v;
                Task::none()
            }

            Message::MultisigCloseImport => {
                self.multisig_import_visible = false;
                self.multisig_import_path.clear();
                Task::none()
            }

            Message::MultisigImportConfirm => {
                let path = self.multisig_import_path.trim().to_string();
                if path.is_empty() {
                    self.error_message = Some("Please enter a file path.".into());
                    return Task::none();
                }
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let service = info.service.clone();
                let network = info.network_config.network;
                self.loading += 1;
                self.error_message = None;

                Task::perform(
                    async move {
                        use anyhow::Context;

                        let data = std::fs::read(&path)
                            .map_err(|e| anyhow::anyhow!("Failed to read file: {e}"))?;
                        let json = String::from_utf8(data)
                            .map_err(|_| anyhow::anyhow!("File is not valid UTF-8"))?;
                        let ms_file: jota_core::multisig::MultisigFile =
                            serde_json::from_str(&json).context("Failed to parse multisig file")?;
                        let network_name = service.network_name();
                        ms_file.validate(network_name)?;
                        let committee = ms_file.to_committee()?;
                        let labels: Vec<String> =
                            ms_file.members.iter().map(|m| m.label.clone()).collect();
                        let filename = std::path::Path::new(&path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(&path);
                        let name = filename.trim_end_matches(".jota-multisig").to_string();
                        let mut config = jota_core::multisig::MultisigConfig {
                            name: name.clone(),
                            committee,
                            labels,
                            network,
                            my_key: None,
                        };
                        if let Ok(pk_bytes) = service.signer().public_key_bytes() {
                            config.detect_my_key(&pk_bytes);
                        }
                        let store = jota_core::multisig::MultisigStore::open()?;
                        store.save_config(&config)?;
                        Ok(format!("Imported: {name}"))
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::MultisigImportCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::MultisigImportCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(msg) => {
                        self.multisig_import_visible = false;
                        self.multisig_import_path.clear();
                        self.success_message = Some(msg);
                        return self.load_multisig();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Multisig Create Wizard --
            Message::MultisigOpenCreate => {
                self.multisig_create_visible = true;
                self.multisig_create_step = 0;
                self.multisig_create_name.clear();
                self.multisig_create_num_participants = "2".to_string();
                self.multisig_create_threshold.clear();
                self.multisig_create_members.clear();
                self.multisig_create_my_weight = "1".to_string();
                self.multisig_create_my_label = "me".to_string();
                self.multisig_create_error = None;
                self.error_message = None;
                self.success_message = None;
                // Compute local public key for display
                self.multisig_my_public_key_b64 = self.wallet_info.as_ref().and_then(|info| {
                    info.service
                        .signer()
                        .public_key_bytes()
                        .ok()
                        .map(|bytes| base64ct::Base64::encode_string(&bytes))
                });
                Task::none()
            }

            Message::MultisigCloseCreate => {
                self.multisig_create_visible = false;
                self.multisig_create_error = None;
                Task::none()
            }

            Message::MultisigCreateNameChanged(v) => {
                self.multisig_create_name = v;
                Task::none()
            }
            Message::MultisigCreateNumParticipantsChanged(v) => {
                self.multisig_create_num_participants = v;
                Task::none()
            }
            Message::MultisigCreateThresholdChanged(v) => {
                self.multisig_create_threshold = v;
                Task::none()
            }
            Message::MultisigCreateMyWeightChanged(v) => {
                self.multisig_create_my_weight = v;
                Task::none()
            }
            Message::MultisigCreateMyLabelChanged(v) => {
                self.multisig_create_my_label = v;
                Task::none()
            }
            Message::MultisigCreateMemberLabelChanged(idx, v) => {
                if let Some(m) = self.multisig_create_members.get_mut(idx) {
                    m.label = v;
                }
                Task::none()
            }
            Message::MultisigCreateMemberKeyChanged(idx, v) => {
                if let Some(m) = self.multisig_create_members.get_mut(idx) {
                    m.public_key = v;
                }
                Task::none()
            }
            Message::MultisigCreateMemberWeightChanged(idx, v) => {
                if let Some(m) = self.multisig_create_members.get_mut(idx) {
                    m.weight = v;
                }
                Task::none()
            }

            Message::MultisigCreateMemberSchemeChanged(idx, v) => {
                if let Some(m) = self.multisig_create_members.get_mut(idx) {
                    m.scheme = v;
                }
                Task::none()
            }

            Message::MultisigCreateNextStep => {
                self.multisig_create_error = None;

                match self.multisig_create_step {
                    0 => {
                        // Validate step 0: name, num participants, threshold
                        let name = self.multisig_create_name.trim();
                        if name.is_empty() {
                            self.multisig_create_error = Some("Name is required.".into());
                            return Task::none();
                        }
                        if let Err(e) = validate_wallet_name(name) {
                            self.multisig_create_error = Some(e.to_string());
                            return Task::none();
                        }

                        let n: usize = match self.multisig_create_num_participants.trim().parse() {
                            Ok(n) if (2..=10).contains(&n) => n,
                            _ => {
                                self.multisig_create_error =
                                    Some("Participants must be between 2 and 10.".into());
                                return Task::none();
                            }
                        };

                        let threshold_str = self.multisig_create_threshold.trim();
                        let threshold: u16 = if threshold_str.is_empty() {
                            n as u16
                        } else {
                            match threshold_str.parse::<u16>() {
                                Ok(t) if t >= 1 && (t as usize) <= n => t,
                                _ => {
                                    self.multisig_create_error =
                                        Some(format!("Threshold must be between 1 and {n}."));
                                    return Task::none();
                                }
                            }
                        };
                        // Update threshold display to show what will be used
                        self.multisig_create_threshold = threshold.to_string();

                        // Pre-populate member forms for remote participants (n - 1)
                        let remote_count = n - 1;
                        self.multisig_create_members.resize_with(remote_count, || {
                            crate::MultisigMemberForm {
                                label: String::new(),
                                public_key: String::new(),
                                weight: "1".to_string(),
                                scheme: String::new(),
                            }
                        });
                        self.multisig_create_members.truncate(remote_count);

                        self.multisig_create_step = 1;
                    }
                    1 => {
                        // Validate step 1: all member forms
                        for (i, m) in self.multisig_create_members.iter().enumerate() {
                            if m.label.trim().is_empty() {
                                self.multisig_create_error =
                                    Some(format!("Label for participant {} is required.", i + 2));
                                return Task::none();
                            }
                            let pk_str = m.public_key.trim();
                            if pk_str.is_empty() {
                                self.multisig_create_error = Some(format!(
                                    "Public key for participant {} is required.",
                                    i + 2
                                ));
                                return Task::none();
                            }
                            match base64ct::Base64::decode_vec(pk_str) {
                                Ok(bytes) if bytes.len() == 32 || bytes.len() == 33 => {}
                                _ => {
                                    self.multisig_create_error = Some(format!(
                                        "Invalid public key for participant {}. Provide base64-encoded key (32 or 33 bytes).",
                                        i + 2
                                    ));
                                    return Task::none();
                                }
                            }
                            let w_str = m.weight.trim();
                            if !w_str.is_empty() {
                                match w_str.parse::<u8>() {
                                    Ok(w) if w >= 1 => {}
                                    _ => {
                                        self.multisig_create_error = Some(format!(
                                            "Weight for participant {} must be a positive number.",
                                            i + 2
                                        ));
                                        return Task::none();
                                    }
                                }
                            }
                        }

                        self.multisig_create_step = 2;
                    }
                    _ => {}
                }
                Task::none()
            }

            Message::MultisigCreatePrevStep => {
                self.multisig_create_error = None;
                if self.multisig_create_step > 0 {
                    self.multisig_create_step -= 1;
                }
                Task::none()
            }

            Message::MultisigCreateConfirm => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                self.loading += 1;
                self.multisig_create_error = None;

                let name = self.multisig_create_name.trim().to_string();
                let threshold: u16 = self.multisig_create_threshold.parse().unwrap_or(2);
                let my_label = self.multisig_create_my_label.trim().to_string();
                let my_weight: u8 = self.multisig_create_my_weight.trim().parse().unwrap_or(1);
                let members_data = self.multisig_create_members.clone();
                let service = info.service.clone();
                let network = info.network_config.network;

                Task::perform(
                    async move {
                        use anyhow::Context;
                        use base64ct::{Base64, Encoding};
                        use jota_core::{
                            Ed25519PublicKey, MultisigCommittee, MultisigMember,
                            MultisigMemberPublicKey, Secp256k1PublicKey,
                        };

                        let pk_bytes = service.signer().public_key_bytes()?;
                        let local_pk = MultisigMemberPublicKey::Ed25519(Ed25519PublicKey::new(
                            pk_bytes
                                .as_slice()
                                .try_into()
                                .context("Invalid public key length")?,
                        ));

                        let mut members = vec![MultisigMember::new(local_pk.clone(), my_weight)];
                        let mut labels = vec![my_label];

                        for m in &members_data {
                            let decoded = Base64::decode_vec(m.public_key.trim())
                                .context("Invalid base64 public key")?;
                            let pk = match decoded.len() {
                                32 => MultisigMemberPublicKey::Ed25519(Ed25519PublicKey::new(
                                    decoded.try_into().unwrap(),
                                )),
                                33 => match m.scheme.to_lowercase().as_str() {
                                    "secp256r1" | "r1" => MultisigMemberPublicKey::Secp256r1(
                                        jota_core::Secp256r1PublicKey::new(
                                            decoded.try_into().unwrap(),
                                        ),
                                    ),
                                    _ => MultisigMemberPublicKey::Secp256k1(
                                        Secp256k1PublicKey::new(decoded.try_into().unwrap()),
                                    ),
                                },
                                _ => {
                                    anyhow::bail!(
                                        "Invalid public key length for '{}'. Expected 32 (Ed25519) or 33 (Secp256k1/r1) bytes.",
                                        m.label
                                    );
                                }
                            };
                            let w: u8 = m.weight.trim().parse().unwrap_or(1);
                            members.push(MultisigMember::new(pk, w));
                            labels.push(m.label.trim().to_string());
                        }

                        let committee = MultisigCommittee::new(members, threshold);
                        if !committee.is_valid() {
                            anyhow::bail!(
                                "Invalid committee: check that total weight >= threshold and no duplicate keys."
                            );
                        }

                        let addr = committee.derive_address();
                        let config = jota_core::multisig::MultisigConfig {
                            name: name.clone(),
                            committee,
                            labels,
                            network,
                            my_key: Some(local_pk),
                        };
                        let store = jota_core::multisig::MultisigStore::open()?;
                        store.save_config(&config)?;
                        Ok(format!("Created: {} ({})", name, addr))
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::MultisigCreateCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::MultisigCreateCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(msg) => {
                        self.multisig_create_visible = false;
                        self.success_message = Some(msg);
                        return self.load_multisig();
                    }
                    Err(e) => self.multisig_create_error = Some(e),
                }
                Task::none()
            }

            Message::MultisigCopyPublicKey => {
                if let Some(pk) = &self.multisig_my_public_key_b64 {
                    if let Some(cb) = &mut self.clipboard {
                        match cb.set_text(pk) {
                            Ok(_) => self.status_message = Some("Public key copied".into()),
                            Err(e) => self.error_message = Some(format!("Copy failed: {e}")),
                        }
                    }
                }
                Task::none()
            }

            Message::MultisigCopyAddress(addr) => {
                if let Some(cb) = &mut self.clipboard {
                    match cb.set_text(&addr) {
                        Ok(_) => self.status_message = Some("Address copied".into()),
                        Err(e) => self.error_message = Some(format!("Copy failed: {e}")),
                    }
                }
                Task::none()
            }

            // -- Multisig Export Config --
            Message::MultisigExportConfig(idx) => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let Some(config) = self.multisig_configs.get(idx).cloned() else {
                    return Task::none();
                };
                let network_name = info.service.network_name().to_string();

                Task::perform(
                    async move {
                        use jota_core::multisig::formats::MemberEntry;

                        let ms_file = jota_core::multisig::MultisigFile {
                            version: 1,
                            file_type: "multisig-address".to_string(),
                            network: network_name,
                            members: config
                                .committee
                                .members()
                                .iter()
                                .enumerate()
                                .map(|(i, m)| MemberEntry {
                                    public_key: m.public_key().clone(),
                                    weight: m.weight(),
                                    label: config.labels.get(i).cloned().unwrap_or_default(),
                                })
                                .collect(),
                            threshold: config.committee.threshold(),
                        };
                        let json = serde_json::to_string_pretty(&ms_file)
                            .map_err(|e| anyhow::anyhow!("Failed to serialize: {e}"))?;

                        let default_name = format!("{}.jota-multisig", config.name);

                        let handle = rfd::AsyncFileDialog::new()
                            .add_filter("Jota Multisig", &["jota-multisig"])
                            .set_file_name(&default_name)
                            .set_title("Export Multisig")
                            .save_file()
                            .await;

                        let path = match handle {
                            Some(file) => file.path().to_path_buf(),
                            None => return Ok("Export cancelled.".to_string()),
                        };
                        std::fs::write(&path, &json)
                            .map_err(|e| anyhow::anyhow!("Failed to write: {e}"))?;
                        Ok(format!("Exported: {}", path.display()))
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::MultisigExportSaved(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::MultisigExportSaved(result) => {
                match result {
                    Ok(msg) => {
                        self.success_message = Some(msg);
                        self.multisig_selected = None;
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Multisig Send --
            Message::MultisigOpenSend(idx) => {
                self.multisig_send_visible = true;
                self.multisig_send_config_idx = Some(idx);
                self.multisig_send_recipient.clear();
                self.multisig_send_amount.clear();
                self.multisig_send_error = None;
                self.multisig_selected = None;
                Task::none()
            }

            Message::MultisigCloseSend => {
                self.multisig_send_visible = false;
                self.multisig_send_config_idx = None;
                self.multisig_send_recipient.clear();
                self.multisig_send_amount.clear();
                self.multisig_send_error = None;
                Task::none()
            }

            Message::MultisigSendRecipientChanged(v) => {
                self.multisig_send_recipient = v;
                Task::none()
            }

            Message::MultisigSendAmountChanged(v) => {
                self.multisig_send_amount = v;
                Task::none()
            }

            Message::MultisigSendConfirm => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let Some(idx) = self.multisig_send_config_idx else {
                    return Task::none();
                };
                let Some(config) = self.multisig_configs.get(idx).cloned() else {
                    return Task::none();
                };

                let recipient_str = self.multisig_send_recipient.trim().to_string();
                if recipient_str.is_empty() {
                    self.multisig_send_error = Some("Recipient is required".into());
                    return Task::none();
                }
                let recipient = match Recipient::parse(&recipient_str) {
                    Ok(r) => r,
                    Err(e) => {
                        self.multisig_send_error = Some(e.to_string());
                        return Task::none();
                    }
                };
                let amount = match parse_iota_amount(&self.multisig_send_amount) {
                    Ok(0) => {
                        self.multisig_send_error = Some("Amount must be greater than 0".into());
                        return Task::none();
                    }
                    Ok(a) => a,
                    Err(e) => {
                        self.multisig_send_error = Some(e.to_string());
                        return Task::none();
                    }
                };

                let service = info.service.clone();
                self.loading += 1;
                self.multisig_send_error = None;

                Task::perform(
                    async move {
                        use anyhow::Context;
                        use jota_core::multisig::*;

                        let resolved = service.resolve_recipient(&recipient).await?;
                        let multisig_addr = config.address();

                        let balance = service.network().balance(&multisig_addr).await?;
                        if balance < amount {
                            anyhow::bail!(
                                "Insufficient balance: {} available, {} needed.",
                                jota_core::display::format_balance(balance),
                                jota_core::display::format_balance(amount),
                            );
                        }

                        let tx = build_transfer(
                            service.network(),
                            &multisig_addr,
                            resolved.address,
                            amount,
                        )
                        .await?;
                        let tx_bytes = jota_core::bcs::to_bytes(&tx)
                            .context("Failed to serialize transaction")?;
                        let tx_digest = compute_tx_digest(&tx_bytes)?;
                        let short_id = &tx_digest[..8.min(tx_digest.len())];

                        let now = timestamp_now();

                        let proposer = config
                            .my_key
                            .as_ref()
                            .and_then(|mk| {
                                config
                                    .committee
                                    .members()
                                    .iter()
                                    .enumerate()
                                    .find(|(_, m)| m.public_key() == mk)
                                    .and_then(|(i, _)| config.labels.get(i))
                                    .cloned()
                            })
                            .unwrap_or_else(|| "unknown".to_string());

                        let mut proposal = jota_core::multisig::TransactionProposal {
                            tx_digest: tx_digest.clone(),
                            multisig_address: multisig_addr.to_string(),
                            tx_bytes: tx_bytes.clone(),
                            proposer,
                            created_at: now,
                            expires_at: Some(now + 7 * 24 * 3600),
                            signatures: Vec::new(),
                            status: jota_core::multisig::ProposalStatus::Pending,
                        };

                        if config.my_key.is_some() {
                            let (member_key, member_sig) = sign_proposal(
                                service.network(),
                                service.signer().as_ref(),
                                &tx_bytes,
                            )
                            .await?;
                            proposal
                                .signatures
                                .push(jota_core::multisig::CollectedSignature {
                                    member_key,
                                    signature: member_sig,
                                    signed_at: now,
                                });
                            update_proposal_status(&config.committee, &mut proposal);
                        }

                        let store = MultisigStore::open()?;
                        store.save_proposal(&proposal)?;

                        Ok(short_id.to_string())
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::MultisigSendCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::MultisigSendCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(short_id) => {
                        self.multisig_send_visible = false;
                        self.multisig_send_config_idx = None;
                        self.multisig_send_recipient.clear();
                        self.multisig_send_amount.clear();
                        self.multisig_send_error = None;
                        // Also clear regular send form (used when multisig is active)
                        self.recipient.clear();
                        self.amount.clear();
                        self.success_message = Some(format!("Proposal created: {short_id}"));
                        if self.is_multisig_active() {
                            self.screen = Screen::Account;
                            return self.refresh_dashboard();
                        }
                        return self.load_multisig();
                    }
                    Err(e) => {
                        if self.is_multisig_active() && self.screen == Screen::Send {
                            self.error_message = Some(e);
                        } else {
                            self.multisig_send_error = Some(e);
                        }
                    }
                }
                Task::none()
            }

            Message::MultisigExportProposal(digest) => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let service = info.service.clone();
                let configs = self.multisig_configs.clone();

                Task::perform(
                    async move {
                        use base64ct::Encoding;
                        use jota_core::multisig::formats::{MemberEntry, SignatureEntry};

                        let store = jota_core::multisig::MultisigStore::open()?;
                        let proposal = store.find_proposal_by_prefix(&digest)?;
                        let config = configs
                            .iter()
                            .find(|c| {
                                c.address().to_string().to_lowercase()
                                    == proposal.multisig_address.to_lowercase()
                            })
                            .ok_or_else(|| anyhow::anyhow!("No config found for this proposal"))?;
                        let network_name = service.network_name().to_string();

                        let proposal_file = jota_core::multisig::ProposalFile {
                            version: 1,
                            file_type: "proposal".to_string(),
                            network: network_name.clone(),
                            multisig: jota_core::multisig::MultisigFile {
                                version: 1,
                                file_type: "multisig-address".to_string(),
                                network: network_name,
                                members: config
                                    .committee
                                    .members()
                                    .iter()
                                    .enumerate()
                                    .map(|(i, m)| MemberEntry {
                                        public_key: m.public_key().clone(),
                                        weight: m.weight(),
                                        label: config.labels.get(i).cloned().unwrap_or_default(),
                                    })
                                    .collect(),
                                threshold: config.committee.threshold(),
                            },
                            tx_bytes: base64ct::Base64::encode_string(&proposal.tx_bytes),
                            proposer: proposal.proposer.clone(),
                            created_at: jota_core::multisig::format_timestamp(proposal.created_at),
                            signatures: proposal
                                .signatures
                                .iter()
                                .map(|s| SignatureEntry {
                                    public_key: s.member_key.clone(),
                                    signature: s.signature.clone(),
                                    signed_at: jota_core::multisig::format_timestamp(s.signed_at),
                                })
                                .collect(),
                        };

                        let json = serde_json::to_string_pretty(&proposal_file)
                            .map_err(|e| anyhow::anyhow!("Failed to serialize: {e}"))?;

                        let short_id = &proposal.tx_digest[..8.min(proposal.tx_digest.len())];
                        let default_name = format!("{short_id}.jota-proposal");

                        let handle = rfd::AsyncFileDialog::new()
                            .add_filter("Jota Proposal", &["jota-proposal"])
                            .set_file_name(&default_name)
                            .set_title("Export Proposal")
                            .save_file()
                            .await;

                        let path = match handle {
                            Some(file) => file.path().to_path_buf(),
                            None => return Ok("Export cancelled.".to_string()),
                        };
                        std::fs::write(&path, &json)
                            .map_err(|e| anyhow::anyhow!("Failed to write: {e}"))?;
                        Ok(format!("Exported: {}", path.display()))
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::MultisigProposalExported(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::MultisigProposalExported(result) => {
                match result {
                    Ok(msg) => self.success_message = Some(msg),
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Multisig Sign External --
            Message::MultisigSignExternalOpen => {
                self.multisig_sign_visible = true;
                self.multisig_sign_proposal_file = None;
                self.multisig_sign_error = None;
                self.multisig_sign_external_path.clear();
                Task::none()
            }

            Message::MultisigSignExternalPathChanged(v) => {
                self.multisig_sign_external_path = v;
                Task::none()
            }

            Message::MultisigSignExternalLoadFile => {
                let path = self.multisig_sign_external_path.trim().to_string();
                if path.is_empty() {
                    self.multisig_sign_error = Some("Please enter a file path.".into());
                    return Task::none();
                }
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let network_name = info.service.network_name().to_string();

                match std::fs::read(&path) {
                    Ok(data) => match String::from_utf8(data) {
                        Ok(json) => {
                            match serde_json::from_str::<jota_core::multisig::ProposalFile>(&json) {
                                Ok(prop_file) => {
                                    if let Err(e) = prop_file.validate(&network_name) {
                                        self.multisig_sign_error =
                                            Some(format!("Invalid proposal file: {e}"));
                                    } else {
                                        self.multisig_sign_proposal_file = Some(prop_file);
                                        self.multisig_sign_error = None;
                                    }
                                }
                                Err(e) => {
                                    self.multisig_sign_error =
                                        Some(format!("Failed to parse proposal: {e}"));
                                }
                            }
                        }
                        Err(_) => {
                            self.multisig_sign_error = Some("File is not valid UTF-8".into());
                        }
                    },
                    Err(e) => {
                        self.multisig_sign_error = Some(format!("Failed to read file: {e}"));
                    }
                }
                Task::none()
            }

            Message::MultisigSignExternalReviewed => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let Some(prop_file) = self.multisig_sign_proposal_file.clone() else {
                    return Task::none();
                };
                let service = info.service.clone();
                self.loading += 1;
                self.multisig_sign_error = None;

                Task::perform(
                    async move {
                        use anyhow::Context;
                        use base64ct::Encoding;
                        use jota_core::multisig::*;

                        let tx_bytes = base64ct::Base64::decode_vec(&prop_file.tx_bytes)
                            .context("Invalid base64 in proposal tx_bytes")?;

                        // Verify we're a member of this committee
                        let pk_bytes = service.signer().public_key_bytes()?;
                        let local_pk = jota_core::MultisigMemberPublicKey::Ed25519(
                            jota_core::Ed25519PublicKey::new(
                                pk_bytes
                                    .as_slice()
                                    .try_into()
                                    .map_err(|_| anyhow::anyhow!("Invalid public key length"))?,
                            ),
                        );
                        if !prop_file
                            .multisig
                            .members
                            .iter()
                            .any(|m| m.public_key == local_pk)
                        {
                            anyhow::bail!("Your key is not a member of this multisig committee.");
                        }

                        let (member_key, member_sig) =
                            sign_proposal(service.network(), service.signer().as_ref(), &tx_bytes)
                                .await?;

                        // Find our label in the proposal's member list
                        let our_label = prop_file
                            .multisig
                            .members
                            .iter()
                            .find(|m| m.public_key == member_key)
                            .map(|m| m.label.clone())
                            .unwrap_or_else(|| "unknown".to_string());

                        let tx_digest = compute_tx_digest(&tx_bytes)?;
                        let short_id = &tx_digest[..8.min(tx_digest.len())];

                        let multisig_addr = prop_file.multisig.to_committee()?.derive_address();

                        let sig_file = jota_core::multisig::SignatureFile {
                            version: 1,
                            file_type: "signature".to_string(),
                            multisig_address: multisig_addr.to_string(),
                            tx_digest: tx_digest.clone(),
                            public_key: member_key,
                            signature: member_sig,
                            signed_at: format_timestamp(timestamp_now()),
                        };

                        let json = serde_json::to_string_pretty(&sig_file)
                            .map_err(|e| anyhow::anyhow!("Failed to serialize: {e}"))?;

                        let our_label: String = our_label
                            .chars()
                            .filter(|c| *c != '/' && *c != '\\' && *c != '.')
                            .collect();
                        let our_label = if our_label.is_empty() {
                            "signer".to_string()
                        } else {
                            our_label
                        };
                        let default_name = format!("{our_label}-{short_id}.jota-sig");

                        let handle = rfd::AsyncFileDialog::new()
                            .add_filter("Jota Signature", &["jota-sig"])
                            .set_file_name(&default_name)
                            .set_title("Save Signature")
                            .save_file()
                            .await;

                        let path = match handle {
                            Some(file) => file.path().to_path_buf(),
                            None => return Ok("Export cancelled.".to_string()),
                        };
                        std::fs::write(&path, &json)
                            .map_err(|e| anyhow::anyhow!("Failed to write: {e}"))?;
                        Ok(format!("Signature saved: {}", path.display()))
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::MultisigSignExternalCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::MultisigSignExternalCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(msg) => {
                        self.multisig_sign_visible = false;
                        self.multisig_sign_proposal_file = None;
                        self.multisig_sign_error = None;
                        self.multisig_sign_external_path.clear();
                        self.success_message = Some(msg);
                        return self.load_multisig();
                    }
                    Err(e) => self.multisig_sign_error = Some(e),
                }
                Task::none()
            }

            Message::MultisigSignExternalClose => {
                self.multisig_sign_visible = false;
                self.multisig_sign_proposal_file = None;
                self.multisig_sign_error = None;
                self.multisig_sign_external_path.clear();
                Task::none()
            }

            // -- Multisig Import Signature --
            Message::MultisigImportSignature(digest) => {
                self.multisig_import_sig_digest = Some(digest);
                self.multisig_import_sig_visible = true;
                self.multisig_import_sig_path.clear();
                Task::none()
            }

            Message::MultisigImportSigPathChanged(v) => {
                self.multisig_import_sig_path = v;
                Task::none()
            }

            Message::MultisigImportSigConfirm => {
                let path = self.multisig_import_sig_path.trim().to_string();
                if path.is_empty() {
                    self.error_message = Some("Please enter a file path.".into());
                    return Task::none();
                }
                let Some(digest) = self.multisig_import_sig_digest.clone() else {
                    return Task::none();
                };
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let network_name = info.service.network_name().to_string();
                let configs = self.multisig_configs.clone();
                self.loading += 1;
                self.error_message = None;

                Task::perform(
                    async move {
                        use jota_core::multisig::*;

                        let data = std::fs::read(&path)
                            .map_err(|e| anyhow::anyhow!("Failed to read file: {e}"))?;
                        let json = String::from_utf8(data)
                            .map_err(|_| anyhow::anyhow!("File is not valid UTF-8"))?;

                        let store = MultisigStore::open()?;
                        let mut proposal = store.find_proposal_by_prefix(&digest)?;
                        let config = configs
                            .iter()
                            .find(|c| {
                                c.address().to_string().to_lowercase()
                                    == proposal.multisig_address.to_lowercase()
                            })
                            .ok_or_else(|| anyhow::anyhow!("No config found for this proposal"))?;

                        // Try parsing as SignatureFile first
                        if let Ok(sig_file) = serde_json::from_str::<formats::SignatureFile>(&json)
                        {
                            sig_file.validate()?;

                            // Verify digest matches
                            if sig_file.tx_digest != proposal.tx_digest {
                                anyhow::bail!(
                                    "Signature digest mismatch: expected {}, got {}",
                                    &proposal.tx_digest[..8.min(proposal.tx_digest.len())],
                                    &sig_file.tx_digest[..8.min(sig_file.tx_digest.len())]
                                );
                            }

                            validate_signature(
                                &config.committee,
                                &proposal.tx_bytes,
                                &sig_file.public_key,
                                &sig_file.signature,
                                &proposal.signatures,
                            )?;

                            proposal.signatures.push(CollectedSignature {
                                member_key: sig_file.public_key,
                                signature: sig_file.signature,
                                signed_at: timestamp_now(),
                            });

                            update_proposal_status(&config.committee, &mut proposal);
                            store.save_proposal(&proposal)?;

                            return Ok("Signature imported successfully.".to_string());
                        }

                        // Try parsing as ProposalFile
                        if let Ok(prop_file) = serde_json::from_str::<formats::ProposalFile>(&json)
                        {
                            prop_file.validate(&network_name)?;

                            let added = merge_signatures(
                                &config.committee,
                                &mut proposal,
                                &prop_file.signatures,
                            )?;

                            store.save_proposal(&proposal)?;
                            return Ok(format!(
                                "Merged {added} new signature(s) from proposal file."
                            ));
                        }

                        anyhow::bail!(
                            "Unrecognized file format. Expected .jota-sig or .jota-proposal."
                        )
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::MultisigSignatureImported(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::MultisigSignatureImported(result) => {
                self.loading = self.loading.saturating_sub(1);
                self.multisig_import_sig_visible = false;
                self.multisig_import_sig_path.clear();
                self.multisig_import_sig_digest = None;
                match result {
                    Ok(msg) => {
                        self.success_message = Some(msg);
                        return self.load_multisig();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Multisig Browse (native file dialogs) --
            Message::MultisigBrowseImport => Task::perform(
                async {
                    let handle = rfd::AsyncFileDialog::new()
                        .add_filter("Jota Multisig", &["jota-multisig"])
                        .set_title("Select Multisig File")
                        .pick_file()
                        .await;
                    match handle {
                        Some(file) => Ok(file.path().to_string_lossy().to_string()),
                        None => Err("Cancelled".to_string()),
                    }
                },
                Message::MultisigBrowseImportResult,
            ),

            Message::MultisigBrowseImportResult(result) => {
                if let Ok(path) = result {
                    self.multisig_import_path = path;
                }
                Task::none()
            }

            Message::MultisigBrowseSignExternal => Task::perform(
                async {
                    let handle = rfd::AsyncFileDialog::new()
                        .add_filter("Jota Proposal", &["jota-proposal"])
                        .set_title("Select Proposal File")
                        .pick_file()
                        .await;
                    match handle {
                        Some(file) => Ok(file.path().to_string_lossy().to_string()),
                        None => Err("Cancelled".to_string()),
                    }
                },
                Message::MultisigBrowseSignExternalResult,
            ),

            Message::MultisigBrowseSignExternalResult(result) => {
                if let Ok(path) = result {
                    self.multisig_sign_external_path = path;
                }
                Task::none()
            }

            Message::MultisigBrowseImportSig => Task::perform(
                async {
                    let handle = rfd::AsyncFileDialog::new()
                        .add_filter("Jota Signature", &["jota-sig", "jota-proposal"])
                        .set_title("Select Signature File")
                        .pick_file()
                        .await;
                    match handle {
                        Some(file) => Ok(file.path().to_string_lossy().to_string()),
                        None => Err("Cancelled".to_string()),
                    }
                },
                Message::MultisigBrowseImportSigResult,
            ),

            Message::MultisigBrowseImportSigResult(result) => {
                if let Ok(path) = result {
                    self.multisig_import_sig_path = path;
                }
                Task::none()
            }

            // -- Account switching --
            Message::AccountInputChanged(v) => {
                self.account_input = v;
                Task::none()
            }

            Message::AccountGoPressed => {
                let trimmed = self.account_input.trim();
                match trimmed.trim_start_matches('#').parse::<u64>() {
                    Ok(index) => {
                        self.account_input.clear();
                        return self.update(Message::AccountIndexChanged(index));
                    }
                    Err(_) => {
                        self.error_message = Some("Invalid account index".into());
                    }
                }
                Task::none()
            }

            Message::AccountIndexChanged(index) => {
                let path = self.wallet_path();
                let pw = self.session_password.clone();
                let is_hardware = self
                    .wallet_info
                    .as_ref()
                    .map(|i| i.is_hardware)
                    .unwrap_or(false);
                self.loading += 1;
                self.error_message = None;

                Task::perform(
                    async move {
                        let mut wallet = Wallet::open(&path, &pw)?;
                        wallet.switch_account(index)?;

                        #[cfg(feature = "ledger")]
                        if is_hardware {
                            use jota_core::ledger_signer::connect_with_verification;
                            use jota_core::Signer;
                            let bip32_path = jota_core::bip32_path_for(
                                wallet.network_config().network,
                                index as u32,
                            );

                            let signer = tokio::task::spawn_blocking(move || {
                                connect_with_verification(bip32_path)
                            })
                            .await
                            .map_err(|e| anyhow::anyhow!("Task failed: {e}"))??;

                            wallet.set_address(*signer.address());
                            wallet.save(&pw)?;
                            return WalletInfo::from_wallet_with_signer(&wallet, Arc::new(signer));
                        }

                        #[cfg(not(feature = "ledger"))]
                        if is_hardware {
                            anyhow::bail!("Hardware wallet support not compiled in.");
                        }

                        wallet.save(&pw)?;
                        WalletInfo::from_wallet(&wallet)
                    },
                    |r: Result<WalletInfo, anyhow::Error>| {
                        Message::AccountSwitched(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::AccountSwitched(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(info) => {
                        self.active_multisig = None;
                        self.qr_data = qr_code::Data::new(&info.address_string).ok();
                        self.wallet_info = Some(info);
                        self.balance = None;
                        self.transactions.clear();
                        self.account_transactions.clear();
                        self.epoch_deltas.clear();
                        self.balance_chart.clear();
                        self.stakes.clear();
                        self.validators.clear();
                        self.nfts.clear();
                        self.token_balances.clear();
                        self.token_meta.clear();
                        self.selected_token = None;
                        return self.refresh_dashboard();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Sign / Verify --
            Message::SignMessageInputChanged(v) => {
                self.sign_message_input = v;
                Task::none()
            }
            Message::SignModeChanged(mode) => {
                self.sign_mode = mode;
                self.signed_result = None;
                self.verify_result = None;
                self.notarize_result = None;
                self.error_message = None;
                self.success_message = None;
                Task::none()
            }
            Message::ConfirmSign => {
                if self.is_multisig_active() {
                    self.error_message =
                        Some("Message signing is not available for multisig addresses.".into());
                    return Task::none();
                }
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                if self.sign_message_input.is_empty() {
                    self.error_message = Some("Message is required".into());
                    return Task::none();
                }
                let service = info.service.clone();
                let msg = self.sign_message_input.clone();
                self.loading += 1;
                self.error_message = None;

                Task::perform(
                    async move { service.sign_message(msg.as_bytes()) },
                    |r: Result<SignedMessage, _>| {
                        Message::SignCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }
            Message::SignCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(signed) => {
                        self.signed_result = Some(signed);
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }
            Message::CopySignature => {
                if let Some(signed) = &self.signed_result {
                    if let Some(cb) = &mut self.clipboard {
                        match cb.set_text(&signed.signature) {
                            Ok(_) => self.status_message = Some("Signature copied".into()),
                            Err(e) => self.error_message = Some(format!("Copy failed: {e}")),
                        }
                    }
                }
                Task::none()
            }
            Message::CopyPublicKey => {
                if let Some(signed) = &self.signed_result {
                    if let Some(cb) = &mut self.clipboard {
                        match cb.set_text(&signed.public_key) {
                            Ok(_) => self.status_message = Some("Public key copied".into()),
                            Err(e) => self.error_message = Some(format!("Copy failed: {e}")),
                        }
                    }
                }
                Task::none()
            }
            Message::VerifyMessageInputChanged(v) => {
                self.verify_message_input = v;
                self.verify_result = None;
                Task::none()
            }
            Message::VerifySignatureInputChanged(v) => {
                self.verify_signature_input = v;
                self.verify_result = None;
                Task::none()
            }
            Message::VerifyPublicKeyInputChanged(v) => {
                self.verify_public_key_input = v;
                self.verify_result = None;
                Task::none()
            }
            Message::ConfirmVerify => {
                let msg = self.verify_message_input.clone();
                let sig = self.verify_signature_input.clone();
                let pk = self.verify_public_key_input.clone();
                if msg.is_empty() || sig.is_empty() || pk.is_empty() {
                    self.error_message = Some("All fields are required".into());
                    return Task::none();
                }
                self.loading += 1;
                self.error_message = None;

                Task::perform(
                    async move { verify_message(msg.as_bytes(), &sig, &pk) },
                    |r: Result<bool, anyhow::Error>| {
                        Message::VerifyCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }
            Message::VerifyCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(valid) => self.verify_result = Some(valid),
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::NotarizeDescriptionChanged(v) => {
                self.notarize_description = v;
                Task::none()
            }
            Message::ConfirmNotarize => {
                if self.is_multisig_active() {
                    self.error_message =
                        Some("Notarization is not available for multisig addresses.".into());
                    return Task::none();
                }
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                if self.sign_message_input.is_empty() {
                    self.error_message = Some("Message is required".into());
                    return Task::none();
                }
                let service = info.service.clone();
                let msg = self.sign_message_input.clone();
                let desc = if self.notarize_description.is_empty() {
                    None
                } else {
                    Some(self.notarize_description.clone())
                };
                self.loading += 1;
                self.error_message = None;

                Task::perform(
                    async move {
                        let result = service.notarize(&msg, desc.as_deref()).await?;
                        Ok(result.digest)
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::NotarizeCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }
            Message::NotarizeCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(digest) => {
                        self.notarize_result = Some(digest);
                        self.success_message = Some("Notarized on-chain".into());
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Native messaging (browser extension bridge) --
            Message::NativeRequest(req) => {
                let info = match &self.wallet_info {
                    Some(info) => info,
                    None => {
                        // Buffer the request — the dApp waits while the user
                        // unlocks. Replayed from WalletOpened / AccountSwitched.
                        self.buffered_native_requests.push(req);
                        return Task::none();
                    }
                };

                let origin = req
                    .params
                    .get("origin")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                match req.method.as_str() {
                    "connect" => {
                        if self.permissions.is_allowed(&info.address_string, &origin) {
                            // Auto-approve for known origins
                            match self.build_connect_response(&req.id, info) {
                                Some(resp) => self.send_native_response(resp),
                                None => return Task::none(),
                            }
                            return Task::none();
                        }

                        // Unknown origin — show approval modal
                        if self.pending_approval.is_some() {
                            self.send_native_response(NativeResponse::err(
                                req.id,
                                "BUSY",
                                "Another request is pending",
                            ));
                            return Task::none();
                        }

                        self.pending_approval = Some(PendingApproval {
                            request_id: req.id,
                            method: "connect".into(),
                            params: req.params,
                            summary: Some("Wants to connect to your wallet".into()),
                            origin,
                        });
                        Task::none()
                    }
                    "signTransaction" | "signAndExecuteTransaction" | "signPersonalMessage" => {
                        // Reject if another approval is already pending
                        if self.pending_approval.is_some() {
                            self.send_native_response(NativeResponse::err(
                                req.id,
                                "BUSY",
                                "Another signing request is pending",
                            ));
                            return Task::none();
                        }

                        // Check chain matches current network (for tx methods)
                        if req.method != "signPersonalMessage" {
                            if let Some(chain) = req.params.get("chain").and_then(|c| c.as_str()) {
                                let expected = format!(
                                    "iota:{}",
                                    info.network_config.network.to_string().to_lowercase()
                                );
                                if chain != expected {
                                    self.send_native_response(NativeResponse::err(
                                        req.id,
                                        "NETWORK_MISMATCH",
                                        format!("Wallet is on {expected}, request is for {chain}"),
                                    ));
                                    return Task::none();
                                }
                            }
                        }

                        let summary = match req.method.as_str() {
                            "signTransaction" => Some("Sign a transaction".into()),
                            "signAndExecuteTransaction" => {
                                Some("Sign and execute a transaction".into())
                            }
                            "signPersonalMessage" => Some("Sign a personal message".into()),
                            _ => None,
                        };

                        self.pending_approval = Some(PendingApproval {
                            request_id: req.id,
                            method: req.method,
                            params: req.params,
                            summary,
                            origin,
                        });
                        Task::none()
                    }
                    _ => {
                        self.send_native_response(NativeResponse::err(
                            req.id,
                            "UNKNOWN_METHOD",
                            format!("Unknown method: {}", req.method),
                        ));
                        Task::none()
                    }
                }
            }

            Message::NativeClientConnected(tx) => {
                self.native_response_tx = Some(tx);
                Task::none()
            }

            Message::NativeClientDisconnected => {
                self.native_response_tx = None;
                self.pending_approval = None;
                Task::none()
            }

            Message::ApproveNativeRequest => {
                let approval = match self.pending_approval.take() {
                    Some(a) => a,
                    None => return Task::none(),
                };
                let Some(info) = &self.wallet_info else {
                    self.send_native_response(NativeResponse::err(
                        approval.request_id,
                        "WALLET_LOCKED",
                        "Wallet is locked",
                    ));
                    return Task::none();
                };

                // Handle connect approval: grant permission and respond with account info
                if approval.method == "connect" {
                    self.permissions
                        .grant(&info.address_string, &approval.origin);
                    if let Some(resp) = self.build_connect_response(&approval.request_id, info) {
                        self.send_native_response(resp);
                    }
                    return Task::none();
                }

                let service = info.service.clone();
                let request_id = approval.request_id.clone();
                let request_id_err = request_id.clone();
                let method = approval.method.clone();
                let params = approval.params.clone();

                Task::perform(
                    async move {
                        match method.as_str() {
                            "signTransaction" => {
                                let tx_b64 = params
                                    .get("transaction")
                                    .and_then(|v| v.as_str())
                                    .ok_or_else(|| {
                                        anyhow::anyhow!("Missing 'transaction' param")
                                    })?;
                                let (bytes, signature) =
                                    service.sign_raw_transaction(tx_b64).await?;
                                let result = serde_json::json!({
                                    "bytes": bytes,
                                    "signature": signature,
                                });
                                Ok((request_id, result))
                            }
                            "signAndExecuteTransaction" => {
                                let tx_b64 = params
                                    .get("transaction")
                                    .and_then(|v| v.as_str())
                                    .ok_or_else(|| {
                                        anyhow::anyhow!("Missing 'transaction' param")
                                    })?;
                                let transfer_result = service.sign_and_execute_raw(tx_b64).await?;
                                let result = serde_json::json!({
                                    "digest": transfer_result.digest,
                                    "effects": transfer_result.status,
                                });
                                Ok((request_id, result))
                            }
                            "signPersonalMessage" => {
                                let msg_b64 = params
                                    .get("message")
                                    .and_then(|v| v.as_str())
                                    .ok_or_else(|| anyhow::anyhow!("Missing 'message' param"))?;
                                let msg_bytes = base64ct::Base64::decode_vec(msg_b64)
                                    .map_err(|e| anyhow::anyhow!("Invalid base64 message: {e}"))?;
                                let signed = service.sign_message(&msg_bytes)?;
                                let result = serde_json::json!({
                                    "bytes": msg_b64,
                                    "signature": signed.signature,
                                });
                                Ok((request_id, result))
                            }
                            other => Err(anyhow::anyhow!("Unknown method: {other}")),
                        }
                    },
                    move |r: Result<(String, serde_json::Value), anyhow::Error>| match r {
                        Ok((id, val)) => Message::NativeSignCompleted(Ok((id, val))),
                        Err(e) => Message::NativeSignCompleted(Err((
                            request_id_err,
                            "SIGNING_FAILED".into(),
                            e.to_string(),
                        ))),
                    },
                )
            }

            Message::RejectNativeRequest => {
                if let Some(approval) = self.pending_approval.take() {
                    self.send_native_response(NativeResponse::err(
                        approval.request_id,
                        "USER_REJECTED",
                        "User rejected the request",
                    ));
                }
                Task::none()
            }

            Message::NativeSignCompleted(result) => {
                match result {
                    Ok((id, value)) => {
                        self.send_native_response(NativeResponse::ok(id, value));
                    }
                    Err((id, code, message)) => {
                        self.send_native_response(NativeResponse::err(id, code, message));
                    }
                }
                Task::none()
            }

            // -- Native host installation --
            Message::ExtensionIdChanged(v) => {
                self.extension_id = v;
                Task::none()
            }

            Message::InstallNativeHost => {
                let ext_id = self.extension_id.trim().to_string();
                if ext_id.is_empty() {
                    self.error_message = Some("Extension ID is required".into());
                    return Task::none();
                }
                self.error_message = None;
                self.success_message = None;

                Task::perform(
                    async move {
                        crate::native_messaging::install_native_host(&ext_id)
                            .map_err(|e| e.to_string())
                    },
                    Message::NativeHostInstalled,
                )
            }

            Message::NativeHostInstalled(result) => {
                match result {
                    Ok(paths) => {
                        let count = paths.len();
                        self.success_message = Some(format!(
                            "Native host installed ({count} browser{})",
                            if count == 1 { "" } else { "s" }
                        ));
                    }
                    Err(e) => self.error_message = Some(format!("Install failed: {e}")),
                }
                Task::none()
            }

            Message::RevokeSitePermission(origin) => {
                if let Some(info) = &self.wallet_info {
                    let addr = info.address_string.clone();
                    self.permissions.revoke(&addr, &origin);
                }
                Task::none()
            }

            Message::HistoryLookbackChanged(epochs) => {
                self.history_lookback = epochs;
                self.refresh_dashboard()
            }

            Message::NetworkChanged(network) => {
                let config = NetworkConfig {
                    network,
                    custom_url: None,
                };
                self.network_config = config.clone();
                self.balance = None;
                self.transactions.clear();
                self.account_transactions.clear();
                self.epoch_deltas.clear();
                self.balance_chart.clear();
                self.stakes.clear();
                self.validators.clear();
                self.nfts.clear();
                self.token_balances.clear();
                self.token_meta.clear();
                self.selected_token = None;
                if let Some(info) = &mut self.wallet_info {
                    info.network_config = config.clone();
                    info.is_mainnet = network == Network::Mainnet;
                    match NetworkClient::new(&config, false) {
                        Ok(client) => {
                            let signer = info.service.signer().clone();
                            let service = WalletService::new(client, signer)
                                .with_notarization_package(info.notarization_package_config);
                            info.notarization_package = service.notarization_package();
                            info.service = Arc::new(service);
                        }
                        Err(e) => {
                            self.error_message = Some(format!("Failed to switch network: {e}"));
                            return Task::none();
                        }
                    }
                    return self.refresh_dashboard();
                }
                Task::none()
            }

            Message::SettingsOldPasswordChanged(v) => {
                self.settings_old_password = v;
                Task::none()
            }
            Message::SettingsNewPasswordChanged(v) => {
                self.settings_new_password = v;
                Task::none()
            }
            Message::SettingsNewPasswordConfirmChanged(v) => {
                self.settings_new_password_confirm = v;
                Task::none()
            }

            Message::ChangePassword => {
                if self.settings_old_password.is_empty() {
                    self.error_message = Some("Current password is required".into());
                    return Task::none();
                }
                if self.settings_new_password.is_empty() {
                    self.error_message = Some("New password is required".into());
                    return Task::none();
                }
                if *self.settings_new_password != *self.settings_new_password_confirm {
                    self.error_message = Some("New passwords don't match".into());
                    return Task::none();
                }
                let path = self.wallet_path();
                let old_pw = Zeroizing::new(self.settings_old_password.as_bytes().to_vec());
                let new_pw = Zeroizing::new(self.settings_new_password.as_bytes().to_vec());
                self.loading += 1;
                self.error_message = None;
                self.success_message = None;

                Task::perform(
                    async move {
                        Wallet::change_password(&path, &old_pw, &new_pw)?;
                        Ok(new_pw)
                    },
                    |r: Result<Zeroizing<Vec<u8>>, anyhow::Error>| {
                        Message::ChangePasswordCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::ChangePasswordCompleted(result) => {
                self.loading = self.loading.saturating_sub(1);
                match result {
                    Ok(new_pw) => {
                        self.session_password = new_pw;
                        self.success_message = Some("Password changed".into());
                        self.settings_old_password.zeroize();
                        self.settings_new_password.zeroize();
                        self.settings_new_password_confirm.zeroize();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }
        }
    }

    // -- Helpers --

    fn clear_form(&mut self) {
        self.password.zeroize();
        self.password_confirm.zeroize();
        self.mnemonic_input.zeroize();
        self.recipient.clear();
        self.amount.clear();
        self.resolved_recipient = None;
        self.selected_token = None;
        self.error_message = None;
        self.success_message = None;
        self.status_message = None;
        self.created_mnemonic = None;
        self.expanded_tx = None;
        self.account_input.clear();
        self.validator_address.clear();
        self.stake_amount.clear();
        self.selected_validator = None;
        self.sign_message_input.clear();
        self.signed_result = None;
        self.verify_message_input.clear();
        self.verify_signature_input.clear();
        self.verify_public_key_input.clear();
        self.verify_result = None;
        self.notarize_description.clear();
        self.notarize_result = None;
        self.send_nft_object_id = None;
        self.send_nft_recipient.clear();
        self.multisig_selected = None;
        self.multisig_proposal_selected = None;
        self.multisig_import_error = None;
        self.multisig_create_visible = false;
        self.multisig_create_step = 0;
        self.multisig_create_name.clear();
        self.multisig_create_num_participants = "2".to_string();
        self.multisig_create_threshold.clear();
        self.multisig_create_members.clear();
        self.multisig_create_my_weight = "1".to_string();
        self.multisig_create_my_label = "me".to_string();
        self.multisig_create_error = None;
        self.multisig_my_public_key_b64 = None;
        self.multisig_send_visible = false;
        self.multisig_send_config_idx = None;
        self.multisig_send_recipient.clear();
        self.multisig_send_amount.clear();
        self.multisig_send_error = None;
        self.multisig_import_path.clear();
        self.multisig_import_visible = false;
        self.multisig_sign_visible = false;
        self.multisig_sign_proposal_file = None;
        self.multisig_sign_error = None;
        self.multisig_sign_external_path.clear();
        self.multisig_import_sig_digest = None;
        self.multisig_import_sig_path.clear();
        self.multisig_import_sig_visible = false;
        self.contact_form_visible = false;
        self.contact_form_name.clear();
        self.contact_form_address.clear();
        self.contact_form_editing = None;
        self.save_contact_offer = None;
        self.settings_old_password.zeroize();
        self.settings_new_password.zeroize();
        self.settings_new_password_confirm.zeroize();
        // Keep extension_id across screens — it's a one-time config
    }

    pub(crate) fn password_warning(&self) -> Option<&'static str> {
        if !self.password.is_empty() && self.password.len() < 4 {
            return Some("Very short password — offers little protection if wallet file is stolen");
        }
        None
    }

    pub(crate) fn validate_create_form(&self) -> Option<String> {
        if self.wallet_name.trim().is_empty() {
            return Some("Wallet name is required".into());
        }
        if self.password != self.password_confirm {
            return Some("Passwords don't match".into());
        }
        if self
            .wallet_entries
            .iter()
            .any(|e| e.name == self.wallet_name.trim())
        {
            return Some(format!(
                "Wallet '{}' already exists",
                self.wallet_name.trim()
            ));
        }
        None
    }

    fn compute_balance_history(&mut self) {
        let Some(current_balance) = self.balance else {
            return;
        };
        if self.epoch_deltas.is_empty() {
            return;
        }

        let start = self.epoch_deltas.len().saturating_sub(30);
        let recent = &self.epoch_deltas[start..];

        let mut bal = current_balance as f64 / 1_000_000_000.0;
        let mut history: Vec<(u64, f64)> = Vec::with_capacity(recent.len() + 1);
        for &(epoch, delta) in recent.iter().rev() {
            history.push((epoch, bal));
            bal -= delta as f64 / 1_000_000_000.0;
        }
        // Add starting point (balance before first displayed epoch)
        if let Some(&(first_epoch, _)) = recent.first() {
            history.push((first_epoch.saturating_sub(1), bal));
        }
        history.reverse();
        self.balance_chart.update(history);
    }

    fn refresh_dashboard(&mut self) -> Task<Message> {
        let Some(info) = &self.wallet_info else {
            return Task::none();
        };
        self.loading += 3;
        self.history_page = 0;

        let svc1 = info.service.clone();
        let svc2 = info.service.clone();
        let svc3 = info.service.clone();
        let network_name = info.service.network_name().to_string();
        let addr = self.active_address().unwrap_or(info.address);
        let address_str = addr.to_string();
        let lookback = self.history_lookback;

        Task::batch([
            Task::perform(
                async move { svc1.network().balance(&addr).await },
                |r: Result<u64, _>| Message::BalanceUpdated(r.map_err(|e| e.to_string())),
            ),
            Task::perform(
                {
                    let address_str = address_str.clone();
                    async move {
                        svc2.network().sync_transactions(&addr, lookback).await?;
                        let cache = TransactionCache::open()?;
                        let page = cache.query(
                            &network_name,
                            &address_str,
                            &TransactionFilter::All,
                            25,
                            0,
                        )?;
                        let deltas = cache.query_epoch_deltas(&network_name, &address_str)?;
                        Ok((page.transactions, page.total, deltas))
                    }
                },
                |r: Result<crate::messages::TransactionPage, anyhow::Error>| {
                    Message::TransactionsLoaded(r.map_err(|e| e.to_string()))
                },
            ),
            Task::perform(
                async move {
                    let balances = svc3.network().get_token_balances(&addr).await?;
                    let mut meta = Vec::new();
                    for b in &balances {
                        if b.coin_type != "0x2::iota::IOTA" {
                            if let Ok(m) = svc3.resolve_coin_type(&b.coin_type).await {
                                meta.push(m);
                            }
                        }
                    }
                    Ok((balances, meta))
                },
                |r: Result<(Vec<TokenBalance>, Vec<CoinMeta>), anyhow::Error>| {
                    Message::TokenBalancesLoaded(r.map_err(|e| e.to_string()))
                },
            ),
        ])
    }

    fn load_history_page(&mut self) -> Task<Message> {
        let Some(info) = &self.wallet_info else {
            return Task::none();
        };
        let network_str = info.service.network_name().to_string();
        let addr = self.active_address().unwrap_or(info.address);
        let address_str = addr.to_string();
        let offset = self.history_page * 25;

        Task::perform(
            async move {
                let cache = TransactionCache::open()?;
                let page = cache.query(
                    &network_str,
                    &address_str,
                    &TransactionFilter::All,
                    25,
                    offset,
                )?;
                Ok((page.transactions, page.total, Vec::new()))
            },
            |r: Result<crate::messages::TransactionPage, anyhow::Error>| {
                Message::TransactionsLoaded(r.map_err(|e| e.to_string()))
            },
        )
    }

    fn load_nfts(&mut self) -> Task<Message> {
        let Some(info) = &self.wallet_info else {
            return Task::none();
        };
        self.loading += 1;
        let service = info.service.clone();
        let addr = self.active_address().unwrap_or(info.address);

        Task::perform(
            async move { service.network().get_nfts(&addr).await },
            |r: Result<Vec<NftSummary>, _>| Message::NftsLoaded(r.map_err(|e| e.to_string())),
        )
    }

    fn load_stakes(&mut self) -> Task<Message> {
        let Some(info) = &self.wallet_info else {
            return Task::none();
        };
        self.loading += 1;
        let service = info.service.clone();
        let addr = self.active_address().unwrap_or(info.address);

        Task::perform(
            async move { service.network().get_stakes(&addr).await },
            |r: Result<Vec<StakedIotaSummary>, _>| {
                Message::StakesLoaded(r.map_err(|e| e.to_string()))
            },
        )
    }

    fn load_validators(&mut self) -> Task<Message> {
        let Some(info) = &self.wallet_info else {
            return Task::none();
        };
        self.loading += 1;
        let service = info.service.clone();

        Task::perform(
            async move { service.get_validators().await },
            |r: Result<Vec<ValidatorSummary>, _>| {
                Message::ValidatorsLoaded(r.map_err(|e| e.to_string()))
            },
        )
    }

    fn load_multisig(&mut self) -> Task<Message> {
        self.loading += 1;
        Task::perform(
            async move {
                let store = jota_core::multisig::MultisigStore::open()?;
                let configs = store.list_configs()?;
                let proposals = store.list_proposals()?;
                Ok((configs, proposals))
            },
            |r: Result<
                (
                    Vec<jota_core::multisig::MultisigConfig>,
                    Vec<jota_core::multisig::TransactionProposal>,
                ),
                anyhow::Error,
            >| { Message::MultisigLoaded(r.map_err(|e| e.to_string())) },
        )
    }

    fn load_contacts(&mut self) -> Task<Message> {
        Task::perform(
            async move {
                let store = ContactStore::open()?;
                Ok(store.list().to_vec())
            },
            |r: Result<Vec<Contact>, anyhow::Error>| {
                Message::ContactsLoaded(r.map_err(|e| e.to_string()))
            },
        )
    }

    /// Replay any native requests that arrived while the wallet was locked,
    /// then append `extra` (typically refresh_dashboard) to the task batch.
    fn replay_buffered_native_requests(&mut self, extra: Task<Message>) -> Task<Message> {
        let pending = std::mem::take(&mut self.buffered_native_requests);
        if pending.is_empty() {
            return extra;
        }
        let mut tasks: Vec<Task<Message>> = pending
            .into_iter()
            .map(|req| self.update(Message::NativeRequest(req)))
            .collect();
        tasks.push(extra);
        Task::batch(tasks)
    }

    fn send_native_response(&self, response: NativeResponse) {
        if let Some(tx) = &self.native_response_tx {
            let _ = tx.send(response);
        }
    }

    fn build_connect_response(
        &self,
        request_id: &str,
        info: &WalletInfo,
    ) -> Option<NativeResponse> {
        let signer = info.service.signer();
        let pk_bytes = match signer.public_key_bytes() {
            Ok(b) => b,
            Err(e) => {
                self.send_native_response(NativeResponse::err(
                    request_id.to_string(),
                    "INTERNAL_ERROR",
                    e.to_string(),
                ));
                return None;
            }
        };
        let chain = format!(
            "iota:{}",
            info.network_config.network.to_string().to_lowercase()
        );
        let result = serde_json::json!({
            "accounts": [{
                "address": info.address_string,
                "publicKey": base64ct::Base64::encode_string(&pk_bytes),
                "chains": [chain],
                "features": [
                    "iota:signTransaction",
                    "iota:signAndExecuteTransaction",
                    "iota:signPersonalMessage",
                ],
            }]
        });
        Some(NativeResponse::ok(request_id.to_string(), result))
    }

    fn resolve_validator_names(stakes: &mut [StakedIotaSummary], validators: &[ValidatorSummary]) {
        if validators.is_empty() {
            return;
        }
        for stake in stakes.iter_mut() {
            if stake.validator_name.is_none() {
                stake.validator_name = validators
                    .iter()
                    .find(|v| v.staking_pool_id == stake.pool_id)
                    .map(|v| v.name.clone());
            }
        }
    }
}
