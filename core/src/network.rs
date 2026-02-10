/// Thin wrapper around the SDK's GraphQL client for network operations.
use anyhow::{Context, Result, bail};
use iota_sdk::crypto::ed25519::Ed25519PrivateKey;
use iota_sdk::crypto::IotaSigner;
use iota_sdk::graphql_client::faucet::FaucetClient;
use iota_sdk::graphql_client::pagination::PaginationFilter;
use iota_sdk::graphql_client::query_types::TransactionsFilter;
use iota_sdk::graphql_client::Client;
use iota_sdk::transaction_builder::TransactionBuilder;
use iota_sdk::transaction_builder::unresolved::Argument as UnresolvedArg;
use iota_sdk::types::{
    Address, Argument, Command as TxCommand, Digest, Input, ObjectId, Transaction,
    TransactionKind,
};

use crate::wallet::{Network, NetworkConfig};

pub struct NetworkClient {
    client: Client,
    network: Network,
    node_url: String,
}

impl NetworkClient {
    pub fn new(config: &NetworkConfig) -> Result<Self> {
        let (client, node_url) = match &config.network {
            Network::Testnet => (Client::new_testnet(), "https://graphql.testnet.iota.cafe".to_string()),
            Network::Mainnet => (Client::new_mainnet(), "https://graphql.mainnet.iota.cafe".to_string()),
            Network::Devnet => (Client::new_devnet(), "https://graphql.devnet.iota.cafe".to_string()),
            Network::Custom => {
                let url = config
                    .custom_url
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Custom network requires a node URL"))?;
                let c = Client::new(url)
                    .context("Failed to create client with custom URL")?;
                (c, url.clone())
            }
        };

        Ok(Self {
            client,
            network: config.network,
            node_url,
        })
    }

    /// Create a client pointed at an arbitrary GraphQL endpoint.
    pub fn new_custom(url: &str) -> Result<Self> {
        let client = Client::new(url)
            .context("Failed to create client with custom URL")?;
        Ok(Self {
            client,
            network: Network::Custom,
            node_url: url.to_string(),
        })
    }

    /// Query the IOTA balance for an address (in nanos).
    pub async fn balance(&self, address: &Address) -> Result<u64> {
        let balance = self
            .client
            .balance(*address, None)
            .await
            .context("Failed to query balance")?;
        Ok(balance.unwrap_or(0))
    }

    /// Dry-run, sign, and execute a built transaction.
    async fn sign_and_execute(
        &self,
        tx: &Transaction,
        private_key: &Ed25519PrivateKey,
    ) -> Result<TransferResult> {
        let dry_run = self
            .client
            .dry_run_tx(tx, false)
            .await
            .context("Dry run failed")?;
        if let Some(err) = dry_run.error {
            bail!("Transaction would fail: {err}");
        }

        let signature = private_key
            .sign_transaction(tx)
            .map_err(|e| anyhow::anyhow!("Failed to sign transaction: {e}"))?;

        let effects = self
            .client
            .execute_tx(&[signature], tx, None)
            .await
            .context("Failed to execute transaction")?;

        Ok(TransferResult {
            digest: effects.digest().to_string(),
            status: format!("{:?}", effects.status()),
            net_gas_usage: effects.gas_summary().net_gas_usage(),
        })
    }

    /// Send IOTA from the signer's address to a recipient.
    /// Amount is in nanos (1 IOTA = 1_000_000_000 nanos).
    pub async fn send_iota(
        &self,
        private_key: &Ed25519PrivateKey,
        sender: &Address,
        recipient: Address,
        amount: u64,
    ) -> Result<TransferResult> {
        let mut builder = TransactionBuilder::new(*sender).with_client(&self.client);
        builder.send_iota(recipient, amount);
        let tx = builder.finish().await.context("Failed to build transaction")?;
        self.sign_and_execute(&tx, private_key).await
    }

    /// Stake IOTA to a validator.
    /// Amount is in nanos (1 IOTA = 1_000_000_000 nanos).
    pub async fn stake_iota(
        &self,
        private_key: &Ed25519PrivateKey,
        sender: &Address,
        validator: Address,
        amount: u64,
    ) -> Result<TransferResult> {
        let mut builder = TransactionBuilder::new(*sender).with_client(&self.client);
        builder.stake(amount, validator);
        let tx = builder.finish().await.context("Failed to build stake transaction")?;
        self.sign_and_execute(&tx, private_key).await
    }

    /// Unstake a previously staked IOTA object.
    pub async fn unstake_iota(
        &self,
        private_key: &Ed25519PrivateKey,
        sender: &Address,
        staked_object_id: ObjectId,
    ) -> Result<TransferResult> {
        let mut builder = TransactionBuilder::new(*sender).with_client(&self.client);
        builder.unstake(staked_object_id);
        let tx = builder.finish().await.context("Failed to build unstake transaction")?;
        self.sign_and_execute(&tx, private_key).await
    }

