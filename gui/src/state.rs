use iota_wallet_core::network::NetworkClient;
use iota_wallet_core::service::WalletService;
use iota_wallet_core::wallet::{AccountRecord, NetworkConfig, Wallet};
use iota_wallet_core::Address;
use iota_wallet_core::ObjectId;
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
    #[cfg(feature = "hardware-wallets")]
    HardwareConnect,
    // Main phase (wallet loaded)
    Account,
    Send,
    Receive,
    History,
    Staking,
    Nfts,
    Sign,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SignMode {
    Sign,
    Verify,
    Notarize,
}

// -- Native messaging approval --

#[derive(Debug, Clone)]
pub(crate) struct PendingApproval {
    pub(crate) request_id: String,
    pub(crate) method: String,
    pub(crate) params: serde_json::Value,
    /// Human-readable description for the approval modal.
    pub(crate) summary: Option<String>,
    /// The requesting site's origin (e.g. "https://dapp.example.com").
    pub(crate) origin: String,
}

// -- Cloneable wallet info extracted after open/create --

#[derive(Clone)]
pub(crate) struct WalletInfo {
    pub(crate) address: Address,
    pub(crate) address_string: String,
    pub(crate) network_config: NetworkConfig,
    pub(crate) service: Arc<WalletService>,
    pub(crate) is_mainnet: bool,
    pub(crate) is_hardware: bool,
    pub(crate) hardware_kind: Option<iota_wallet_core::HardwareKind>,
    pub(crate) account_index: u64,
    pub(crate) known_accounts: Vec<AccountRecord>,
    /// Explicitly configured package (env var), not the resolved testnet default.
    pub(crate) notarization_package_config: Option<ObjectId>,
    /// Resolved package (config or testnet default) — used by UI to check availability.
    pub(crate) notarization_package: Option<ObjectId>,
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
        let signer: Arc<dyn iota_wallet_core::Signer> = Arc::new(wallet.signer()?);
        Self::from_wallet_with_signer(wallet, signer)
    }

    pub(crate) fn from_wallet_with_signer(
        wallet: &Wallet,
        signer: Arc<dyn iota_wallet_core::Signer>,
    ) -> anyhow::Result<Self> {
        let notarization_package = std::env::var("IOTA_NOTARIZATION_PKG_ID")
            .ok()
            .and_then(|s| ObjectId::from_hex(&s).ok());

        let network_client = NetworkClient::new(wallet.network_config(), false)?;
        let service = WalletService::new(network_client, signer)
            .with_notarization_package(notarization_package);

        let resolved_package = service.notarization_package();

        Ok(Self {
            address: *wallet.address(),
            address_string: wallet.address().to_string(),
            network_config: wallet.network_config().clone(),
            service: Arc::new(service),
            is_mainnet: wallet.is_mainnet(),
            is_hardware: wallet.is_hardware(),
            hardware_kind: wallet.hardware_kind(),
            account_index: wallet.account_index(),
            known_accounts: wallet.known_accounts().to_vec(),
            notarization_package_config: notarization_package,
            notarization_package: resolved_package,
        })
    }
}
