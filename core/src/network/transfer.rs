use anyhow::{bail, Context, Result};
use iota_sdk::graphql_client::pagination::PaginationFilter;
use iota_sdk::graphql_client::WaitForTx;
use iota_sdk::transaction_builder::unresolved::Argument as UnresolvedArg;
use iota_sdk::transaction_builder::TransactionBuilder;
use iota_sdk::types::{
    Address, Argument, Command as TxCommand, Input, Object, ObjectId, StructTag, Transaction,
    TransactionKind,
};

use super::types::TransferResult;
use super::NetworkClient;
use crate::signer::Signer;

impl NetworkClient {
    /// Fetch all objects referenced by a transaction (gas payment + PTB inputs).
    /// Used to provide coin metadata for hardware clear signing.
    async fn fetch_input_objects(&self, tx: &Transaction) -> Result<Vec<Object>> {
        let Transaction::V1(v1) = tx;

        let mut ids: Vec<(ObjectId, Option<u64>)> = Vec::new();

        for obj_ref in &v1.gas_payment.objects {
            ids.push((obj_ref.object_id, Some(obj_ref.version)));
        }

        if let Some(ptb) = v1.kind.as_programmable_transaction_opt() {
            for input in &ptb.inputs {
                match input {
                    Input::ImmutableOrOwned(r) | Input::Receiving(r) => {
                        ids.push((r.object_id, Some(r.version)));
                    }
                    Input::Shared { object_id, .. } => {
                        ids.push((*object_id, None));
                    }
                    Input::Pure { .. } => {}
                }
            }
        }

        let mut objects = Vec::new();
        for (id, version) in ids {
            if let Some(obj) = self.client.object(id, version).await? {
                objects.push(obj);
            }
        }
        Ok(objects)
    }

    /// Dry-run, sign, and execute a built transaction.
    pub(super) async fn sign_and_execute(
        &self,
        tx: &Transaction,
        signer: &dyn Signer,
    ) -> Result<TransferResult> {
        let dry_run = self
            .client
            .dry_run_tx(tx, false)
            .await
            .context("Dry run failed")?;
        if let Some(err) = dry_run.error {
            bail!("Transaction would fail: {err}");
        }

        let objects = self
            .fetch_input_objects(tx)
            .await
            .context("Failed to fetch input objects for signing")?;
        let signature = signer.sign_transaction(tx, &objects)?;

        let effects = self
            .client
            .execute_tx(&[signature], tx, WaitForTx::Finalized)
            .await
            .context("Failed to execute transaction")?;

        let tx_digest = effects.as_v1().transaction_digest;

        if let Some(error) = effects.status().error() {
            bail!("Transaction failed: {error:?} (digest: {tx_digest})");
        }

        Ok(TransferResult {
            digest: tx_digest.to_string(),
            status: format!("{:?}", effects.status()),
            net_gas_usage: effects.gas_summary().net_gas_usage(),
        })
    }

    /// Send IOTA from the signer's address to a recipient.
    /// Amount is in nanos (1 IOTA = 1_000_000_000 nanos).
    pub async fn send_iota(
        &self,
        signer: &dyn Signer,
        sender: &Address,
        recipient: Address,
        amount: u64,
    ) -> Result<TransferResult> {
        let mut builder = TransactionBuilder::new(*sender).with_client(&self.client);
        builder.send_iota(recipient, amount);
        let tx = builder
            .finish()
            .await
            .context("Failed to build transaction")?;
        self.sign_and_execute(&tx, signer).await
    }

    /// Sweep the entire balance to a recipient address by transferring the gas
    /// coin directly. The network deducts gas from it; the recipient gets the
    /// rest. No dust remains with the sender.
    pub async fn sweep_all(
        &self,
        signer: &dyn Signer,
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
        let tx = builder
            .finish()
            .await
            .context("Failed to build sweep transaction")?;

        let result = self.sign_and_execute(&tx, signer).await?;
        // net_gas_usage is signed: positive = gas consumed, negative = rebate.
        // Recipient gets balance minus gas consumed (or plus rebate).
        let amount = if result.net_gas_usage > 0 {
            balance.saturating_sub(result.net_gas_usage.unsigned_abs())
        } else {
            balance.saturating_add(result.net_gas_usage.unsigned_abs())
        };

        Ok((result, amount))
    }

    /// Send a non-IOTA token. When `amount` is 0, transfers all coin objects
    /// (sweep). Otherwise merges coins and splits the requested amount.
    pub async fn send_token(
        &self,
        signer: &dyn Signer,
        sender: &Address,
        recipient: Address,
        coin_type: &str,
        amount: u64,
    ) -> Result<TransferResult> {
        let struct_tag: StructTag = coin_type
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid coin type '{coin_type}': {e}"))?;

        // Collect all coin ObjectIds for this type
        let mut coin_ids: Vec<ObjectId> = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let page = self
                .client
                .coins(
                    *sender,
                    struct_tag.clone(),
                    PaginationFilter {
                        cursor: cursor.clone(),
                        ..Default::default()
                    },
                )
                .await
                .context("Failed to query token coins")?;
            coin_ids.extend(page.data().iter().map(|c| *c.id()));
            if !page.page_info().has_next_page {
                break;
            }
            cursor = page.page_info().end_cursor.clone();
        }

        if coin_ids.is_empty() {
            bail!("No {coin_type} coins found in wallet.");
        }

        let mut builder = TransactionBuilder::new(*sender).with_client(&self.client);
        if amount == 0 {
            // Sweep — transfer all coins without splitting
            builder.send_coins::<_, u64>(coin_ids, recipient, None);
        } else {
            builder.send_coins(coin_ids, recipient, amount);
        }

        let tx = builder
            .finish()
            .await
            .context("Failed to build token transfer transaction")?;
        self.sign_and_execute(&tx, signer).await
    }
}

/// Best-effort extraction of the transfer amount from a ProgrammableTransaction.
/// Works for standard SplitCoins-based IOTA transfers built by the SDK.
pub(super) fn extract_transfer_amount(kind: &TransactionKind) -> Option<u64> {
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
pub(super) fn extract_transfer_recipient(kind: &TransactionKind) -> Option<String> {
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
