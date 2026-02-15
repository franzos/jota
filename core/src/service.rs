use std::collections::HashMap;
use std::sync::Arc;

use iota_sdk::types::{Address, Digest, ObjectId};

use crate::error::{Result, WalletError};
use crate::network::{
    CoinMeta, NetworkClient, NetworkStatus, NftSummary, StakedIotaSummary, TokenBalance,
    TransactionDetailsSummary, TransferResult,
};
use crate::recipient::{Recipient, ResolvedRecipient};
use crate::signer::{SignedMessage, Signer};

const COIN_META_CACHE_LIMIT: usize = 256;

pub struct WalletService {
    network: NetworkClient,
    signer: Arc<dyn Signer>,
    notarization_package: Option<ObjectId>,
    coin_meta_cache: tokio::sync::Mutex<HashMap<String, CoinMeta>>,
}

impl WalletService {
    pub fn new(network: NetworkClient, signer: Arc<dyn Signer>) -> Self {
        Self {
            network,
            signer,
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
        self.network.network_name()
    }

    pub fn signer(&self) -> &Arc<dyn Signer> {
        &self.signer
    }

    pub async fn balance(&self) -> Result<u64> {
        Ok(self.network.balance(self.signer.address()).await?)
    }

    pub async fn send(&self, recipient: Address, amount: u64) -> Result<TransferResult> {
        Ok(self
            .network
            .send_iota(
                self.signer.as_ref(),
                self.signer.address(),
                recipient,
                amount,
            )
            .await?)
    }

    pub async fn sweep_all(&self, recipient: Address) -> Result<(TransferResult, u64)> {
        Ok(self
            .network
            .sweep_all(self.signer.as_ref(), self.signer.address(), recipient)
            .await?)
    }

    pub async fn stake(&self, validator: Address, amount: u64) -> Result<TransferResult> {
        Ok(self
            .network
            .stake_iota(
                self.signer.as_ref(),
                self.signer.address(),
                validator,
                amount,
            )
            .await?)
    }

    pub async fn unstake(&self, staked_object_id: ObjectId) -> Result<TransferResult> {
        Ok(self
            .network
            .unstake_iota(
                self.signer.as_ref(),
                self.signer.address(),
                staked_object_id,
            )
            .await?)
    }

    pub async fn faucet(&self) -> Result<()> {
        Ok(self.network.faucet(self.signer.address()).await?)
    }

    pub async fn get_stakes(&self) -> Result<Vec<StakedIotaSummary>> {
        Ok(self.network.get_stakes(self.signer.address()).await?)
    }

    pub async fn get_token_balances(&self) -> Result<Vec<TokenBalance>> {
        Ok(self
            .network
            .get_token_balances(self.signer.address())
            .await?)
    }

    pub async fn get_nfts(&self) -> Result<Vec<NftSummary>> {
        Ok(self.network.get_nfts(self.signer.address()).await?)
    }

    pub async fn send_nft(
        &self,
        object_id: ObjectId,
        recipient: Address,
    ) -> Result<TransferResult> {
        Ok(self
            .network
            .send_nft(
                self.signer.as_ref(),
                self.signer.address(),
                object_id,
                recipient,
            )
            .await?)
    }

    pub async fn sync_transactions(&self) -> Result<()> {
        Ok(self
            .network
            .sync_transactions(self.signer.address())
            .await?)
    }

    pub async fn transaction_details(&self, digest: &Digest) -> Result<TransactionDetailsSummary> {
        Ok(self.network.transaction_details(digest).await?)
    }

    pub async fn status(&self) -> Result<NetworkStatus> {
        Ok(self.network.status().await?)
    }

    pub async fn resolve_recipient(&self, recipient: &Recipient) -> Result<ResolvedRecipient> {
        Ok(self.network.resolve_recipient(recipient).await?)
    }

    pub fn sign_message(&self, msg: &[u8]) -> Result<SignedMessage> {
        Ok(self.signer.sign_message(msg)?)
    }

    pub fn verify_address(&self) -> Result<()> {
        Ok(self.signer.verify_address()?)
    }

    pub fn reconnect_signer(&self) -> Result<()> {
        Ok(self.signer.reconnect()?)
    }

    pub fn notarization_package(&self) -> Option<ObjectId> {
        self.resolve_notarization_package()
    }

    /// Resolve the notarization package: explicit config > testnet default.
    fn resolve_notarization_package(&self) -> Option<ObjectId> {
        if self.notarization_package.is_some() {
            return self.notarization_package;
        }
        if self.network.network_name() == "testnet" {
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
            WalletError::InvalidState(
                "Notarization not configured. Set IOTA_NOTARIZATION_PKG_ID or use --notarization-package.".into(),
            )
        })?;
        Ok(self
            .network
            .notarize(
                self.signer.as_ref(),
                self.signer.address(),
                pkg,
                message,
                description,
            )
            .await?)
    }

