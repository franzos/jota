use anyhow::{Context, Result, bail};
use iota_sdk::types::{Digest, ObjectId};

use super::Command;
use crate::display;
use crate::network::TransactionFilter;
use crate::recipient::Recipient;

impl Command {
    /// Parse a command from a raw input string.
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();
        if input.is_empty() {
            bail!("No command entered. Type 'help' for a list of commands.");
        }

        let mut parts = input.splitn(3, char::is_whitespace);
        let cmd = parts.next().unwrap().to_lowercase();
        let arg1 = parts.next().map(|s| s.trim());
        let arg2 = parts.next().map(|s| s.trim());

        match cmd.as_str() {
            "balance" | "bal" => Ok(Command::Balance),

            "address" | "addr" => Ok(Command::Address),

            "transfer" | "send" => {
                let addr_str = arg1.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing recipient. Usage: transfer <address|name.iota> <amount>"
                    )
                })?;
                let amount_str = arg2.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing amount. Usage: transfer <address|name.iota> <amount>"
                    )
                })?;

                let recipient = Recipient::parse(addr_str)?;

                let amount = display::parse_iota_amount(amount_str)
                    .with_context(|| format!("Invalid amount '{amount_str}'"))?;

                if amount == 0 {
                    bail!("Cannot send 0 IOTA.");
                }

                Ok(Command::Transfer { recipient, amount })
            }

            "sweep_all" | "sweep" => {
                let addr_str = arg1.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing recipient. Usage: sweep_all <address|name.iota>"
                    )
                })?;

                let recipient = Recipient::parse(addr_str)?;

                Ok(Command::SweepAll { recipient })
            }

            "show_transfers" | "transfers" | "txs" => {
                let filter = TransactionFilter::from_str_opt(arg1);
                Ok(Command::ShowTransfers { filter })
            }

            "show_transfer" | "tx" => {
                let digest_str = arg1.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing transaction digest. Usage: show_transfer <digest>"
                    )
                })?;

                let digest = Digest::from_base58(digest_str).map_err(|e| {
                    anyhow::anyhow!("Invalid transaction digest '{digest_str}': {e}")
                })?;

                Ok(Command::ShowTransfer { digest })
            }

            "stake" => {
                let addr_str = arg1.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing validator. Usage: stake <validator_address|name.iota> <amount>\n  Find validators at https://explorer.iota.org/validators"
                    )
                })?;
                let amount_str = arg2.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing amount. Usage: stake <validator_address|name.iota> <amount>"
                    )
                })?;

                let validator = Recipient::parse(addr_str)?;

                let amount = display::parse_iota_amount(amount_str)
                    .with_context(|| format!("Invalid amount '{amount_str}'"))?;

                if amount == 0 {
                    bail!("Cannot stake 0 IOTA.");
                }

                Ok(Command::Stake { validator, amount })
            }

            "unstake" => {
                let id_str = arg1.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing staked object ID. Usage: unstake <staked_object_id>"
                    )
                })?;

                let staked_object_id = ObjectId::from_hex(id_str).map_err(|e| {
                    anyhow::anyhow!("Invalid object ID '{id_str}': {e}")
                })?;

                Ok(Command::Unstake { staked_object_id })
            }

            "stakes" => Ok(Command::Stakes),

            "tokens" | "token_balances" => Ok(Command::Tokens),

            "status" => Ok(Command::Status {
                node_url: arg1.map(|s| s.to_string()),
            }),

            "faucet" => Ok(Command::Faucet),

            "seed" => Ok(Command::Seed),

            "account" | "acc" => {
                let index = arg1
                    .map(|s| s.parse::<u64>())
                    .transpose()
                    .map_err(|e| anyhow::anyhow!("Invalid account index: {e}"))?;
                Ok(Command::Account { index })
            }

            "password" | "passwd" => Ok(Command::Password),

            "help" | "?" => Ok(Command::Help {
                command: arg1.map(|s| s.to_string()),
            }),

            "exit" | "quit" | "q" => Ok(Command::Exit),

            other => bail!(
                "Unknown command: '{other}'. Type 'help' for a list of commands."
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_balance() {
        assert_eq!(Command::parse("balance").unwrap(), Command::Balance);
        assert_eq!(Command::parse("bal").unwrap(), Command::Balance);
        assert_eq!(Command::parse("  balance  ").unwrap(), Command::Balance);
    }

    #[test]
    fn parse_address() {
        assert_eq!(Command::parse("address").unwrap(), Command::Address);
        assert_eq!(Command::parse("addr").unwrap(), Command::Address);
    }

    #[test]
    fn parse_transfer() {
        let cmd = Command::parse(
            "transfer 0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900 1.5",
        )
        .unwrap();
        match cmd {
            Command::Transfer { recipient, amount } => {
                assert_eq!(
                    format!("{recipient}"),
                    "0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900"
                );
                assert_eq!(amount, 1_500_000_000);
            }
            other => panic!("expected Transfer, got {other:?}"),
        }
    }

    #[test]
    fn parse_transfer_alias() {
        let cmd = Command::parse(
            "send 0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900 2",
        )
        .unwrap();
        assert!(matches!(cmd, Command::Transfer { .. }));
    }

    #[test]
    fn parse_transfer_missing_amount() {
        let result = Command::parse(
            "transfer 0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900",
        );
        assert!(result.is_err());
    }

    #[test]
    fn parse_transfer_zero_amount() {
        let result = Command::parse(
            "transfer 0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900 0",
        );
        assert!(result.is_err());
    }

    #[test]
    fn parse_show_transfers() {
        assert_eq!(
            Command::parse("show_transfers").unwrap(),
            Command::ShowTransfers {
                filter: TransactionFilter::All
            }
        );
        assert_eq!(
            Command::parse("show_transfers in").unwrap(),
            Command::ShowTransfers {
                filter: TransactionFilter::In
            }
        );
        assert_eq!(
            Command::parse("txs out").unwrap(),
            Command::ShowTransfers {
                filter: TransactionFilter::Out
            }
        );
    }

    #[test]
    fn parse_faucet() {
        assert_eq!(Command::parse("faucet").unwrap(), Command::Faucet);
    }

    #[test]
    fn parse_password() {
        assert_eq!(Command::parse("password").unwrap(), Command::Password);
        assert_eq!(Command::parse("passwd").unwrap(), Command::Password);
    }

    #[test]
    fn parse_seed() {
        assert_eq!(Command::parse("seed").unwrap(), Command::Seed);
    }

    #[test]
    fn parse_help() {
        assert_eq!(
            Command::parse("help").unwrap(),
            Command::Help { command: None }
        );
        assert_eq!(
            Command::parse("help balance").unwrap(),
            Command::Help {
                command: Some("balance".to_string())
            }
        );
    }

    #[test]
    fn parse_exit() {
        assert_eq!(Command::parse("exit").unwrap(), Command::Exit);
        assert_eq!(Command::parse("quit").unwrap(), Command::Exit);
        assert_eq!(Command::parse("q").unwrap(), Command::Exit);
    }

    #[test]
    fn parse_unknown_command() {
        let result = Command::parse("foobar");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("foobar"));
    }

    #[test]
    fn parse_empty_input() {
        assert!(Command::parse("").is_err());
        assert!(Command::parse("   ").is_err());
    }

    #[test]
    fn parse_stake() {
        let cmd = Command::parse(
            "stake 0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900 1.5",
        )
        .unwrap();
        match cmd {
            Command::Stake { validator, amount } => {
                assert_eq!(
                    format!("{validator}"),
                    "0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900"
                );
                assert_eq!(amount, 1_500_000_000);
            }
            other => panic!("expected Stake, got {other:?}"),
        }
    }

    #[test]
    fn parse_stake_missing_amount() {
        let result = Command::parse(
            "stake 0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900",
        );
        assert!(result.is_err());
    }

    #[test]
    fn parse_stake_zero_amount() {
        let result = Command::parse(
            "stake 0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900 0",
        );
        assert!(result.is_err());
    }

    #[test]
    fn parse_unstake() {
        let cmd = Command::parse(
            "unstake 0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900",
        )
        .unwrap();
        assert!(matches!(cmd, Command::Unstake { .. }));
    }

    #[test]
    fn parse_unstake_missing_id() {
        let result = Command::parse("unstake");
        assert!(result.is_err());
    }

    #[test]
    fn parse_stakes() {
        assert_eq!(Command::parse("stakes").unwrap(), Command::Stakes);
    }

    #[test]
    fn parse_sweep_all() {
        let cmd = Command::parse(
            "sweep_all 0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900",
        )
        .unwrap();
        assert!(matches!(cmd, Command::SweepAll { .. }));
    }

    #[test]
    fn parse_sweep_alias() {
        let cmd = Command::parse(
            "sweep 0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900",
        )
        .unwrap();
        assert!(matches!(cmd, Command::SweepAll { .. }));
    }

    #[test]
    fn parse_sweep_all_missing_address() {
        assert!(Command::parse("sweep_all").is_err());
    }

    #[test]
    fn parse_status() {
        assert_eq!(
            Command::parse("status").unwrap(),
            Command::Status { node_url: None }
        );
        assert_eq!(
            Command::parse("status https://example.com/graphql").unwrap(),
            Command::Status { node_url: Some("https://example.com/graphql".to_string()) }
        );
    }

    #[test]
    fn parse_tokens() {
        assert_eq!(Command::parse("tokens").unwrap(), Command::Tokens);
        assert_eq!(Command::parse("token_balances").unwrap(), Command::Tokens);
    }

    #[test]
    fn parse_account() {
        assert_eq!(
            Command::parse("account").unwrap(),
            Command::Account { index: None }
        );
        assert_eq!(
            Command::parse("acc").unwrap(),
            Command::Account { index: None }
        );
        assert_eq!(
            Command::parse("account 3").unwrap(),
            Command::Account { index: Some(3) }
        );
        assert!(Command::parse("account abc").is_err());
    }

    #[test]
    fn parse_case_insensitive() {
        assert_eq!(Command::parse("BALANCE").unwrap(), Command::Balance);
        assert_eq!(Command::parse("Balance").unwrap(), Command::Balance);
        assert_eq!(Command::parse("EXIT").unwrap(), Command::Exit);
    }

    #[test]
    fn parse_transfer_iota_name() {
        let cmd = Command::parse("transfer franz.iota 1.5").unwrap();
        match cmd {
            Command::Transfer { recipient, amount } => {
                assert_eq!(recipient, Recipient::Name("franz.iota".into()));
                assert_eq!(amount, 1_500_000_000);
            }
            other => panic!("expected Transfer, got {other:?}"),
        }
    }

    #[test]
    fn parse_sweep_iota_name() {
        let cmd = Command::parse("sweep_all franz.iota").unwrap();
        match cmd {
            Command::SweepAll { recipient } => {
                assert_eq!(recipient, Recipient::Name("franz.iota".into()));
            }
            other => panic!("expected SweepAll, got {other:?}"),
        }
    }

    #[test]
    fn parse_stake_iota_name() {
        let cmd = Command::parse("stake validator.iota 2").unwrap();
        match cmd {
            Command::Stake { validator, amount } => {
                assert_eq!(validator, Recipient::Name("validator.iota".into()));
                assert_eq!(amount, 2_000_000_000);
            }
            other => panic!("expected Stake, got {other:?}"),
        }
    }
}
