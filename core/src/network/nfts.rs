use anyhow::{Context, Result};
use iota_sdk::transaction_builder::TransactionBuilder;
use iota_sdk::types::{Address, ObjectId};

use super::types::{NftSummary, TransferResult};
use super::NetworkClient;
use crate::signer::Signer;

/// Max pages to fetch when listing owned objects for NFTs.
const MAX_PAGES: usize = 10;

impl NetworkClient {
    /// Query owned objects with Display metadata, filtering out Coin types.
    /// Paginates through up to MAX_PAGES of 50 objects each.
    pub async fn get_nfts(&self, address: &Address) -> Result<Vec<NftSummary>> {
        let mut nfts = Vec::new();
        let mut cursor: Option<String> = None;

        for _ in 0..MAX_PAGES {
            // address.objects returns MoveObjectConnection — nodes are MoveObject
            // directly, so display/contents are top-level fields (no asMoveObject).
            // GraphQL treats null optional args as absent, so one query handles both cases.
            let cursor_value = cursor
                .as_deref()
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null);

            let query = serde_json::json!({
                "query": r#"query ($owner: IotaAddress!, $cursor: String) {
                    address(address: $owner) {
                        objects(first: 50, after: $cursor) {
                            pageInfo { hasNextPage endCursor }
                            nodes {
                                address
                                contents {
                                    type { repr }
                                }
                                display {
                                    key
                                    value
                                }
                            }
                        }
                    }
                }"#,
                "variables": {
                    "owner": address.to_string(),
                    "cursor": cursor_value,
                }
            });

            let data = self
                .execute_query(query, "Failed to query owned objects")
                .await?;
            let objects = data.get("address").and_then(|a| a.get("objects"));

            let empty = vec![];
            let nodes = objects
                .and_then(|o| o.get("nodes"))
                .and_then(|n| n.as_array())
                .unwrap_or(&empty);

            for node in nodes {
                if let Some(nft) = Self::parse_nft_node(node) {
                    nfts.push(nft);
                }
            }

            // Check pagination
            let has_next = objects
                .and_then(|o| o.get("pageInfo"))
                .and_then(|p| p.get("hasNextPage"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if !has_next {
                break;
            }

            cursor = objects
                .and_then(|o| o.get("pageInfo"))
                .and_then(|p| p.get("endCursor"))
                .and_then(|v| v.as_str())
                .map(String::from);
        }

        Ok(nfts)
    }

    /// Parse a single MoveObject node into an NftSummary, if it qualifies.
    fn parse_nft_node(node: &serde_json::Value) -> Option<NftSummary> {
        let object_id = node
            .get("address")
            .and_then(|v| v.as_str())
            .and_then(|s| ObjectId::from_hex(s).ok())?;

        let object_type = node
            .get("contents")
            .and_then(|c| c.get("type"))
            .and_then(|t| t.get("repr"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Skip Coin objects — those are fungible tokens, not NFTs
        if object_type.starts_with("0x2::coin::Coin")
            || object_type.starts_with(
                "0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin",
            )
        {
            return None;
        }

        // Skip StakedIota objects
        if object_type.contains("::staking_pool::StakedIota") {
            return None;
        }

        // Parse Display fields
        let display_fields = node.get("display").and_then(|d| d.as_array());

        let (mut name, mut description, mut image_url) = (None, None, None);
        if let Some(fields) = display_fields {
            for field in fields {
                let key = field.get("key").and_then(|k| k.as_str()).unwrap_or("");
                let value = field
                    .get("value")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                match key {
                    "name" => name = value,
                    "description" => description = value,
                    "image_url" => image_url = value,
                    _ => {}
                }
            }
        }

        // Only include objects that have Display metadata (indicating they're NFT-like)
        if name.is_none() && description.is_none() && image_url.is_none() {
            return None;
        }

        Some(NftSummary {
            object_id,
            object_type,
            name,
            description,
            image_url,
        })
    }

    /// Transfer an owned object (NFT) to a recipient address.
    pub async fn send_nft(
        &self,
        signer: &dyn Signer,
        sender: &Address,
        object_id: ObjectId,
        recipient: Address,
    ) -> Result<TransferResult> {
        let mut builder = TransactionBuilder::new(*sender).with_client(&self.client);
        builder.transfer_objects(recipient, [object_id]);
        let tx = builder
            .finish()
            .await
            .context("Failed to build NFT transfer transaction")?;
        self.sign_and_execute(&tx, signer).await
    }
}
