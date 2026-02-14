use crate::messages::Message;
use crate::state::{Screen, WalletInfo};
use crate::App;
use iced::widget::qr_code;
use iced::Task;
use iota_wallet_core::cache::TransactionCache;
use iota_wallet_core::display::{parse_iota_amount, parse_token_amount};
use iota_wallet_core::network::{
    CoinMeta, NetworkClient, NftSummary, StakedIotaSummary, TokenBalance, TransactionFilter,
    TransactionSummary,
};
use iota_wallet_core::service::WalletService;
use iota_wallet_core::wallet::{Network, NetworkConfig, Wallet};
use iota_wallet_core::{list_wallets, validate_wallet_name};
use iota_wallet_core::{verify_message, ObjectId, Recipient, SignedMessage};
use std::sync::Arc;
use zeroize::{Zeroize, Zeroizing};

impl App {
    // -- Update --

    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::GoTo(screen) => {
                self.clear_form();
                if screen == Screen::WalletSelect {
                    self.wallet_entries = list_wallets(&self.wallet_dir);
                    self.wallet_info = None;
                    self.qr_data = None;
                    self.balance = None;
                    self.transactions.clear();
                    self.account_transactions.clear();
                    self.epoch_deltas.clear();
                    self.balance_chart.clear();
                    self.stakes.clear();
                    self.nfts.clear();
                    self.token_balances.clear();
                    self.token_meta.clear();
                    self.session_password.zeroize();
                }
                let load_stakes = screen == Screen::Staking;
                let load_nfts = screen == Screen::Nfts;
                self.screen = screen;
                if load_stakes {
                    return self.load_stakes();
                }
                if load_nfts {
                    return self.load_nfts();
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
                let name = self.selected_wallet.clone().unwrap_or_default();
                let path = self.wallet_dir.join(format!("{name}.wallet"));
                let pw = Zeroizing::new(self.password.as_bytes().to_vec());
                self.session_password = pw.clone();
                self.loading += 1;
                self.error_message = None;

                Task::perform(
                    async move {
                        let wallet = Wallet::open(&path, &pw)?;

                        #[cfg(feature = "ledger")]
                        if wallet.is_hardware() {
                            use iota_wallet_core::ledger_signer::connect_and_verify;

                            let bip32_path = iota_wallet_core::bip32_path_for(
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
                    Ok(info) => {
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
                        std::fs::create_dir_all(path.parent().expect("wallet path has parent"))?;
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
                        std::fs::create_dir_all(path.parent().expect("wallet path has parent"))?;
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
                            use iota_wallet_core::ledger_signer::connect_with_verification;
                            use iota_wallet_core::Signer;

                            let bip32_path = iota_wallet_core::bip32_path_for(config.network, 0);

                            let signer = tokio::task::spawn_blocking(move || {
                                connect_with_verification(bip32_path)
                            })
                            .await
                            .map_err(|e| anyhow::anyhow!("Task failed: {e}"))??;

                            let address = *signer.address();
                            std::fs::create_dir_all(
                                path.parent().expect("wallet path has parent"),
                            )?;
                            let wallet = Wallet::create_hardware(
                                path,
                                &pw,
                                address,
                                config,
                                iota_wallet_core::HardwareKind::Ledger,
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
                    |r: Result<(), _>| {
                        Message::HardwareReconnected(r.map_err(|e| e.to_string()))
                    },
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
                self.loading += 1;
                self.error_message = None;
                self.success_message = None;

                Task::perform(
                    async move {
                        service.faucet().await?;
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
                if let Some(info) = &self.wallet_info {
                    if let Some(cb) = &mut self.clipboard {
                        match cb.set_text(&info.address_string) {
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

            // -- Staking --
            Message::ValidatorResolved(result) => {
                self.resolved_validator = Some(result);
                Task::none()
            }

            Message::ValidatorAddressChanged(v) => {
                self.validator_address = v.clone();
                self.resolved_validator = None;
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
                                Message::ValidatorResolved(r.map_err(|e| e.to_string()))
                            },
                        );
                    }
                }
                Task::none()
            }
            Message::StakeAmountChanged(v) => {
                self.stake_amount = v;
                Task::none()
            }
            Message::RefreshStakes => self.load_stakes(),

            Message::ConfirmStake => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let validator_str = self.validator_address.trim().to_string();
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
                        return self.load_stakes();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            Message::ConfirmUnstake(object_id_str) => {
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
                    Ok(s) => self.stakes = s,
                    Err(e) => self.error_message = Some(e),
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
                let name = self.selected_wallet.clone().unwrap_or_default();
                let path = self.wallet_dir.join(format!("{name}.wallet"));
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
                            use iota_wallet_core::ledger_signer::connect_with_verification;
                            use iota_wallet_core::Signer;
                            let bip32_path = iota_wallet_core::bip32_path_for(
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
                        self.qr_data = qr_code::Data::new(&info.address_string).ok();
                        self.wallet_info = Some(info);
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
                            let service = WalletService::new(client, signer, network.to_string())
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
                let name = self.selected_wallet.clone().unwrap_or_default();
                let path = self.wallet_dir.join(format!("{name}.wallet"));
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
        self.resolved_validator = None;
        self.selected_token = None;
        self.error_message = None;
        self.success_message = None;
        self.status_message = None;
        self.created_mnemonic = None;
        self.expanded_tx = None;
        self.account_input.clear();
        self.validator_address.clear();
        self.stake_amount.clear();
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
        self.settings_old_password.zeroize();
        self.settings_new_password.zeroize();
        self.settings_new_password_confirm.zeroize();
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
        let address_str = info.address.to_string();

        Task::batch([
            Task::perform(
                async move { svc1.balance().await },
                |r: Result<u64, _>| {
                    Message::BalanceUpdated(r.map_err(|e| e.to_string()))
                },
            ),
            Task::perform(
                async move {
                    svc2.sync_transactions().await?;
                    let cache = TransactionCache::open()?;
                    let page =
                        cache.query(&network_name, &address_str, &TransactionFilter::All, 25, 0)?;
                    let deltas = cache.query_epoch_deltas(&network_name, &address_str)?;
                    Ok((page.transactions, page.total, deltas))
                },
                |r: Result<(Vec<TransactionSummary>, u32, Vec<(u64, i64)>), anyhow::Error>| {
                    Message::TransactionsLoaded(r.map_err(|e| e.to_string()))
                },
            ),
            Task::perform(
                async move {
                    let balances = svc3.get_token_balances().await?;
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
        let address_str = info.address.to_string();
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
            |r: Result<(Vec<TransactionSummary>, u32, Vec<(u64, i64)>), anyhow::Error>| {
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

        Task::perform(
            async move { service.get_nfts().await },
            |r: Result<Vec<NftSummary>, _>| {
                Message::NftsLoaded(r.map_err(|e| e.to_string()))
            },
        )
    }

    fn load_stakes(&mut self) -> Task<Message> {
        let Some(info) = &self.wallet_info else {
            return Task::none();
        };
        self.loading += 1;
        let service = info.service.clone();

        Task::perform(
            async move { service.get_stakes().await },
            |r: Result<Vec<StakedIotaSummary>, _>| {
                Message::StakesLoaded(r.map_err(|e| e.to_string()))
            },
        )
    }
}
