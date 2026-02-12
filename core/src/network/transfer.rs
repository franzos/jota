use anyhow::{Context, Result, bail};
use iota_sdk::transaction_builder::TransactionBuilder;
use iota_sdk::transaction_builder::unresolved::Argument as UnresolvedArg;
use iota_sdk::types::{
    Address, Argument, Command as TxCommand, Input, Transaction, TransactionKind,
};

use super::NetworkClient;
use super::types::TransferResult;
use crate::signer::Signer;

impl NetworkClient {
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

        let signature = signer.sign_transaction(tx)?;

        let effects = self
            .client
            .execute_tx(&[signature], tx, None)
            .await
            .context("Failed to execute transaction")?;

        if let Some(error) = effects.status().error() {
            bail!("Transaction failed: {error:?} (digest: {})", effects.digest());
        }

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
        signer: &dyn Signer,
        sender: &Address,
        recipient: Address,
        amount: u64,
    ) -> Result<TransferResult> {
        let mut builder = TransactionBuilder::new(*sender).with_client(&self.client);
        builder.send_iota(recipient, amount);
        let tx = builder.finish().await.context("Failed to build transaction")?;
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
        let tx = builder.finish().await.context("Failed to build sweep transaction")?;

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
