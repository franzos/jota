use anyhow::{bail, Context, Result};
use iota_sdk::crypto::multisig::MultisigAggregator;
use iota_sdk::crypto::simple::SimpleVerifier;
use iota_sdk::crypto::Verifier;
use iota_sdk::graphql_client::WaitForTx;
use iota_sdk::types::{
    Address, MultisigCommittee, MultisigMemberPublicKey, MultisigMemberSignature, SimpleSignature,
    Transaction, UserSignature,
};

use super::formats::SignatureEntry;
use super::{CollectedSignature, ProposalStatus, TransactionProposal};
use crate::network::transfer::{extract_transfer_amount, extract_transfer_recipient};
use crate::network::NetworkClient;
use crate::network::TransferResult;
use crate::signer::Signer;

// ── Transaction building ───────────────────────────────────────────────

/// Build an unsigned IOTA transfer from a multisig address.
pub async fn build_transfer(
    network: &NetworkClient,
    sender: &Address,
    recipient: Address,
    amount: u64,
) -> Result<Transaction> {
    use iota_sdk::transaction_builder::TransactionBuilder;

    let mut builder = TransactionBuilder::new(*sender).with_client(network.client());
    builder.send_iota(recipient, amount);
    builder
        .finish()
        .await
        .context("Failed to build multisig transfer transaction")
}

// ── Signing ────────────────────────────────────────────────────────────

/// Sign a proposal with the local signer. Returns the member key and signature.
pub async fn sign_proposal(
    network: &NetworkClient,
    signer: &dyn Signer,
    tx_bytes: &[u8],
) -> Result<(MultisigMemberPublicKey, MultisigMemberSignature)> {
    let tx: Transaction = bcs::from_bytes(tx_bytes).context("Failed to deserialize transaction")?;

    let objects = network
        .fetch_input_objects(&tx)
        .await
        .context("Failed to fetch input objects for signing")?;
    let user_sig = signer.sign_transaction(&tx, &objects)?;

    extract_member_signature(user_sig)
}

fn extract_member_signature(
    sig: UserSignature,
) -> Result<(MultisigMemberPublicKey, MultisigMemberSignature)> {
    match sig {
        UserSignature::Simple(SimpleSignature::Ed25519 {
            signature,
            public_key,
        }) => Ok((
            MultisigMemberPublicKey::Ed25519(public_key),
            MultisigMemberSignature::Ed25519(signature),
        )),
        _ => bail!("Only Ed25519 local signing is supported for multisig in v1."),
    }
}

// ── Signature validation ───────────────────────────────────────────────

/// Validate an incoming signature against a proposal's transaction bytes.
///
/// Checks:
/// 1. The public key belongs to the committee
/// 2. The signature cryptographically verifies against tx_bytes
/// 3. No duplicate (same member hasn't already signed)
pub fn validate_signature(
    committee: &MultisigCommittee,
    tx_bytes: &[u8],
    member_key: &MultisigMemberPublicKey,
    member_sig: &MultisigMemberSignature,
    existing_signatures: &[CollectedSignature],
) -> Result<()> {
    // Check member is in committee
    if !committee
        .members()
        .iter()
        .any(|m| m.public_key() == member_key)
    {
        bail!("Signer is not a member of this multisig committee.");
    }

    // Check not duplicate
    if existing_signatures
        .iter()
        .any(|s| &s.member_key == member_key)
    {
        bail!("This member has already signed this proposal.");
    }

    // Cryptographic verification: reconstruct UserSignature::Simple and verify
    // against the transaction's signing digest using the SDK's SimpleVerifier.
    let tx: Transaction = bcs::from_bytes(tx_bytes).context("Failed to deserialize transaction")?;
    let signing_digest = tx.signing_digest();
    let simple_sig = reconstruct_simple_signature(member_key, member_sig)?;

    SimpleVerifier
        .verify(&signing_digest, &simple_sig)
        .map_err(|e| anyhow::anyhow!("Signature verification failed: {e}"))?;

    Ok(())
}

/// Reconstruct a `SimpleSignature` from member key + member signature.
fn reconstruct_simple_signature(
    key: &MultisigMemberPublicKey,
    sig: &MultisigMemberSignature,
) -> Result<SimpleSignature> {
    match (key, sig) {
        (MultisigMemberPublicKey::Ed25519(pk), MultisigMemberSignature::Ed25519(s)) => {
            Ok(SimpleSignature::Ed25519 {
                signature: *s,
                public_key: *pk,
            })
        }
        (MultisigMemberPublicKey::Secp256k1(pk), MultisigMemberSignature::Secp256k1(s)) => {
            Ok(SimpleSignature::Secp256k1 {
                signature: *s,
                public_key: *pk,
            })
        }
        (MultisigMemberPublicKey::Secp256r1(pk), MultisigMemberSignature::Secp256r1(s)) => {
            Ok(SimpleSignature::Secp256r1 {
                signature: *s,
                public_key: *pk,
            })
        }
        _ => bail!("Unsupported or mismatched key/signature scheme combination."),
    }
}

/// Reconstruct a `UserSignature::Simple` from member key + member signature.
fn reconstruct_user_signature(
    key: &MultisigMemberPublicKey,
    sig: &MultisigMemberSignature,
) -> Result<UserSignature> {
    reconstruct_simple_signature(key, sig).map(UserSignature::Simple)
}

// ── Aggregation + submission ───────────────────────────────────────────