    /// Query all StakedIota objects owned by the given address, including
    /// estimated rewards computed by the network.
    pub async fn get_stakes(&self, address: &Address) -> Result<Vec<StakedIotaSummary>> {
        let query = serde_json::json!({
            "query": r#"query ($owner: IotaAddress!) {
                address(address: $owner) {
                    stakedIotas {
                        nodes {
                            address
                            stakeStatus
                            activatedEpoch { epochId }
                            poolId
                            principal
                            estimatedReward
                        }
                    }
                }
            }"#,
            "variables": {
                "owner": address.to_string()
            }
        });

        let response = self
            .client
            .run_query_from_json(query.as_object().unwrap().clone())
            .await
            .context("Failed to query staked objects")?;

        let data = response.data.context("No data in staked IOTA response")?;
        let nodes = data
            .get("address")
            .and_then(|a| a.get("stakedIotas"))
            .and_then(|s| s.get("nodes"))
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();

        let mut stakes = Vec::new();
        for node in &nodes {
            let object_id = node
                .get("address")
                .and_then(|v| v.as_str())
                .and_then(|s| ObjectId::from_hex(s).ok());
            let pool_id = node
                .get("poolId")
                .and_then(|v| v.as_str())
                .and_then(|s| ObjectId::from_hex(s).ok());
            let principal = node
                .get("principal")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let stake_activation_epoch = node
                .get("activatedEpoch")
                .and_then(|v| v.get("epochId"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let estimated_reward = node
                .get("estimatedReward")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok());
            let status = match node.get("stakeStatus").and_then(|v| v.as_str()) {
                Some("ACTIVE") => StakeStatus::Active,
                Some("PENDING") => StakeStatus::Pending,
                _ => StakeStatus::Unstaked,
            };

            if let (Some(object_id), Some(pool_id)) = (object_id, pool_id) {
                stakes.push(StakedIotaSummary {
                    object_id,
                    pool_id,
                    principal,
                    stake_activation_epoch,
                    estimated_reward,
                    status,
                });
            }
        }

        Ok(stakes)
    }

    /// Request tokens from the faucet (testnet/devnet only).
    pub async fn faucet(&self, address: &Address) -> Result<()> {
        match &self.network {
            Network::Mainnet => bail!("Faucet is not available on mainnet"),
            Network::Testnet => {
                FaucetClient::new_testnet()
                    .request_and_wait(*address)
                    .await
                    .map_err(|e| anyhow::anyhow!("Faucet request failed: {e}"))?;
            }
            Network::Devnet => {
                FaucetClient::new_devnet()
                    .request_and_wait(*address)
                    .await
                    .map_err(|e| anyhow::anyhow!("Faucet request failed: {e}"))?;
            }
            Network::Custom => {
                bail!("Faucet is not available for custom networks. Use --testnet or --devnet.");
            }
        }
        Ok(())
    }

    /// Query recent transactions involving the given address.
    ///
    /// Always queries both sent and received to determine true direction,
    /// since outgoing txs can also appear in recv queries (change).
    pub async fn transactions(
        &self,
        address: &Address,
        filter: TransactionFilter,
    ) -> Result<Vec<TransactionSummary>> {
        let sent = self.query_transactions(
            TransactionsFilter {
                sign_address: Some(*address),
                ..Default::default()
            },
            TransactionDirection::Out,
        ).await?;
        let recv = self.query_transactions(
            TransactionsFilter {
                recv_address: Some(*address),
                ..Default::default()
            },
            TransactionDirection::In,
        ).await?;

        // Merge: sent takes priority (a tx you signed is "out" even if you also received change)
        let mut all = sent;
        for tx in recv {
            if !all.iter().any(|t| t.digest == tx.digest) {
                all.push(tx);
            }
        }
        all.sort_by(|a, b| {
            b.epoch.cmp(&a.epoch)
                .then(b.lamport_version.cmp(&a.lamport_version))
        });

        // Apply filter
        match filter {
            TransactionFilter::All => Ok(all),
            TransactionFilter::In => Ok(all.into_iter().filter(|t| t.direction == Some(TransactionDirection::In)).collect()),
            TransactionFilter::Out => Ok(all.into_iter().filter(|t| t.direction == Some(TransactionDirection::Out)).collect()),
        }
    }

