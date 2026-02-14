/// Ledger hardware wallet signer — implements `Signer` using a connected Ledger device.
use anyhow::Result;
use base64ct::{Base64, Encoding};
use iota_sdk::types::{
    Address, Ed25519PublicKey, Object, SimpleSignature, Transaction, UserSignature,
};
use ledger_iota_rebased::{Bip32Path, DeviceStatus, LedgerError, LedgerIota, TransportType};

use crate::signer::{SignedMessage, Signer};

pub struct LedgerSigner {
    ledger: LedgerIota,
    path: Bip32Path,
    public_key: Ed25519PublicKey,
    address: Address,
}

/// Map a `LedgerError` to a user-friendly `anyhow::Error`.
/// Preserves the library's messages for specific error variants
/// and only replaces opaque transport errors.
fn ledger_error_to_anyhow(e: LedgerError) -> anyhow::Error {
    match e {
        LedgerError::Transport(_) => {
            anyhow::anyhow!("Ledger not found. Plug it in and open the IOTA app.")
        }
        other => anyhow::anyhow!("{other}"),
    }
}

impl LedgerSigner {
    /// Connect to a Ledger device and fetch the public key + address for the given derivation path.
    pub fn connect(path: Bip32Path) -> Result<Self> {
        let ledger =
            LedgerIota::new(&TransportType::NativeHID).map_err(ledger_error_to_anyhow)?;

        let (public_key, address) = ledger.get_pubkey(&path).map_err(ledger_error_to_anyhow)?;

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

    /// Diagnose the device state after a failed operation and return
    /// an enriched error message with a recovery hint.
    fn enrich_error(&self, operation: &str, e: LedgerError) -> anyhow::Error {
        // User rejections and blind-signing errors are already clear
        if matches!(e, LedgerError::UserRejected | LedgerError::BlindSigningDisabled) {
            return anyhow::anyhow!("{e}");
        }
        match self.ledger.check_status() {
            DeviceStatus::Connected => anyhow::anyhow!("{operation} failed: {e}"),
            DeviceStatus::Locked => {
                anyhow::anyhow!(
                    "Ledger is locked. Enter your PIN and reopen the IOTA app, then reconnect."
                )
            }
            DeviceStatus::AppClosed => {
                anyhow::anyhow!("IOTA app was closed. Reopen it on your Ledger, then reconnect.")
            }
            DeviceStatus::WrongApp(name) => {
                anyhow::anyhow!(
                    "{name} is open instead of the IOTA app. Switch apps, then reconnect."
                )
            }
            DeviceStatus::Disconnected => {
                anyhow::anyhow!(
                    "Ledger disconnected. Plug it in, unlock, open the IOTA app, then reconnect."
                )
            }
        }
    }
}

/// Connect to a Ledger device and verify the derived address matches a stored address.
pub fn connect_and_verify(
    path: Bip32Path,
    stored_address: &Address,
) -> Result<LedgerSigner> {
    let signer = LedgerSigner::connect(path)?;
    if signer.address() != stored_address {
        anyhow::bail!(
            "Address mismatch. Device: {} Stored: {}. Wrong device or account?",
            signer.address(),
            stored_address
        );
    }
    Ok(signer)
}

/// Connect to a Ledger device and ask the user to verify the address on the device screen.
pub fn connect_with_verification(path: Bip32Path) -> Result<LedgerSigner> {
    let signer = LedgerSigner::connect(path)?;
    signer.verify_address()?;
    Ok(signer)
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
            .map_err(|e| self.enrich_error("Signing", e))?;

        Ok(UserSignature::Simple(SimpleSignature::Ed25519 {
            signature,
            public_key: self.public_key.clone(),
        }))
    }

    fn sign_message(&self, msg: &[u8]) -> Result<SignedMessage> {
        let signature = self
            .ledger
            .sign_message(msg, &self.path)
            .map_err(|e| self.enrich_error("Message signing", e))?;

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
            .map_err(|e| self.enrich_error("Address verification", e))?;
        Ok(())
    }

    fn reconnect(&self) -> Result<()> {
        self.ledger.reconnect().map_err(ledger_error_to_anyhow)
    }
}
