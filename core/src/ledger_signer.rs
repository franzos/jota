/// Ledger hardware wallet signer — implements `Signer` using a connected Ledger device.
use anyhow::Result;
use base64ct::{Base64, Encoding};
use iota_sdk::types::{
    Address, Ed25519PublicKey, Object, SimpleSignature, Transaction, UserSignature,
};
use ledger_iota_rebased::{Bip32Path, LedgerIota, TransportType};

use crate::signer::{SignedMessage, Signer};

pub struct LedgerSigner {
    ledger: LedgerIota,
    path: Bip32Path,
    public_key: Ed25519PublicKey,
    address: Address,
}

impl LedgerSigner {
    /// Connect to a Ledger device and fetch the public key + address for the given derivation path.
    pub fn connect(path: Bip32Path) -> Result<Self> {
        let ledger = LedgerIota::new(&TransportType::NativeHID)
            .map_err(|e| anyhow::anyhow!("Failed to connect to Ledger: {e}"))?;

        let (public_key, address) = ledger
            .get_pubkey(&path)
            .map_err(|e| anyhow::anyhow!("Failed to get public key from Ledger: {e}"))?;

        Ok(Self {
            ledger,
            path,
            public_key,
            address,
        })
    }

    pub fn public_key(&self) -> &Ed25519PublicKey {
        &self.public_key
    }

    pub fn path(&self) -> &Bip32Path {
        &self.path
    }
}

impl Signer for LedgerSigner {
    fn sign_transaction(&self, tx: &Transaction, objects: &[Object]) -> Result<UserSignature> {
        // Prepend the intent prefix [0, 0, 0] (TransactionData intent).
        // The Ledger app expects IntentMessage<TransactionData> format.
        let mut bcs_bytes = vec![0x00, 0x00, 0x00];
        bcs_bytes.extend_from_slice(&tx.to_bcs());

        let ledger_objects: Vec<ledger_iota_rebased::ObjectData> = objects
            .iter()
            .cloned()
            .map(|o| ledger_iota_rebased::ObjectData::try_from(o))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Cannot prepare object data for Ledger clear signing: {e}"))?;

        let obj_ref = if ledger_objects.is_empty() {
            None
        } else {
            Some(ledger_objects.as_slice())
        };

        let signature = self
            .ledger
            .sign_tx(&bcs_bytes, &self.path, obj_ref)
            .map_err(|e| anyhow::anyhow!("Ledger signing failed: {e}"))?;

        Ok(UserSignature::Simple(SimpleSignature::Ed25519 {
            signature,
            public_key: self.public_key.clone(),
        }))
    }

    fn sign_message(&self, msg: &[u8]) -> Result<SignedMessage> {
        let signature = self
            .ledger
            .sign_message(msg, &self.path)
            .map_err(|e| anyhow::anyhow!("Ledger message signing failed: {e}"))?;

        let sig: &[u8; 64] = signature.as_ref();
        let pk: &[u8; 32] = self.public_key.as_ref();

        Ok(SignedMessage {
            message: String::from_utf8(msg.to_vec())
                .unwrap_or_else(|_| Base64::encode_string(msg)),
            signature: Base64::encode_string(sig),
            public_key: Base64::encode_string(pk),
            address: self.address.to_string(),
        })
    }

    fn address(&self) -> &Address {
        &self.address
    }

    fn verify_address(&self) -> Result<()> {
        self.ledger
            .verify_address(&self.path)
            .map_err(|e| anyhow::anyhow!("Address verification failed: {e}"))?;
        Ok(())
    }
}