/// Aggregate collected signatures and submit the multisig transaction.
/// Returns the transaction result, or marks the proposal as stale if dry-run fails.
pub async fn aggregate_and_submit(
    network: &NetworkClient,
    committee: &MultisigCommittee,
    proposal: &mut TransactionProposal,
) -> Result<TransferResult> {
    let tx: Transaction =
        bcs::from_bytes(&proposal.tx_bytes).context("Failed to deserialize transaction")?;

    // Aggregate signatures
    let mut aggregator = MultisigAggregator::new_with_transaction(committee.clone(), &tx);

    for sig in &proposal.signatures {
        let user_sig = reconstruct_user_signature(&sig.member_key, &sig.signature)?;
        aggregator
            .add_signature(user_sig)
            .map_err(|e| anyhow::anyhow!("Failed to add signature to aggregator: {e}"))?;
    }

    let aggregated = aggregator
        .finish()
        .map_err(|e| anyhow::anyhow!("Failed to aggregate signatures: {e}"))?;

    let multisig_sig = UserSignature::Multisig(aggregated);

    // Dry-run first
    let dry_run = network.client().dry_run_tx(&tx, false).await;
    match dry_run {
        Ok(result) => {
            if let Some(err) = result.error {
                proposal.status = ProposalStatus::Stale {
                    reason: format!("Dry-run failed: {err}"),
                };
                bail!("Transaction dry-run failed (objects may be stale): {err}");
            }
        }
        Err(e) => {
            proposal.status = ProposalStatus::Stale {
                reason: format!("Dry-run error: {e}"),
            };
            bail!("Dry-run failed: {e}");
        }
    }

    // Execute
    let effects = network
        .client()
        .execute_tx(&[multisig_sig], &tx, WaitForTx::Finalized)
        .await
        .context("Failed to execute multisig transaction")?;

    let tx_digest = effects.as_v1().transaction_digest;

    if let Some(error) = effects.status().error() {
        proposal.status = ProposalStatus::Failed {
            reason: format!("{error:?}"),
        };
        bail!("Multisig transaction failed: {error:?} (digest: {tx_digest})");
    }

    let result = TransferResult {
        digest: tx_digest.to_string(),
        status: format!("{:?}", effects.status()),
        net_gas_usage: effects.gas_summary().net_gas_usage(),
    };

    proposal.status = ProposalStatus::Submitted {
        digest: result.digest.clone(),
    };

    Ok(result)
}

// ── Transaction digest ─────────────────────────────────────────────────

/// Compute the hex-encoded signing digest of a transaction.
pub fn compute_tx_digest(tx_bytes: &[u8]) -> Result<String> {
    let tx: Transaction = bcs::from_bytes(tx_bytes).context("Failed to deserialize transaction")?;
    let digest = tx.signing_digest();
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

// ── Transaction description ────────────────────────────────────────────

pub struct TransactionDescription {
    pub sender: String,
    pub recipient: Option<String>,
    pub amount: Option<u64>,
    pub gas_budget: u64,
}

/// Decode transaction bytes to extract a human-readable description.
pub fn describe_transaction(tx_bytes: &[u8]) -> Result<TransactionDescription> {
    let tx: Transaction =
        bcs::from_bytes(tx_bytes).context("Failed to deserialize transaction bytes")?;
    let Transaction::V1(v1) = &tx;

    let amount = extract_transfer_amount(&v1.kind);
    let recipient = extract_transfer_recipient(&v1.kind);
    let gas_budget = v1.gas_payment.budget;
    let sender = v1.sender;

    Ok(TransactionDescription {
        sender: sender.to_string(),
        recipient,
        amount,
        gas_budget,
    })
}

// ── Signature merging ──────────────────────────────────────────────────

/// Merge new signatures from a list of `SignatureEntry` into an existing proposal.
/// Only adds signatures that pass validation and aren't already present.
/// Returns the number of newly added signatures.
pub fn merge_signatures(
    committee: &MultisigCommittee,
    proposal: &mut TransactionProposal,
    incoming: &[SignatureEntry],
) -> Result<usize> {
    let mut added = 0;
    for entry in incoming {
        // Skip if already have this signer
        if proposal
            .signatures
            .iter()
            .any(|s| s.member_key == entry.public_key)
        {
            continue;
        }

        // Validate the signature
        validate_signature(
            committee,
            &proposal.tx_bytes,
            &entry.public_key,
            &entry.signature,
            &proposal.signatures,
        )?;

        let signed_at = timestamp_now();
        proposal.signatures.push(CollectedSignature {
            member_key: entry.public_key.clone(),
            signature: entry.signature.clone(),
            signed_at,
        });
        added += 1;
    }

    // Update status if threshold met
    update_proposal_status(committee, proposal);

    Ok(added)
}

/// Check if collected signatures meet the threshold and update status.
pub fn update_proposal_status(committee: &MultisigCommittee, proposal: &mut TransactionProposal) {
    if proposal.status != ProposalStatus::Pending {
        return;
    }
    let collected_weight: u16 = proposal
        .signatures
        .iter()
        .filter_map(|s| {
            committee
                .members()
                .iter()
                .find(|m| m.public_key() == &s.member_key)
                .map(|m| m.weight() as u16)
        })
        .sum();
    if collected_weight >= committee.threshold() {
        proposal.status = ProposalStatus::Ready;
    }
}

pub fn timestamp_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Format a unix timestamp as ISO 8601 UTC.
pub fn format_timestamp(unix_secs: i64) -> String {
    if unix_secs < 0 {
        return "1970-01-01T00:00:00Z".to_string();
    }
    let secs = unix_secs as u64;
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Civil calendar date from days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
