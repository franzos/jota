use iota_sdk::types::Address;
use iota_wallet_core::network::NetworkClient;
use iota_wallet_core::signer::Signer;
use iota_wallet_core::wallet::{NetworkConfig, Wallet};
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
    Settings,
}

// -- Cloneable wallet info extracted after open/create --

#[derive(Clone)]
pub(crate) struct WalletInfo {
    pub(crate) address: Address,
    pub(crate) address_string: String,
    pub(crate) network_config: NetworkConfig,
    pub(crate) signer: Arc<dyn Signer>,
    pub(crate) network_client: Arc<NetworkClient>,
    pub(crate) is_mainnet: bool,
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
        Ok(Self {
            address: *wallet.address(),
            address_string: wallet.address().to_string(),
            network_config: wallet.network_config().clone(),
            signer: Arc::new(wallet.signer()),
            network_client: Arc::new(network_client),
            is_mainnet: wallet.is_mainnet(),
        })
    }
}