    pub async fn default_iota_name(&self, address: &Address) -> Result<Option<String>> {
        Ok(self.network.default_iota_name(address).await?)
    }

    /// Resolve a token alias (e.g. "usdt") or full coin type to `CoinMeta`.
    /// Matches against the wallet's token balances, then fetches on-chain metadata.
    pub async fn resolve_coin_type(&self, alias: &str) -> Result<CoinMeta> {
        let key = alias.to_lowercase();

        // Check cache first
        {
            let cache = self.coin_meta_cache.lock().await;
            if let Some(meta) = cache.get(&key) {
                return Ok(meta.clone());
            }
        }

        // If it looks like a full coin type (contains "::"), try directly
        let coin_type = if alias.contains("::") {
            alias.to_string()
        } else {
            // Search wallet balances for a matching coin type
            let balances = self.get_token_balances().await?;
            let matches: Vec<_> = balances
                .iter()
                .filter(|b| {
                    let parts: Vec<&str> = b.coin_type.split("::").collect();
                    if let Some(name) = parts.last() {
                        name.to_lowercase() == key
                    } else {
                        false
                    }
                })
                .collect();
            match matches.len() {
                0 => {
                    return Err(WalletError::InvalidAmount(format!(
                        "No token matching '{alias}' found in wallet. Use 'tokens' to list available tokens."
                    )))
                }
                1 => matches[0].coin_type.clone(),
                _ => {
                    let types: Vec<&str> = matches.iter().map(|b| b.coin_type.as_str()).collect();
                    return Err(WalletError::InvalidAmount(format!(
                        "Multiple tokens match '{alias}': {}\nSpecify the full coin type instead.",
                        types.join(", ")
                    )));
                }
            }
        };

        let meta = self.network.coin_metadata(&coin_type).await?;

        // Cache under the normalized key (1 clone for the return value)
        {
            let mut cache = self.coin_meta_cache.lock().await;
            if cache.len() >= COIN_META_CACHE_LIMIT {
                cache.clear();
            }
            cache.insert(key, meta.clone());
        }

        Ok(meta)
    }

    /// Send a non-IOTA token to a recipient. Amount must be > 0; use
    /// `sweep_all_token` to transfer the entire balance.
    pub async fn send_token(
        &self,
        recipient: Address,
        coin_type: &str,
        amount: u64,
    ) -> Result<TransferResult> {
        if amount == 0 {
            return Err(WalletError::InvalidAmount(
                "Cannot send 0 tokens. Use sweep to transfer the entire balance.".into(),
            ));
        }
        Ok(self
            .network
            .send_token(
                self.signer.as_ref(),
                self.signer.address(),
                recipient,
                coin_type,
                amount,
            )
            .await?)
    }

    /// Sweep all of a specific token to a recipient.
    pub async fn sweep_all_token(
        &self,
        recipient: Address,
        coin_type: &str,
    ) -> Result<(TransferResult, u128)> {
        // Get the total balance for this token
        let balances = self.get_token_balances().await?;
        let token_balance = balances
            .iter()
            .find(|b| b.coin_type == coin_type)
            .ok_or_else(|| {
                WalletError::InsufficientBalance(format!(
                    "No balance found for token type '{coin_type}'"
                ))
            })?;
        let total = token_balance.total_balance;
        if total == 0 {
            return Err(WalletError::InsufficientBalance(
                "Nothing to sweep — token balance is 0.".into(),
            ));
        }

        let result = self
            .network
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
