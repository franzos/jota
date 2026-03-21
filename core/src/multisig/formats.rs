use anyhow::{bail, Context, Result};
use base64ct::{Base64, Encoding};
use iota_sdk::types::{
    MultisigCommittee, MultisigMember, MultisigMemberPublicKey, MultisigMemberSignature,
    Transaction,
};
use serde::{Deserialize, Serialize};

/// A member entry in the shareable file format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberEntry {
    pub public_key: MultisigMemberPublicKey,
    pub weight: u8,
    pub label: String,
}

/// `.jota-multisig` file format — describes a multisig committee for sharing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultisigFile {
    pub version: u32,
    #[serde(rename = "type")]
    pub file_type: String,
    pub network: String,
    pub members: Vec<MemberEntry>,
    pub threshold: u16,
}

impl MultisigFile {
    /// Validate the file contents against an expected network.
    pub fn validate(&self, expected_network: &str) -> Result<()> {
        if self.version != 1 {
            bail!(
                "Unsupported multisig file version {}. Expected 1.",
                self.version
            );
        }
        if self.file_type != "multisig-address" {
            bail!(
                "Invalid file type '{}'. Expected 'multisig-address'.",
                self.file_type
            );
        }
        if self.network != expected_network {
            bail!(
                "Network mismatch: file is for '{}', expected '{expected_network}'.",
                self.network
            );
        }
        let committee = self.to_committee()?;
        if !committee.is_valid() {
            bail!("Invalid multisig committee: check member weights and threshold.");
        }
        Ok(())
    }

    /// Construct a `MultisigCommittee` from the member entries.
    pub fn to_committee(&self) -> Result<MultisigCommittee> {
        if self.members.is_empty() {
            bail!("Multisig file has no members.");
        }
        let members: Vec<MultisigMember> = self
            .members
            .iter()
            .map(|m| MultisigMember::new(m.public_key.clone(), m.weight))
            .collect();
        Ok(MultisigCommittee::new(members, self.threshold))
    }
}

/// Signature entry within a proposal file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureEntry {
    pub public_key: MultisigMemberPublicKey,
    pub signature: MultisigMemberSignature,
    pub signed_at: String,
}

/// `.jota-proposal` file format — a transaction waiting for multisig signatures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalFile {
    pub version: u32,
    #[serde(rename = "type")]
    pub file_type: String,
    pub network: String,
    pub multisig: MultisigFile,
    pub tx_bytes: String,
    pub proposer: String,
    pub created_at: String,
    pub signatures: Vec<SignatureEntry>,
}

impl ProposalFile {
    /// Validate the proposal against an expected network.
    /// Checks structure, embedded multisig, and that the transaction sender matches
    /// the committee's derived address.
    pub fn validate(&self, expected_network: &str) -> Result<()> {
        if self.version != 1 {
            bail!(
                "Unsupported proposal file version {}. Expected 1.",
                self.version
            );
        }
        if self.file_type != "proposal" {
            bail!(
                "Invalid file type '{}'. Expected 'proposal'.",
                self.file_type
            );
        }
        if self.network != expected_network {
            bail!(
                "Network mismatch: file is for '{}', expected '{expected_network}'.",
                self.network
            );
        }
        self.multisig.validate(expected_network)?;

        // Verify the transaction sender matches the committee address
        let bytes =
            Base64::decode_vec(&self.tx_bytes).context("Invalid base64 in proposal tx_bytes")?;
        let tx: Transaction =
            bcs::from_bytes(&bytes).context("Failed to BCS-deserialize transaction")?;
        let sender = match &tx {
            Transaction::V1(v1) => v1.sender,
        };
        let committee = self.multisig.to_committee()?;
        let expected_sender = committee.derive_address();
        if sender != expected_sender {
            bail!(
                "Transaction sender {} does not match multisig address {}.",
                sender,
                expected_sender
            );
        }
        Ok(())
    }
}

/// `.jota-sig` file format — a single member's signature for a proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureFile {
    pub version: u32,
    #[serde(rename = "type")]
    pub file_type: String,
    pub multisig_address: String,
    pub tx_digest: String,
    pub public_key: MultisigMemberPublicKey,
    pub signature: MultisigMemberSignature,
    pub signed_at: String,
}

impl SignatureFile {
    /// Basic structural validation.
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!(
                "Unsupported signature file version {}. Expected 1.",
                self.version
            );
        }
        if self.file_type != "signature" {
            bail!(
                "Invalid file type '{}'. Expected 'signature'.",
                self.file_type
            );
        }
        Ok(())
    }
}
