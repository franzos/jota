use anyhow::Result;
use iota_sdk::types::Address;

use super::NetworkClient;
use crate::recipient::{Recipient, ResolvedRecipient};

impl NetworkClient {
    /// Resolve an IOTA Name (e.g. `franz.iota`) to an address via GraphQL.
    pub async fn resolve_iota_name(&self, name: &str) -> Result<Address> {
        let query = serde_json::json!({
            "query": r#"query ($name: String!) {
                resolveIotaNamesAddress(name: $name) {
                    address
                }
            }"#,
            "variables": { "name": name }
        });

        let data = self.execute_query(query, "Failed to resolve IOTA name").await?;
        let addr_str = data
            .get("resolveIotaNamesAddress")
            .and_then(|v| v.get("address"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Could not resolve '{name}' — name not found"))?;

        Address::from_hex(addr_str)
            .map_err(|e| anyhow::anyhow!("Invalid address in name resolution: {e}"))
    }

    /// Look up the default IOTA Name for an address (reverse resolution).
    pub async fn default_iota_name(&self, address: &Address) -> Result<Option<String>> {
        let query = serde_json::json!({
            "query": r#"query ($addr: IotaAddress!) {
                address(address: $addr) {
                    iotaNamesDefaultName(format: DOT)
                }
            }"#,
            "variables": { "addr": address.to_string() }
        });

        let data = self.execute_query(query, "Failed to query default IOTA name").await?;
        let name = data
            .get("address")
            .and_then(|v| v.get("iotaNamesDefaultName"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Ok(name)
    }

    /// Resolve a `Recipient` to a concrete address + optional display name.
    pub async fn resolve_recipient(&self, recipient: &Recipient) -> Result<ResolvedRecipient> {
        match recipient {
            Recipient::Address(addr) => Ok(ResolvedRecipient {
                address: *addr,
                name: None,
            }),
            Recipient::Name(name) => {
                let address = self.resolve_iota_name(name).await?;
                Ok(ResolvedRecipient {
                    address,
                    name: Some(name.clone()),
                })
            }
        }
    }
}
