pub mod formats;
pub mod signing;
pub mod store;

pub use formats::{MultisigFile, ProposalFile, SignatureFile};
pub use signing::{
    aggregate_and_submit, build_transfer, compute_tx_digest, describe_transaction,
    format_timestamp, merge_signatures, sign_proposal, timestamp_now, update_proposal_status,
    validate_signature, TransactionDescription,
};
pub use store::MultisigStore;

use crate::wallet::Network;
use iota_sdk::types::{
    Address, Ed25519PublicKey, MultisigCommittee, MultisigMemberPublicKey, MultisigMemberSignature,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultisigConfig {
    pub name: String,
    pub committee: MultisigCommittee,
    pub labels: Vec<String>,
    pub network: Network,
    pub my_key: Option<MultisigMemberPublicKey>,
}

impl MultisigConfig {
    pub fn address(&self) -> Address {
        self.committee.derive_address()
    }

    /// Try to identify which committee member belongs to the local signer.
    /// Returns `true` if a matching Ed25519 member was found and `my_key` was set.
    pub fn detect_my_key(&mut self, signer_public_key_bytes: &[u8]) -> bool {
        let Ok(bytes) = <[u8; 32]>::try_from(signer_public_key_bytes) else {
            return false;
        };
        let pk = Ed25519PublicKey::new(bytes);
        let member_pk = MultisigMemberPublicKey::Ed25519(pk);
        if self
            .committee
            .members()
            .iter()
            .any(|m| m.public_key() == &member_pk)
        {
            self.my_key = Some(member_pk);
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionProposal {
    pub tx_digest: String,
    pub multisig_address: String,
    pub tx_bytes: Vec<u8>,
    pub proposer: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub signatures: Vec<CollectedSignature>,
    pub status: ProposalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectedSignature {
    pub member_key: MultisigMemberPublicKey,
    pub signature: MultisigMemberSignature,
    pub signed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProposalStatus {
    Pending,
    Ready,
    Submitted { digest: String },
    Failed { reason: String },
    Stale { reason: String },
    Cancelled,
}