    async fn query_transactions(
        &self,
        filter: TransactionsFilter,
        direction: TransactionDirection,
    ) -> Result<Vec<TransactionSummary>> {
        let page = self
            .client
            .transactions_data_effects(Some(filter), PaginationFilter::default())
            .await
            .context("Failed to query transactions")?;

        let summaries = page
            .data()
            .iter()
            .map(|item| {
                let digest = item.tx.transaction.digest().to_string();
                let (sender, amount) = match &item.tx.transaction {
                    Transaction::V1(v1) => {
                        let sender = Some(v1.sender.to_string());
                        let amount = extract_transfer_amount(&v1.kind);
                        (sender, amount)
                    }
                };
                let net = item.effects.gas_summary().net_gas_usage();
                let fee = if net > 0 { Some(net as u64) } else { None };
                let epoch = item.effects.epoch();
                let lamport_version = item.effects.as_v1().lamport_version;
                TransactionSummary {
                    digest,
                    direction: Some(direction),
                    timestamp: None,
                    sender,
                    amount,
                    fee,
                    epoch,
                    lamport_version,
                }
            })
            .collect();

        Ok(summaries)
    }

    /// Sweep the entire balance to a recipient address by transferring the gas
    /// coin directly. The network deducts gas from it; the recipient gets the
    /// rest. No dust remains with the sender.
    pub async fn sweep_all(
        &self,
        private_key: &Ed25519PrivateKey,
        sender: &Address,
        recipient: Address,
    ) -> Result<(TransferResult, u64)> {
        let balance = self.balance(sender).await?;
        if balance == 0 {
            bail!("Nothing to sweep — balance is 0.");
        }

        // Transfer the gas coin itself — the network deducts gas from it
        // and the recipient receives the remainder.
        let mut builder = TransactionBuilder::new(*sender).with_client(&self.client);
        builder.transfer_objects(recipient, [UnresolvedArg::Gas]);
        let tx = builder.finish().await.context("Failed to build sweep transaction")?;

        let result = self.sign_and_execute(&tx, private_key).await?;
        let amount = if result.net_gas_usage > 0 {
            balance.saturating_sub(result.net_gas_usage as u64)
        } else {
            balance
        };

        Ok((result, amount))
    }

    /// Look up a transaction by its digest, returning data and effects.
    pub async fn transaction_details(
        &self,
        digest: &Digest,
    ) -> Result<TransactionDetailsSummary> {
        let data_effects = self
            .client
            .transaction_data_effects(*digest)
            .await
            .context("Failed to query transaction")?
            .ok_or_else(|| anyhow::anyhow!("Transaction not found: {digest}"))?;

        let tx = &data_effects.tx.transaction;
        let effects = &data_effects.effects;

        let (sender, amount) = match tx {
            Transaction::V1(v1) => {
                let sender = v1.sender.to_string();
                let amount = extract_transfer_amount(&v1.kind);
                (sender, amount)
            }
        };

        let status = format!("{:?}", effects.status());
        let gas = effects.gas_summary();
        let net = gas.net_gas_usage();
        let fee = if net > 0 { Some(net as u64) } else { None };

        // Try to extract the recipient from the TransferObjects command
        let recipient = match tx {
            Transaction::V1(v1) => extract_transfer_recipient(&v1.kind),
        };

        Ok(TransactionDetailsSummary {
            digest: digest.to_string(),
            status,
            sender,
            recipient,
            amount,
            fee,
        })
    }

    /// Query all coin type balances for an address.
    pub async fn get_token_balances(&self, address: &Address) -> Result<Vec<TokenBalance>> {
        let query = serde_json::json!({
            "query": r#"query ($owner: IotaAddress!) {
                address(address: $owner) {
                    balances {
                        nodes {
                            coinType { repr }
                            coinObjectCount
                            totalBalance
                        }
                    }
                }
            }"#,
            "variables": {
                "owner": address.to_string()
            }
        });

        let response = self
            .client
            .run_query_from_json(query.as_object().unwrap().clone())
            .await
            .context("Failed to query token balances")?;

        let data = response.data.context("No data in balances response")?;
        let nodes = data
            .get("address")
            .and_then(|a| a.get("balances"))
            .and_then(|b| b.get("nodes"))
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();

