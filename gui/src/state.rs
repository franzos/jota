use iota_wallet_core::Address;
use iota_wallet_core::network::NetworkClient;
use iota_wallet_core::service::WalletService;
use iota_wallet_core::wallet::{AccountRecord, NetworkConfig, Wallet};
use std::fmt;
use std::sync::Arc;

// -- Screens --

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Screen {
    // Wizard phase (no wallet loaded)
    WalletSelect,
    Unlock,
    Create,
    Recover,
    // Main phase (wallet loaded)
    Account,
    Send,
    Receive,
    History,
    Staking,
    Sign,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SignMode {
    Sign,
    Verify,
}

// -- Cloneable wallet info extracted after open/create --

#[derive(Clone)]
pub(crate) struct WalletInfo {
    pub(crate) address: Address,
    pub(crate) address_string: String,
    pub(crate) network_config: NetworkConfig,
    pub(crate) service: Arc<WalletService>,
    pub(crate) is_mainnet: bool,
    pub(crate) account_index: u64,
    pub(crate) known_accounts: Vec<AccountRecord>,
}

impl fmt::Debug for WalletInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WalletInfo")
            .field("address", &self.address)
            .field("is_mainnet", &self.is_mainnet)
            .finish_non_exhaustive()
    }
}

impl WalletInfo {
    pub(crate) fn from_wallet(wallet: &Wallet) -> anyhow::Result<Self> {
        let network_client = NetworkClient::new(wallet.network_config(), false)?;
        let service = WalletService::new(
            network_client,
            Arc::new(wallet.signer()),
            wallet.network_config().network.to_string(),
        );
        Ok(Self {
            address: *wallet.address(),
            address_string: wallet.address().to_string(),
            network_config: wallet.network_config().clone(),
            service: Arc::new(service),
            is_mainnet: wallet.is_mainnet(),
            account_index: wallet.account_index(),
            known_accounts: wallet.known_accounts().to_vec(),
        })
    }
}
