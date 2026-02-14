use iota_sdk::graphql_client::TransactionDataEffects;
use iota_sdk::types::{ObjectId, Transaction};

use super::transfer::extract_transfer_amount;
use crate::wallet::Network;

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

#[derive(Debug, Clone)]
pub struct CoinMeta {
    pub coin_type: String,
    pub symbol: String,
    pub decimals: u8,
    pub name: String,
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

#[derive(Debug, Clone)]
pub struct NftSummary {
    pub object_id: ObjectId,
    pub object_type: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
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

/// Build a `TransactionSummary` from a GraphQL `TransactionDataEffects` response.
pub fn transaction_summary_from_graphql(
    item: &TransactionDataEffects,
    direction: TransactionDirection,
) -> TransactionSummary {
    let digest = item.tx.transaction.digest().to_string();
    let (sender, amount) = match &item.tx.transaction {
        Transaction::V1(v1) => {
            let sender = Some(v1.sender.to_string());
            let amount = extract_transfer_amount(&v1.kind);
            (sender, amount)
        }
    };
    let net = item.effects.gas_summary().net_gas_usage();
    let fee = u64::try_from(net).ok().filter(|&f| f > 0);
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
}
