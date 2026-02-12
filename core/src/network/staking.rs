use anyhow::{Context, Result};
use iota_sdk::transaction_builder::TransactionBuilder;
use iota_sdk::types::{Address, ObjectId};

use super::NetworkClient;
use super::types::{StakeStatus, StakedIotaSummary, TransferResult};
use crate::signer::Signer;

impl NetworkClient {
    /// Stake IOTA to a validator.
    /// Amount is in nanos (1 IOTA = 1_000_000_000 nanos).
    pub async fn stake_iota(
        &self,
        signer: &dyn Signer,
        sender: &Address,
        validator: Address,
        amount: u64,
    ) -> Result<TransferResult> {
        let mut builder = TransactionBuilder::new(*sender).with_client(&self.client);
        builder.stake(amount, validator);
        let tx = builder.finish().await.context("Failed to build stake transaction")?;
        self.sign_and_execute(&tx, signer).await
    }

    /// Unstake a previously staked IOTA object.
    pub async fn unstake_iota(
        &self,
        signer: &dyn Signer,
        sender: &Address,
        staked_object_id: ObjectId,
    ) -> Result<TransferResult> {
        let mut builder = TransactionBuilder::new(*sender).with_client(&self.client);
        builder.unstake(staked_object_id);
        let tx = builder.finish().await.context("Failed to build unstake transaction")?;
        self.sign_and_execute(&tx, signer).await
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
            .run_query_from_json(
                query.as_object()
                    .ok_or_else(|| anyhow::anyhow!("Expected JSON object for GraphQL query"))?
                    .clone(),
            )
            .await
            .context("Failed to query staked objects")?;

        let data = response.data.context("No data in staked IOTA response")?;
        let empty = vec![];
        let nodes = data
            .get("address")
            .and_then(|a| a.get("stakedIotas"))
            .and_then(|s| s.get("nodes"))
            .and_then(|n| n.as_array())
            .unwrap_or(&empty);

        let mut stakes = Vec::new();
        for node in nodes {
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
}
