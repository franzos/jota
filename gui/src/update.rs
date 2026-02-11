use crate::messages::Message;
use crate::state::{Screen, WalletInfo};
use crate::App;
use iced::Task;
use iota_sdk::types::{Address, ObjectId};
use iota_wallet_core::cache::TransactionCache;
use iota_wallet_core::display::parse_iota_amount;
use iota_wallet_core::network::{
    NetworkClient, StakedIotaSummary, TransactionFilter, TransactionSummary,
};
use iota_wallet_core::wallet::{Network, NetworkConfig, Wallet};
use iota_wallet_core::{list_wallets, validate_wallet_name};
use std::sync::Arc;
use zeroize::{Zeroize, Zeroizing};

impl App {
    // -- Update --

    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::GoTo(screen) => {
                self.clear_form();
                if screen == Screen::WalletSelect {
                    self.wallet_names = list_wallets(&self.wallet_dir);
                    self.wallet_info = None;
                    self.balance = None;
                    self.transactions.clear();
                    self.epoch_deltas.clear();
                    self.balance_chart.clear();
                    self.stakes.clear();
                }
                let load_stakes = screen == Screen::Staking;
                self.screen = screen;
                if load_stakes {
                    return self.load_stakes();
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
                self.password = Zeroizing::new(v);
                Task::none()
            }
            Message::PasswordConfirmChanged(v) => {
                self.password_confirm = Zeroizing::new(v);
                Task::none()
            }
            Message::WalletNameChanged(v) => {
                self.wallet_name = v;
                Task::none()
            }
            Message::MnemonicInputChanged(v) => {
                self.mnemonic_input = Zeroizing::new(v);
                Task::none()
            }
            Message::RecipientChanged(v) => {
                self.recipient = v;
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
                self.loading = true;
                self.error_message = None;

                Task::perform(
                    async move {
                        let wallet = Wallet::open(&path, &pw)?;
                        WalletInfo::from_wallet(&wallet)
                    },
                    |r: Result<WalletInfo, anyhow::Error>| {
                        Message::WalletOpened(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::WalletOpened(result) => {
                self.loading = false;
                match result {
                    Ok(info) => {
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
                let config = self.network_config.clone();
                self.loading = true;
                self.error_message = None;

                Task::perform(
                    async move {
                        std::fs::create_dir_all(path.parent().unwrap())?;
                        let wallet = Wallet::create_new(path, &pw, config)?;
                        let mnemonic = Zeroizing::new(wallet.mnemonic().to_string());
                        let info = WalletInfo::from_wallet(&wallet)?;
                        Ok((info, mnemonic))
                    },
                    |r: Result<(WalletInfo, Zeroizing<String>), anyhow::Error>| {
                        Message::WalletCreated(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::WalletCreated(result) => {
                self.loading = false;
                match result {
                    Ok((info, mnemonic)) => {
                        self.selected_wallet = Some(self.wallet_name.clone());
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
                let mnemonic = Zeroizing::new(self.mnemonic_input.trim().to_string());
                let config = self.network_config.clone();
                self.loading = true;
                self.error_message = None;

                Task::perform(
                    async move {
                        std::fs::create_dir_all(path.parent().unwrap())?;
                        let wallet =
                            Wallet::recover_from_mnemonic(path, &pw, &mnemonic, config)?;
                        WalletInfo::from_wallet(&wallet)
                    },
                    |r: Result<WalletInfo, anyhow::Error>| {
                        Message::WalletRecovered(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::WalletRecovered(result) => {
                self.loading = false;
                match result {
                    Ok(info) => {
                        self.selected_wallet = Some(self.wallet_name.clone());
                        self.wallet_info = Some(info);
                        self.clear_form();
                        self.screen = Screen::Account;
                        return self.refresh_dashboard();
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }

            // -- Dashboard --
            Message::RefreshBalance => self.refresh_dashboard(),

            Message::BalanceUpdated(result) => {
                self.loading = false;
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
                let address = info.address;
                let net = info.network_client.clone();
                self.loading = true;
                self.error_message = None;
                self.success_message = None;

                Task::perform(
                    async move {
                        net.faucet(&address).await?;
                        Ok(())
                    },
                    |r: Result<(), anyhow::Error>| {
                        Message::FaucetCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::FaucetCompleted(result) => {
                self.loading = false;
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
                    if let Ok(mut cb) = arboard::Clipboard::new() {
                        let _ = cb.set_text(&info.address_string);
                        self.status_message = Some("Address copied".into());
                    }
                }
                Task::none()
            }

            Message::TransactionsLoaded(result) => {
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
            Message::ConfirmSend => {
                let Some(info) = &self.wallet_info else {
                    return Task::none();
                };
                let recipient_str = self.recipient.trim().to_string();
                if recipient_str.is_empty() {
                    self.error_message = Some("Recipient address is required".into());
                    return Task::none();
                }
                let amount = match parse_iota_amount(&self.amount) {
                    Ok(0) => {
                        self.error_message = Some("Amount must be greater than 0".into());
                        return Task::none();
                    }
                    Ok(a) => a,
                    Err(e) => {
                        self.error_message = Some(e);
                        return Task::none();
                    }
                };
                let sender = info.address;
                let net = info.network_client.clone();
                let signer = info.signer.clone();
                self.loading = true;
                self.error_message = None;

                Task::perform(
                    async move {
                        let recipient = Address::from_hex(&recipient_str)
                            .map_err(|e| anyhow::anyhow!("Invalid recipient address: {e}"))?;
                        let result = net.send_iota(signer.as_ref(), &sender, recipient, amount).await?;
                        Ok(result.digest)
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::SendCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::SendCompleted(result) => {
                self.loading = false;
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
                let network = self
                    .wallet_info
                    .as_ref()
                    .map(|i| &i.network_config.network);
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
            Message::ValidatorAddressChanged(v) => {
                self.validator_address = v;
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
                    self.error_message = Some("Validator address is required".into());
                    return Task::none();
                }
                let amount = match parse_iota_amount(&self.stake_amount) {
                    Ok(0) => {
                        self.error_message = Some("Amount must be greater than 0".into());
                        return Task::none();
                    }
                    Ok(a) => a,
                    Err(e) => {
                        self.error_message = Some(e);
                        return Task::none();
                    }
                };
                let sender = info.address;
                let net = info.network_client.clone();
                let signer = info.signer.clone();
                self.loading = true;
                self.error_message = None;

                Task::perform(
                    async move {
                        let validator = Address::from_hex(&validator_str)
                            .map_err(|e| anyhow::anyhow!("Invalid validator address: {e}"))?;
                        let result = net.stake_iota(signer.as_ref(), &sender, validator, amount).await?;
                        Ok(result.digest)
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::StakeCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::StakeCompleted(result) => {
                self.loading = false;
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
                let sender = info.address;
                let net = info.network_client.clone();
                let signer = info.signer.clone();
                self.loading = true;
                self.error_message = None;
                self.success_message = None;

                Task::perform(
                    async move {
                        let object_id = ObjectId::from_hex(&object_id_str)
                            .map_err(|e| anyhow::anyhow!("Invalid object ID: {e}"))?;
                        let result = net.unstake_iota(signer.as_ref(), &sender, object_id).await?;
                        Ok(result.digest)
                    },
                    |r: Result<String, anyhow::Error>| {
                        Message::UnstakeCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::UnstakeCompleted(result) => {
                self.loading = false;
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
                self.loading = false;
                match result {
                    Ok(s) => self.stakes = s,
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
                if let Some(info) = &mut self.wallet_info {
                    info.network_config = config.clone();
                    info.is_mainnet = network == Network::Mainnet;
                    match NetworkClient::new(&config, false) {
                        Ok(client) => info.network_client = Arc::new(client),
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
                self.settings_old_password = Zeroizing::new(v);
                Task::none()
            }
            Message::SettingsNewPasswordChanged(v) => {
                self.settings_new_password = Zeroizing::new(v);
                Task::none()
            }
            Message::SettingsNewPasswordConfirmChanged(v) => {
                self.settings_new_password_confirm = Zeroizing::new(v);
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
                self.loading = true;
                self.error_message = None;
                self.success_message = None;

                Task::perform(
                    async move {
                        Wallet::change_password(&path, &old_pw, &new_pw)?;
                        Ok(())
                    },
                    |r: Result<(), anyhow::Error>| {
                        Message::ChangePasswordCompleted(r.map_err(|e| e.to_string()))
                    },
                )
            }

            Message::ChangePasswordCompleted(result) => {
                self.loading = false;
                match result {
                    Ok(()) => {
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
        self.error_message = None;
        self.success_message = None;
        self.status_message = None;
        self.created_mnemonic = None;
        self.expanded_tx = None;
        self.validator_address.clear();
        self.stake_amount.clear();
        self.settings_old_password.zeroize();
        self.settings_new_password.zeroize();
        self.settings_new_password_confirm.zeroize();
    }

    pub(crate) fn validate_create_form(&self) -> Option<String> {
        if self.password.is_empty() {
            return Some("Password is required".into());
        }
        if self.password != self.password_confirm {
            return Some("Passwords don't match".into());
        }
        if self.wallet_name.trim().is_empty() {
            return Some("Wallet name is required".into());
        }
        None
    }

    fn compute_balance_history(&mut self) {
        let Some(current_balance) = self.balance else { return };
        if self.epoch_deltas.is_empty() { return; }

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
        self.loading = true;
        self.history_page = 0;

        let addr1 = info.address;
        let net1 = info.network_client.clone();
        let addr2 = info.address;
        let net2 = info.network_client.clone();
        let network = info.network_config.network;

        Task::batch([
            Task::perform(
                async move {
                    net1.balance(&addr1).await
                },
                |r: Result<u64, anyhow::Error>| {
                    Message::BalanceUpdated(r.map_err(|e| e.to_string()))
                },
            ),
            Task::perform(
                async move {
                    net2.sync_transactions(&addr2).await?;
                    // Cache ops are sync -- no await, no Send issue
                    let cache = TransactionCache::open()?;
                    let network_str = network.to_string();
                    let address_str = addr2.to_string();
                    let page = cache.query(&network_str, &address_str, &TransactionFilter::All, 25, 0)?;
                    let deltas = cache.query_epoch_deltas(&network_str, &address_str)?;
                    Ok((page.transactions, page.total, deltas))
                },
                |r: Result<(Vec<TransactionSummary>, u32, Vec<(u64, i64)>), anyhow::Error>| {
                    Message::TransactionsLoaded(r.map_err(|e| e.to_string()))
                },
            ),
        ])
    }

    fn load_history_page(&mut self) -> Task<Message> {
        let Some(info) = &self.wallet_info else {
            return Task::none();
        };
        let network_str = info.network_config.network.to_string();
        let address_str = info.address.to_string();
        let offset = self.history_page * 25;

        Task::perform(
            async move {
                let cache = TransactionCache::open()?;
                let page = cache.query(&network_str, &address_str, &TransactionFilter::All, 25, offset)?;
                Ok((page.transactions, page.total, Vec::new()))
            },
            |r: Result<(Vec<TransactionSummary>, u32, Vec<(u64, i64)>), anyhow::Error>| {
                Message::TransactionsLoaded(r.map_err(|e| e.to_string()))
            },
        )
    }

    fn load_stakes(&mut self) -> Task<Message> {
        let Some(info) = &self.wallet_info else {
            return Task::none();
        };
        self.loading = true;
        let addr = info.address;
        let net = info.network_client.clone();

        Task::perform(
            async move {
                net.get_stakes(&addr).await
            },
            |r: Result<Vec<StakedIotaSummary>, anyhow::Error>| {
                Message::StakesLoaded(r.map_err(|e| e.to_string()))
            },
        )
    }
}
