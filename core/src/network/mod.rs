/// Thin wrapper around the SDK's GraphQL client for network operations.
mod history;
mod names;
mod staking;
mod transfer;
mod types;

pub use types::*;

use anyhow::{Context, Result, bail};
use iota_sdk::graphql_client::faucet::FaucetClient;
use iota_sdk::graphql_client::Client;
use iota_sdk::types::Address;

use crate::wallet::{Network, NetworkConfig};

pub struct NetworkClient {
    pub(super) client: Client,
    pub(super) network: Network,
    pub(super) node_url: String,
}

/// Reject non-HTTPS node URLs unless `allow_insecure` is set.
fn validate_node_url(url: &str, allow_insecure: bool) -> Result<()> {
    if url.starts_with("https://") {
        return Ok(());
    }
    if url.starts_with("http://") {
        if allow_insecure {
            return Ok(());
        }
        bail!("Refusing to connect over plain HTTP: {url}\nUse --insecure to allow unencrypted connections.");
    }
    bail!("Invalid node URL scheme: {url}\nExpected an https:// URL.");
}

impl NetworkClient {
    pub fn new(config: &NetworkConfig, allow_insecure: bool) -> Result<Self> {
        let (client, node_url) = match &config.network {
            Network::Testnet => (Client::new_testnet(), "https://graphql.testnet.iota.cafe".to_string()),
            Network::Mainnet => (Client::new_mainnet(), "https://graphql.mainnet.iota.cafe".to_string()),
            Network::Devnet => (Client::new_devnet(), "https://graphql.devnet.iota.cafe".to_string()),
            Network::Custom => {
                let url = config
                    .custom_url
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Custom network requires a node URL"))?;
                validate_node_url(url, allow_insecure)?;
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
    pub fn new_custom(url: &str, allow_insecure: bool) -> Result<Self> {
        validate_node_url(url, allow_insecure)?;
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
            .run_query_from_json(
                query.as_object()
                    .ok_or_else(|| anyhow::anyhow!("Expected JSON object for GraphQL query"))?
                    .clone(),
            )
            .await
            .context("Failed to query token balances")?;

        let data = response.data.context("No data in balances response")?;
        let empty = vec![];
        let nodes = data
            .get("address")
            .and_then(|a| a.get("balances"))
            .and_then(|b| b.get("nodes"))
            .and_then(|n| n.as_array())
            .unwrap_or(&empty);

        let mut balances = Vec::new();
        for node in nodes {
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

        let result = NetworkClient::new(&config, false);
        assert!(result.is_err(), "Custom network without URL should fail");
        let err = result.err().expect("already checked is_err").to_string();
        assert!(
            err.contains("Custom network requires a node URL"),
            "error should mention missing URL, got: {err}"
        );
    }

    #[test]
    fn rejects_http_url_without_insecure() {
        let config = NetworkConfig {
            network: Network::Custom,
            custom_url: Some("http://localhost:9125/graphql".to_string()),
        };
        let err = NetworkClient::new(&config, false).err().expect("should fail");
        assert!(err.to_string().contains("--insecure"));
    }

    #[test]
    fn accepts_http_url_with_insecure() {
        let config = NetworkConfig {
            network: Network::Custom,
            custom_url: Some("http://localhost:9125/graphql".to_string()),
        };
        // Should pass URL validation (may fail on connection, which is fine)
        if let Err(e) = NetworkClient::new(&config, true) {
            assert!(
                !e.to_string().contains("--insecure"),
                "should not reject due to HTTP scheme when --insecure is set"
            );
        }
    }

    #[test]
    fn rejects_invalid_url_scheme() {
        let config = NetworkConfig {
            network: Network::Custom,
            custom_url: Some("ftp://example.com/graphql".to_string()),
        };
        let err = NetworkClient::new(&config, false).err().expect("should fail");
        assert!(err.to_string().contains("Invalid node URL scheme"));
    }
}