        let mut balances = Vec::new();
        for node in &nodes {
            let coin_type = node
                .get("coinType")
                .and_then(|v| v.get("repr"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let coin_object_count = node
                .get("coinObjectCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let total_balance = node
                .get("totalBalance")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u128>().ok())
                .unwrap_or(0);

            balances.push(TokenBalance {
                coin_type,
                coin_object_count,
                total_balance,
            });
        }

        Ok(balances)
    }

    /// Query network status: current epoch, gas price, and node URL.
    pub async fn status(&self) -> Result<NetworkStatus> {
        let epoch = self
            .client
            .epoch(None)
            .await
            .context("Failed to query epoch")?
            .context("No epoch data available")?;

        let epoch_id = epoch.epoch_id;
        let reference_gas_price = epoch
            .reference_gas_price
            .and_then(|b| u64::try_from(b).ok())
            .unwrap_or(0);
        let node_url = self.node_url.clone();

        Ok(NetworkStatus {
            epoch: epoch_id,
            reference_gas_price,
            network: self.network,
            node_url,
        })
    }

    pub fn network(&self) -> &Network {
        &self.network
    }

    pub fn client(&self) -> &Client {
        &self.client
    }
}

pub struct TransferResult {
    pub digest: String,
    pub status: String,
    pub net_gas_usage: i64,
}

#[derive(Debug, Clone)]
pub struct NetworkStatus {
    pub epoch: u64,
    pub reference_gas_price: u64,
    pub network: Network,
    pub node_url: String,
}

#[derive(Debug, Clone)]
pub struct TransactionDetailsSummary {
    pub digest: String,
    pub status: String,
    pub sender: String,
    pub recipient: Option<String>,
    pub amount: Option<u64>,
    pub fee: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct StakedIotaSummary {
    pub object_id: ObjectId,
    pub pool_id: ObjectId,
    pub principal: u64,
    pub stake_activation_epoch: u64,
    pub estimated_reward: Option<u64>,
    pub status: StakeStatus,
}

#[derive(Debug, Clone)]
pub struct TokenBalance {
    pub coin_type: String,
    pub coin_object_count: u64,
    pub total_balance: u128,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StakeStatus {
    Active,
    Pending,
    Unstaked,
}

impl std::fmt::Display for StakeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Pending => write!(f, "pending"),
            Self::Unstaked => write!(f, "unstaked"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransactionFilter {
    All,
    In,
    Out,
}

impl std::str::FromStr for TransactionFilter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "in" => Ok(Self::In),
            "out" => Ok(Self::Out),
            "all" => Ok(Self::All),
            other => Err(format!(
                "Unknown transaction filter: '{other}'. Use 'in', 'out', or 'all'."
            )),
        }
    }
}

impl TransactionFilter {
    pub fn from_str_opt(s: Option<&str>) -> Self {
        s.and_then(|s| s.parse().ok()).unwrap_or(Self::All)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransactionDirection {
    In,
    Out,
}

impl std::fmt::Display for TransactionDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::In => write!(f, "in"),
            Self::Out => write!(f, "out"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransactionSummary {
    pub digest: String,
    pub direction: Option<TransactionDirection>,
    pub timestamp: Option<String>,
    pub sender: Option<String>,
    pub amount: Option<u64>,
    /// Net gas fee in nanos (computation + storage - rebate).
    pub fee: Option<u64>,
    /// Epoch in which this transaction was executed.
    pub epoch: u64,
    /// Lamport version — monotonically increasing, used for chronological sorting.
    pub lamport_version: u64,
}

/// Best-effort extraction of the transfer amount from a ProgrammableTransaction.
/// Works for standard SplitCoins-based IOTA transfers built by the SDK.
fn extract_transfer_amount(kind: &TransactionKind) -> Option<u64> {
    let ptb = kind.as_programmable_transaction_opt()?;
    for cmd in &ptb.commands {
        if let TxCommand::SplitCoins(split) = cmd {
            // Sum all split amounts (typically just one for simple transfers)
            let mut total: u64 = 0;
            for arg in &split.amounts {
                if let Argument::Input(idx) = arg {
                    if let Some(Input::Pure { value }) = ptb.inputs.get(*idx as usize) {
                        if value.len() == 8 {
                            let nanos = u64::from_le_bytes(value[..8].try_into().ok()?);
                            total = total.checked_add(nanos)?;
                        }
                    }
                }
            }
            if total > 0 {
                return Some(total);
            }
        }
    }
    None
}

/// Best-effort extraction of the transfer recipient from a ProgrammableTransaction.
/// Looks for TransferObjects commands with a pure address argument.
fn extract_transfer_recipient(kind: &TransactionKind) -> Option<String> {
    let ptb = kind.as_programmable_transaction_opt()?;
    for cmd in &ptb.commands {
        if let TxCommand::TransferObjects(transfer) = cmd {
            if let Argument::Input(idx) = &transfer.address {
                if let Some(Input::Pure { value }) = ptb.inputs.get(*idx as usize) {
                    if value.len() == 32 {
                        let addr = Address::new({
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(value);
                            arr
                        });
                        return Some(addr.to_string());
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_network_without_url_fails() {
        let config = NetworkConfig {
            network: Network::Custom,
            custom_url: None,
        };

        let result = NetworkClient::new(&config);
        assert!(result.is_err(), "Custom network without URL should fail");
        let err = result.err().expect("already checked is_err").to_string();
        assert!(
            err.contains("Custom network requires a node URL"),
            "error should mention missing URL, got: {err}"
        );
    }
}
