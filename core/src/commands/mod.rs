/// Command definitions and parsing for the wallet REPL and one-shot mode.
mod execute;
mod help;
mod parse;

pub use help::help_text;

use iota_sdk::types::{Digest, ObjectId};

use crate::display;

/// UTF-8 safe string truncation for confirmation prompts.
fn truncate_preview(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}
use crate::network::TransactionFilter;
use crate::recipient::{Recipient, ResolvedRecipient};

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Show wallet balance
    Balance,
    /// Show wallet address
    Address,
    /// Transfer IOTA or tokens: transfer <address|name.iota> <amount> [token]
    /// When `token` is Some, `amount` is 0 and `raw_amount` holds the user input
    /// (parsed with the token's actual decimals at execute time).
    Transfer {
        recipient: Recipient,
        amount: u64,
        token: Option<String>,
        raw_amount: String,
    },
    /// Sweep entire balance: sweep_all <address|name.iota> [token]
    SweepAll { recipient: Recipient, token: Option<String> },
    /// Show transaction history: show_transfers [in|out|all]
    ShowTransfers { filter: TransactionFilter },
    /// Look up a transaction by digest: show_transfer <digest>
    ShowTransfer { digest: Digest },
    /// Request faucet tokens (testnet/devnet only)
    Faucet,
    /// Stake IOTA to a validator: stake <validator_address|name.iota> <amount>
    Stake { validator: Recipient, amount: u64 },
    /// Unstake a staked IOTA object: unstake <staked_object_id>
    Unstake { staked_object_id: ObjectId },
    /// Show all active stakes
    Stakes,
    /// Show non-native token balances
    Tokens,
    /// List owned NFTs
    Nfts,
    /// Transfer an NFT: send_nft <object_id> <address|name.iota>
    SendNft { object_id: ObjectId, recipient: Recipient },
    /// Show network status: status [node_url]
    Status { node_url: Option<String> },
    /// Show seed phrase (mnemonic)
    Seed,
    /// Show or switch account index: account [index]
    Account { index: Option<u64> },
    /// Sign an arbitrary message with the wallet's key
    SignMessage { message: String },
    /// Verify a signed message: verify <message> <signature_b64> <public_key_b64>
    VerifyMessage { message: String, signature: String, public_key: String },
    /// Notarize a message on-chain: notarize <message>
    Notarize { message: String },
    /// Change wallet password
    Password,
    /// Reconnect the Ledger device
    Reconnect,
    /// Print help
    Help { command: Option<String> },
    /// Exit the wallet
    Exit,
}

impl Command {
    /// Return a reference to the recipient/validator if this command has one.
    pub fn recipient(&self) -> Option<&Recipient> {
        match self {
            Command::Transfer { recipient, .. } => Some(recipient),
            Command::SweepAll { recipient, .. } => Some(recipient),
            Command::Stake { validator, .. } => Some(validator),
            Command::SendNft { recipient, .. } => Some(recipient),
            _ => None,
        }
    }

    /// Returns a confirmation prompt if this command should ask before executing.
    /// When a `ResolvedRecipient` is provided, shows the resolved name + address.
    pub fn confirmation_prompt(
        &self,
        resolved: Option<&ResolvedRecipient>,
    ) -> Option<String> {
        let display_recipient = |r: &Recipient| -> String {
            match resolved {
                Some(res) => res.to_string(),
                None => r.to_string(),
            }
        };

        match self {
            Command::Transfer { recipient, amount, token, raw_amount } => {
                let amount_str = match token {
                    Some(t) => format!("{raw_amount} {t}"),
                    None => display::format_balance(*amount),
                };
                Some(format!("Send {} to {}?", amount_str, display_recipient(recipient)))
            }
            Command::SweepAll { recipient, token } => {
                let what = match token {
                    Some(t) => format!("all {t}"),
                    None => "entire balance".to_string(),
                };
                Some(format!("Sweep {what} to {}?", display_recipient(recipient)))
            }
            Command::Stake { validator, amount } => Some(format!(
                "Stake {} to validator {}?",
                display::format_balance(*amount),
                display_recipient(validator),
            )),
            Command::SendNft { object_id, recipient } => {
                Some(format!("Send NFT {} to {}?", object_id, display_recipient(recipient)))
            }
            Command::SignMessage { message } => {
                let preview = truncate_preview(message, 40);
                Some(format!("Sign message \"{preview}\" with your private key?"))
            }
            Command::Notarize { message } => {
                let preview = truncate_preview(message, 40);
                Some(format!(
                    "Notarize \"{preview}\" on-chain? This is permanent, publicly visible, and costs gas."
                ))
            }
            Command::Seed => Some("This will display sensitive data. Continue?".to_string()),
            Command::Password => Some("Change wallet password?".to_string()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iota_sdk::types::Address;

    #[test]
    fn stake_requires_confirmation() {
        let cmd = Command::Stake {
            validator: Recipient::Address(Address::ZERO),
            amount: 1_000_000_000,
        };
        assert!(cmd.confirmation_prompt(None).is_some());
    }

    #[test]
    fn transfer_requires_confirmation() {
        let cmd = Command::Transfer {
            recipient: Recipient::Address(Address::ZERO),
            amount: 1_000_000_000,
            token: None,
            raw_amount: "1".into(),
        };
        let prompt = cmd.confirmation_prompt(None).unwrap();
        assert!(prompt.contains("1.000000000 IOTA"));
    }

    #[test]
    fn sweep_all_requires_confirmation() {
        let cmd = Command::SweepAll {
            recipient: Recipient::Address(Address::ZERO),
            token: None,
        };
        assert!(cmd.confirmation_prompt(None).is_some());
    }

    #[test]
    fn seed_requires_confirmation() {
        assert!(Command::Seed.confirmation_prompt(None).is_some());
        assert!(Command::Balance.confirmation_prompt(None).is_none());
        assert!(Command::Address.confirmation_prompt(None).is_none());
    }

    #[test]
    fn confirmation_prompt_with_resolved() {
        let cmd = Command::Transfer {
            recipient: Recipient::Name("franz.iota".into()),
            amount: 1_000_000_000,
            token: None,
            raw_amount: "1".into(),
        };
        let resolved = ResolvedRecipient {
            address: Address::ZERO,
            name: Some("franz.iota".into()),
        };
        let prompt = cmd.confirmation_prompt(Some(&resolved)).unwrap();
        assert!(prompt.contains("franz.iota"));
        assert!(prompt.contains("0x"));
    }
}
