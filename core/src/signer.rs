/// Signing abstraction that decouples transaction signing from a concrete key type.
///
use anyhow::Result;
use base64ct::{Base64, Encoding};
use iota_sdk::crypto::ed25519::{Ed25519PrivateKey, Ed25519VerifyingKey};
use iota_sdk::crypto::{IotaSigner, IotaVerifier};
use iota_sdk::types::{
    Address, PersonalMessage, SimpleSignature, Transaction, UserSignature,
};

pub trait Signer: Send + Sync {
    /// Sign a fully-built transaction and return the user signature.
    fn sign_transaction(&self, tx: &Transaction) -> Result<UserSignature>;

    /// Sign an arbitrary message and return the signed result.
    fn sign_message(&self, msg: &[u8]) -> Result<SignedMessage>;

    /// The on-chain address controlled by this signer.
    fn address(&self) -> &Address;
}

/// Result of signing an arbitrary message.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SignedMessage {
    pub message: String,
    pub signature: String,
    pub public_key: String,
    pub address: String,
}

/// Verify a signed message given the raw message bytes, base64 signature, and base64 public key.
/// Returns `Ok(true)` if valid, `Ok(false)` if the signature doesn't match.
pub fn verify_message(msg: &[u8], signature_b64: &str, public_key_b64: &str) -> Result<bool> {
    let sig_bytes = Base64::decode_vec(signature_b64)
        .map_err(|e| anyhow::anyhow!("Invalid base64 signature: {e}"))?;
    let pk_bytes = Base64::decode_vec(public_key_b64)
        .map_err(|e| anyhow::anyhow!("Invalid base64 public key: {e}"))?;

    let sig_array: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("Signature must be 64 bytes, got {}", sig_bytes.len()))?;
    let pk_array: [u8; 32] = pk_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("Public key must be 32 bytes, got {}", pk_bytes.len()))?;

    let signature = iota_sdk::types::Ed25519Signature::new(sig_array);
    let public_key = iota_sdk::types::Ed25519PublicKey::new(pk_array);

    let user_sig = UserSignature::Simple(SimpleSignature::Ed25519 {
        signature,
        public_key: public_key.clone(),
    });
    let personal_msg = PersonalMessage(msg.into());

    let verifier = Ed25519VerifyingKey::new(&public_key)
        .map_err(|e| anyhow::anyhow!("Invalid public key: {e}"))?;

    match verifier.verify_personal_message(&personal_msg, &user_sig) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Software signer backed by an in-memory Ed25519 private key.
pub struct SoftwareSigner {
    private_key: Ed25519PrivateKey,
    address: Address,
}

impl SoftwareSigner {
    pub fn new(private_key: Ed25519PrivateKey) -> Self {
        let address = private_key.public_key().derive_address();
        Self {
            private_key,
            address,
        }
    }
}

impl Signer for SoftwareSigner {
    fn sign_transaction(&self, tx: &Transaction) -> Result<UserSignature> {
        self.private_key
            .sign_transaction(tx)
            .map_err(|e| anyhow::anyhow!("Failed to sign transaction: {e}"))
    }

    fn sign_message(&self, msg: &[u8]) -> Result<SignedMessage> {
        let personal_msg = PersonalMessage(msg.into());
        let user_sig = self
            .private_key
            .sign_personal_message(&personal_msg)
            .map_err(|e| anyhow::anyhow!("Failed to sign message: {e}"))?;

        let (sig_bytes, pk_bytes) = match &user_sig {
            UserSignature::Simple(SimpleSignature::Ed25519 {
                signature,
                public_key,
            }) => {
                let sig: &[u8; 64] = signature.as_ref();
                let pk: &[u8; 32] = public_key.as_ref();
                (sig.to_vec(), pk.to_vec())
            }
            _ => anyhow::bail!("Unexpected signature type from Ed25519 key"),
        };

        Ok(SignedMessage {
            message: String::from_utf8(msg.to_vec())
                .unwrap_or_else(|_| Base64::encode_string(msg)),
            signature: Base64::encode_string(&sig_bytes),
            public_key: Base64::encode_string(&pk_bytes),
            address: self.address.to_string(),
        })
    }

    fn address(&self) -> &Address {
        &self.address
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_signer() -> SoftwareSigner {
        let mut rng = rand::thread_rng();
        let private_key = Ed25519PrivateKey::generate(&mut rng);
        SoftwareSigner::new(private_key)
    }

    #[test]
    fn sign_verify_round_trip() {
        let signer = test_signer();
        let msg = b"hello world";
        let signed = signer.sign_message(msg).unwrap();

        assert_eq!(signed.message, "hello world");
        assert!(!signed.signature.is_empty());
        assert!(!signed.public_key.is_empty());
        assert!(signed.address.starts_with("0x"));

        let valid = verify_message(msg, &signed.signature, &signed.public_key).unwrap();
        assert!(valid, "round-trip verification should pass");
    }

    #[test]
    fn wrong_message_fails_verification() {
        let signer = test_signer();
        let signed = signer.sign_message(b"hello world").unwrap();

        let valid = verify_message(b"wrong message", &signed.signature, &signed.public_key).unwrap();
        assert!(!valid, "wrong message should fail verification");
    }

    #[test]
    fn wrong_public_key_fails_verification() {
        let signer1 = test_signer();
        let signer2 = test_signer();

        let signed1 = signer1.sign_message(b"hello").unwrap();
        let signed2 = signer2.sign_message(b"hello").unwrap();

        let valid = verify_message(b"hello", &signed1.signature, &signed2.public_key).unwrap();
        assert!(!valid, "wrong public key should fail verification");
    }

    #[test]
    fn invalid_base64_returns_error() {
        assert!(verify_message(b"test", "not-valid-base64!!!", "AAAA").is_err());
    }
}
