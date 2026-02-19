use anyhow::{Context, Result};
use iota_sdk::transaction_builder::TransactionBuilder;
use iota_sdk::types::{Address, ObjectId};

use super::types::{StakeStatus, StakedIotaSummary, TransferResult, ValidatorSummary};
use super::NetworkClient;
use crate::signer::Signer;

/// Extract a string field from a JSON value and parse it via `FromStr`.
fn json_str_field<T: std::str::FromStr>(node: &serde_json::Value, key: &str) -> Option<T> {
    node.get(key)
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
}

/// Extract a hex-encoded ObjectId from a JSON value.
fn json_object_id(node: &serde_json::Value, key: &str) -> Option<ObjectId> {
    node.get(key)
        .and_then(|v| v.as_str())
        .and_then(|s| ObjectId::from_hex(s).ok())
}

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
        let tx = builder
            .finish()
            .await
            .context("Failed to build stake transaction")?;
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
        let tx = builder
            .finish()
            .await
            .context("Failed to build unstake transaction")?;
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

        let data = self
            .execute_query(query, "Failed to query staked objects")
            .await?;
        let nodes = data
            .get("address")
            .and_then(|a| a.get("stakedIotas"))
            .and_then(|s| s.get("nodes"))
            .and_then(|n| n.as_array())
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        let mut stakes = Vec::new();
        for node in nodes {
            let object_id = json_object_id(node, "address");
            let pool_id = json_object_id(node, "poolId");
            let principal = json_str_field::<u64>(node, "principal").unwrap_or(0);
            let stake_activation_epoch = node
                .get("activatedEpoch")
                .and_then(|v| v.get("epochId"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let estimated_reward = json_str_field::<u64>(node, "estimatedReward");
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
                    validator_name: None,
                });
            }
        }

        Ok(stakes)
    }

    /// Fetch the full list of active validators from the current epoch,
    /// paginating in batches of 50.
    pub async fn get_validators(&self) -> Result<Vec<ValidatorSummary>> {
        let mut validators = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let after = match &cursor {
                Some(c) => format!(r#", after: "{}""#, c),
                None => String::new(),
            };

            let query_str = format!(
                r#"query {{
                    epoch {{
                        epochId
                        validatorSet {{
                            activeValidators(first: 50{after}) {{
                                pageInfo {{ hasNextPage endCursor }}
                                nodes {{
                                    address {{ address }}
                                    name
                                    stakingPoolId
                                    stakingPoolActivationEpoch
                                    commissionRate
                                    apy
                                    stakingPoolIotaBalance
                                    imageUrl
                                }}
                            }}
                        }}
                    }}
                }}"#
            );

            let query = serde_json::json!({ "query": query_str });
            let data = self
                .execute_query(query, "Failed to query validators")
                .await?;

            let epoch_data = data.get("epoch");
            let current_epoch = epoch_data
                .and_then(|e| e.get("epochId"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let active = epoch_data
                .and_then(|e| e.get("validatorSet"))
                .and_then(|vs| vs.get("activeValidators"));

            let nodes = active
                .and_then(|a| a.get("nodes"))
                .and_then(|n| n.as_array())
                .map(|v| v.as_slice())
                .unwrap_or(&[]);

            for node in nodes {
                let address = node
                    .get("address")
                    .and_then(|a| a.get("address"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = node
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let staking_pool_id = json_object_id(node, "stakingPoolId");
                let activation_epoch = node
                    .get("stakingPoolActivationEpoch")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let commission_rate = node
                    .get("commissionRate")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let apy = node.get("apy").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let staking_pool_iota_balance =
                    json_str_field::<u64>(node, "stakingPoolIotaBalance").unwrap_or(0);
                let image_url = node
                    .get("imageUrl")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let age_epochs = current_epoch.saturating_sub(activation_epoch);

                if let Some(staking_pool_id) = staking_pool_id {
                    validators.push(ValidatorSummary {
                        address,
                        name,
                        staking_pool_id,
                        commission_rate,
                        apy,
                        staking_pool_iota_balance,
                        image_url,
                        age_epochs,
                    });
                }
            }

            let has_next = active
                .and_then(|a| a.get("pageInfo"))
                .and_then(|pi| pi.get("hasNextPage"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if has_next {
                cursor = active
                    .and_then(|a| a.get("pageInfo"))
                    .and_then(|pi| pi.get("endCursor"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            } else {
                break;
            }
        }

        validators.sort_by(|a, b| {
            b.age_epochs.cmp(&a.age_epochs).then(
                b.staking_pool_iota_balance
                    .cmp(&a.staking_pool_iota_balance),
            )
        });

        Ok(validators)
    }
}
