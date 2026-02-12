use anyhow::{Context, Result};
use iota_sdk::transaction_builder::{Shared, TransactionBuilder, res};
use iota_sdk::types::{ObjectId, TypeTag};

use super::NetworkClient;
use super::types::TransferResult;
use crate::signer::Signer;

/// Notarization package deployed on testnet.
pub const TESTNET_NOTARIZATION_PACKAGE: &str =
    "0xd0ea634d088f5fb5c47d1bfd0d915ae984d49d41771ea95a08928eeae44d8187";

/// Standard IOTA clock shared object (0x6).
const CLOCK_OBJECT_ID: ObjectId = {
    let mut bytes = [0u8; 32];
    bytes[31] = 6;
    ObjectId::new(bytes)
};

impl NetworkClient {
    /// Create a locked notarization on-chain.
    ///
    /// Builds a three-step PTB:
    ///   1. `notarization::new_state_from_string(message, metadata)` -> "state"
    ///   2. `timelock::none()` -> "lock"
    ///   3. `locked_notarization::create<0x1::string::String>(state, description, updatable_metadata, lock, clock)`
    pub async fn notarize(
        &self,
        signer: &dyn Signer,
        sender: &iota_sdk::types::Address,
        package_id: ObjectId,
        message: &str,
        description: Option<&str>,
    ) -> Result<TransferResult> {
        let string_type: TypeTag = "0x1::string::String"
            .parse()
            .context("Failed to parse String TypeTag")?;

        let mut builder = TransactionBuilder::new(*sender).with_client(&self.client);
        builder.gas_budget(50_000_000);

        // Step 1: create notarization state from string
        builder
            .move_call(package_id, "notarization", "new_state_from_string")
            .arguments((message.to_string(), Option::<String>::None))
            .name("state");

        // Step 2: create a TimeLock::None via timelock::none()
        builder
            .move_call(package_id, "timelock", "none")
            .name("lock");

        // Step 3: create the locked notarization
        builder
            .move_call(package_id, "locked_notarization", "create")
            .type_tags([string_type])
            .arguments((
                res("state"),
                description.map(|s| s.to_string()),
                Option::<String>::None, // updatable_metadata
                res("lock"),
                Shared(CLOCK_OBJECT_ID),
            ));

        let tx = builder
            .finish()
            .await
            .context("Failed to build notarization transaction")?;
        self.sign_and_execute(&tx, signer).await
    }
}
