/// Command definitions and parsing for the wallet REPL and one-shot mode.
mod execute;
mod help;
mod parse;

pub use help::help_text;

use iota_sdk::types::{Digest, ObjectId};

use crate::display;
use crate::network::TransactionFilter;
use crate::recipient::{Recipient, ResolvedRecipient};

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Show wallet balance
    Balance,
    /// Show wallet address
    Address,
    /// Transfer IOTA to a recipient: transfer <address|name.iota> <amount>
    Transfer { recipient: Recipient, amount: u64 },
    /// Sweep entire balance minus gas: sweep_all <address|name.iota>
    SweepAll { recipient: Recipient },
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
    /// Show network status: status [node_url]
    Status { node_url: Option<String> },
    /// Show seed phrase (mnemonic)
    Seed,
    /// Show or switch account index: account [index]
    Account { index: Option<u64> },
    /// Change wallet password
    Password,
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
            Command::SweepAll { recipient } => Some(recipient),
            Command::Stake { validator, .. } => Some(validator),
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
            Command::Transfer { recipient, amount } => Some(format!(
                "Send {} to {}?",
                display::format_balance(*amount),
                display_recipient(recipient),
            )),
            Command::SweepAll { recipient } => Some(format!(
                "Sweep entire balance to {}?",
                display_recipient(recipient),
            )),
            Command::Stake { validator, amount } => Some(format!(
                "Stake {} to validator {}?",
                display::format_balance(*amount),
                display_recipient(validator),
            )),
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
        };
        let prompt = cmd.confirmation_prompt(None).unwrap();
        assert!(prompt.contains("1.000000000 IOTA"));
    }

    #[test]
    fn sweep_all_requires_confirmation() {
        let cmd = Command::SweepAll {
            recipient: Recipient::Address(Address::ZERO),
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
