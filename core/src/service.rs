use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Result, bail};
use iota_sdk::types::{Address, Digest, ObjectId};

use crate::network::{
    CoinMeta, NetworkClient, NetworkStatus, StakedIotaSummary, TokenBalance,
    TransactionDetailsSummary, TransferResult,
};
use crate::recipient::{Recipient, ResolvedRecipient};
use crate::signer::{Signer, SignedMessage};

pub struct WalletService {
    network: NetworkClient,
    signer: Arc<dyn Signer>,
    network_name: String,
    notarization_package: Option<ObjectId>,
    coin_meta_cache: tokio::sync::Mutex<HashMap<String, CoinMeta>>,
}

impl WalletService {
    pub fn new(
        network: NetworkClient,
        signer: Arc<dyn Signer>,
        network_name: String,
    ) -> Self {
        Self {
            network,
            signer,
            network_name,
            notarization_package: None,
            coin_meta_cache: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn with_notarization_package(mut self, package: Option<ObjectId>) -> Self {
        self.notarization_package = package;
        self
    }

    pub fn address(&self) -> &Address {
        self.signer.address()
    }

    pub fn network_name(&self) -> &str {
        &self.network_name
    }

    pub fn signer(&self) -> &Arc<dyn Signer> {
        &self.signer
    }

    pub async fn balance(&self) -> Result<u64> {
        self.network.balance(self.signer.address()).await
    }

    pub async fn send(&self, recipient: Address, amount: u64) -> Result<TransferResult> {
        self.network
            .send_iota(self.signer.as_ref(), self.signer.address(), recipient, amount)
            .await
    }

    pub async fn sweep_all(&self, recipient: Address) -> Result<(TransferResult, u64)> {
        self.network
            .sweep_all(self.signer.as_ref(), self.signer.address(), recipient)
            .await
    }

    pub async fn stake(&self, validator: Address, amount: u64) -> Result<TransferResult> {
        self.network
            .stake_iota(self.signer.as_ref(), self.signer.address(), validator, amount)
            .await
    }

    pub async fn unstake(&self, staked_object_id: ObjectId) -> Result<TransferResult> {
        self.network
            .unstake_iota(self.signer.as_ref(), self.signer.address(), staked_object_id)
            .await
    }

    pub async fn faucet(&self) -> Result<()> {
        self.network.faucet(self.signer.address()).await
    }

    pub async fn get_stakes(&self) -> Result<Vec<StakedIotaSummary>> {
        self.network.get_stakes(self.signer.address()).await
    }

    pub async fn get_token_balances(&self) -> Result<Vec<TokenBalance>> {
        self.network.get_token_balances(self.signer.address()).await
    }

    pub async fn sync_transactions(&self) -> Result<()> {
        self.network.sync_transactions(self.signer.address()).await
    }

    pub async fn transaction_details(&self, digest: &Digest) -> Result<TransactionDetailsSummary> {
        self.network.transaction_details(digest).await
    }

    pub async fn status(&self) -> Result<NetworkStatus> {
        self.network.status().await
    }

    pub async fn resolve_recipient(&self, recipient: &Recipient) -> Result<ResolvedRecipient> {
        self.network.resolve_recipient(recipient).await
    }

    pub fn sign_message(&self, msg: &[u8]) -> Result<SignedMessage> {
        self.signer.sign_message(msg)
    }

    pub fn notarization_package(&self) -> Option<ObjectId> {
        self.resolve_notarization_package()
    }

    /// Resolve the notarization package: explicit config > testnet default.
    fn resolve_notarization_package(&self) -> Option<ObjectId> {
        if self.notarization_package.is_some() {
            return self.notarization_package;
        }
        if self.network_name == "testnet" {
            ObjectId::from_hex(crate::network::TESTNET_NOTARIZATION_PACKAGE).ok()
        } else {
            None
        }
    }

    pub async fn notarize(
        &self,
        message: &str,
        description: Option<&str>,
    ) -> Result<TransferResult> {
        let pkg = self.resolve_notarization_package().ok_or_else(|| {
            anyhow::anyhow!(
                "Notarization not configured. Set IOTA_NOTARIZATION_PKG_ID or use --notarization-package."
            )
        })?;
        self.network
            .notarize(self.signer.as_ref(), self.signer.address(), pkg, message, description)
            .await
    }

    pub async fn default_iota_name(&self, address: &Address) -> Result<Option<String>> {
        self.network.default_iota_name(address).await
    }

    /// Resolve a token alias (e.g. "usdt") or full coin type to `CoinMeta`.
    /// Matches against the wallet's token balances, then fetches on-chain metadata.
    pub async fn resolve_coin_type(&self, alias: &str) -> Result<CoinMeta> {
        let lower = alias.to_lowercase();

        // Check cache first
        {
            let cache = self.coin_meta_cache.lock().await;
            if let Some(meta) = cache.get(&lower) {
                return Ok(meta.clone());
            }
        }

        // If it looks like a full coin type (contains "::"), try directly
        let coin_type = if alias.contains("::") {
            alias.to_string()
        } else {
            // Search wallet balances for a matching coin type
            let balances = self.get_token_balances().await?;
            let matches: Vec<_> = balances.iter().filter(|b| {
                let parts: Vec<&str> = b.coin_type.split("::").collect();
                if let Some(name) = parts.last() {
                    name.to_lowercase() == lower
                } else {
                    false
                }
            }).collect();
            match matches.len() {
                0 => bail!(
                    "No token matching '{alias}' found in wallet. Use 'tokens' to list available tokens."
                ),
                1 => matches[0].coin_type.clone(),
                _ => {
                    let types: Vec<&str> = matches.iter().map(|b| b.coin_type.as_str()).collect();
                    bail!(
                        "Multiple tokens match '{alias}': {}\nSpecify the full coin type instead.",
                        types.join(", ")
                    )
                }
            }
        };

        let meta = self.network.coin_metadata(&coin_type).await?;

        // Cache the result under both the alias and the full coin type
        {
            let mut cache = self.coin_meta_cache.lock().await;
            cache.insert(lower, meta.clone());
            cache.insert(meta.coin_type.to_lowercase(), meta.clone());
        }

        Ok(meta)
    }

    /// Send a non-IOTA token to a recipient.
    pub async fn send_token(
        &self,
        recipient: Address,
        coin_type: &str,
        amount: u64,
    ) -> Result<TransferResult> {
        self.network
            .send_token(self.signer.as_ref(), self.signer.address(), recipient, coin_type, amount)
            .await
    }

    /// Sweep all of a specific token to a recipient.
    pub async fn sweep_all_token(
        &self,
        recipient: Address,
        coin_type: &str,
    ) -> Result<(TransferResult, u128)> {
        // Get the total balance for this token
        let balances = self.get_token_balances().await?;
        let token_balance = balances.iter().find(|b| b.coin_type == coin_type)
            .ok_or_else(|| anyhow::anyhow!("No balance found for token type '{coin_type}'"))?;
        let total = token_balance.total_balance;
        if total == 0 {
            bail!("Nothing to sweep — token balance is 0.");
        }

        let result = self.network
            .send_token(
                self.signer.as_ref(),
                self.signer.address(),
                recipient,
                coin_type,
                // For sweep, we send all coins without specifying an amount (transfer all objects)
                0,
            )
            .await?;

        Ok((result, total))
    }
}
