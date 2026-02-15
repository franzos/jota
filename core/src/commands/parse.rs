use anyhow::{bail, Context, Result};
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
                        "Missing recipient. Usage: transfer <address|name.iota> <amount> [token]"
                    )
                })?;
                let rest = arg2.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing amount. Usage: transfer <address|name.iota> <amount> [token]"
                    )
                })?;

                let recipient = Recipient::parse(addr_str)?;

                // rest may be "50 usdt" or just "1.5"
                let (amount_str, token) = match rest.split_once(char::is_whitespace) {
                    Some((amt, tok)) => (amt.trim(), Some(tok.trim().to_string())),
                    None => (rest, None),
                };

                let raw_amount = amount_str.to_string();

                if token.is_some() {
                    // Defer full parsing to execute time (need token's decimals).
                    // Basic validation: must look like a number (digits and at most one dot).
                    let valid = !amount_str.is_empty()
                        && amount_str.chars().all(|c| c.is_ascii_digit() || c == '.')
                        && amount_str.matches('.').count() <= 1
                        && amount_str.chars().any(|c| c.is_ascii_digit());
                    if !valid {
                        bail!("Invalid amount '{amount_str}'");
                    }
                    Ok(Command::Transfer {
                        recipient,
                        amount: 0,
                        token,
                        raw_amount,
                    })
                } else {
                    let amount = display::parse_iota_amount(amount_str)
                        .with_context(|| format!("Invalid amount '{amount_str}'"))?;
                    if amount == 0 {
                        bail!("Cannot send 0 IOTA.");
                    }
                    Ok(Command::Transfer {
                        recipient,
                        amount,
                        token,
                        raw_amount,
                    })
                }
            }

            "sweep_all" | "sweep" => {
                let addr_str = arg1.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing recipient. Usage: sweep_all <address|name.iota> [token]"
                    )
                })?;

                let recipient = Recipient::parse(addr_str)?;
                let token = arg2.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

                Ok(Command::SweepAll { recipient, token })
            }

            "show_transfers" | "transfers" | "txs" => {
                let filter = TransactionFilter::from_str_opt(arg1);
                Ok(Command::ShowTransfers { filter })
            }

            "show_transfer" | "tx" => {
                let digest_str = arg1.ok_or_else(|| {
                    anyhow::anyhow!("Missing transaction digest. Usage: show_transfer <digest>")
                })?;

                let digest = Digest::from_base58(digest_str)
                    .with_context(|| format!("Invalid transaction digest '{digest_str}'"))?;

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
                    anyhow::anyhow!("Missing staked object ID. Usage: unstake <staked_object_id>")
                })?;

                let staked_object_id = ObjectId::from_hex(id_str)
                    .with_context(|| format!("Invalid object ID '{id_str}'"))?;

                Ok(Command::Unstake { staked_object_id })
            }

            "stakes" => Ok(Command::Stakes),

            "tokens" | "token_balances" => Ok(Command::Tokens),

            "nfts" => Ok(Command::Nfts),

            "send_nft" => {
                let id_str = arg1.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing object ID. Usage: send_nft <object_id> <address|name.iota>"
                    )
                })?;
                let addr_str = arg2.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing recipient. Usage: send_nft <object_id> <address|name.iota>"
                    )
                })?;

                let object_id = ObjectId::from_hex(id_str)
                    .with_context(|| format!("Invalid object ID '{id_str}'"))?;
                let recipient =
                    Recipient::parse(addr_str.split_whitespace().next().unwrap_or(addr_str))?;

                Ok(Command::SendNft {
                    object_id,
                    recipient,
                })
            }

            "status" => Ok(Command::Status {
                node_url: arg1.map(|s| s.to_string()),
            }),

            "faucet" => Ok(Command::Faucet),

            "seed" => Ok(Command::Seed),

            "account" | "acc" => {
                let index = arg1
                    .map(|s| s.parse::<u64>())
                    .transpose()
                    .context("Invalid account index")?;
                Ok(Command::Account { index })
            }

            "sign_message" | "sign" => {
                // Rejoin arg1 + arg2 to capture the full message text
                let message = match (arg1, arg2) {
                    (Some(a), Some(b)) => format!("{a} {b}"),
                    (Some(a), None) => a.to_string(),
                    _ => bail!("Missing message. Usage: sign_message <message>"),
                };
                Ok(Command::SignMessage { message })
            }

            "notarize" => {
                let message = match (arg1, arg2) {
                    (Some(a), Some(b)) => format!("{a} {b}"),
                    (Some(a), None) => a.to_string(),
                    _ => bail!("Missing message. Usage: notarize <message>"),
                };
                Ok(Command::Notarize { message })
            }

            "verify_message" | "verify" => {
                // Re-split with 4 parts: cmd, message, signature, public_key
                let mut parts = input.splitn(4, char::is_whitespace);
                let _cmd = parts.next(); // skip command
                let message = parts
                    .next()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Missing arguments. Usage: verify <message> <signature_b64> <public_key_b64>"
                        )
                    })?
                    .to_string();
                let signature = parts
                    .next()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Missing signature. Usage: verify <message> <signature_b64> <public_key_b64>"
                        )
                    })?
                    .to_string();
                let public_key = parts
                    .next()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Missing public key. Usage: verify <message> <signature_b64> <public_key_b64>"
                        )
                    })?
                    .to_string();
                Ok(Command::VerifyMessage {
                    message,
                    signature,
                    public_key,
                })
            }

            "password" | "passwd" => Ok(Command::Password),

            "reconnect" => Ok(Command::Reconnect),

            "help" | "?" => Ok(Command::Help {
                command: arg1.map(|s| s.to_string()),
            }),

            "exit" | "quit" | "q" => Ok(Command::Exit),

            other => bail!("Unknown command: '{other}'. Type 'help' for a list of commands."),
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
            Command::Transfer {
                recipient,
                amount,
                token,
                ..
            } => {
                assert_eq!(
                    format!("{recipient}"),
                    "0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900"
                );
                assert_eq!(amount, 1_500_000_000);
                assert_eq!(token, None);
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
            Command::Status {
                node_url: Some("https://example.com/graphql".to_string())
            }
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
            Command::Transfer {
                recipient,
                amount,
                token,
                ..
            } => {
                assert_eq!(recipient, Recipient::Name("franz.iota".into()));
                assert_eq!(amount, 1_500_000_000);
                assert_eq!(token, None);
            }
            other => panic!("expected Transfer, got {other:?}"),
        }
    }

    #[test]
    fn parse_transfer_with_token() {
        let cmd = Command::parse("transfer franz.iota 50 usdt").unwrap();
        match cmd {
            Command::Transfer {
                recipient,
                amount,
                token,
                raw_amount,
            } => {
                assert_eq!(recipient, Recipient::Name("franz.iota".into()));
                // amount is 0 — deferred to execute time with correct decimals
                assert_eq!(amount, 0);
                assert_eq!(raw_amount, "50");
                assert_eq!(token, Some("usdt".to_string()));
            }
            other => panic!("expected Transfer, got {other:?}"),
        }
    }

    #[test]
    fn parse_sweep_all_with_token() {
        let cmd = Command::parse("sweep_all franz.iota usdt").unwrap();
        match cmd {
            Command::SweepAll { recipient, token } => {
                assert_eq!(recipient, Recipient::Name("franz.iota".into()));
                assert_eq!(token, Some("usdt".to_string()));
            }
            other => panic!("expected SweepAll, got {other:?}"),
        }
    }

    #[test]
    fn parse_sweep_iota_name() {
        let cmd = Command::parse("sweep_all franz.iota").unwrap();
        match cmd {
            Command::SweepAll { recipient, .. } => {
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

    // -- Token amount validation edge cases --

    const TEST_ADDR: &str = "0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900";

    #[test]
    fn transfer_token_bare_dot_rejected() {
        let input = format!("transfer {TEST_ADDR} . usdt");
        assert!(
            Command::parse(&input).is_err(),
            "bare dot should be rejected"
        );
    }

    #[test]
    fn transfer_token_nan_rejected() {
        let input = format!("transfer {TEST_ADDR} NaN usdt");
        assert!(Command::parse(&input).is_err(), "NaN should be rejected");
    }

    #[test]
    fn transfer_token_inf_rejected() {
        let input = format!("transfer {TEST_ADDR} inf usdt");
        assert!(Command::parse(&input).is_err(), "inf should be rejected");
    }

    #[test]
    fn transfer_token_negative_rejected() {
        let input = format!("transfer {TEST_ADDR} -5 usdt");
        assert!(
            Command::parse(&input).is_err(),
            "negative amount should be rejected"
        );
    }

    #[test]
    fn transfer_token_scientific_notation_rejected() {
        let input = format!("transfer {TEST_ADDR} 1e10 usdt");
        assert!(
            Command::parse(&input).is_err(),
            "scientific notation should be rejected"
        );
    }

    #[test]
    fn parse_sign_message() {
        let cmd = Command::parse("sign_message hello world").unwrap();
        match cmd {
            Command::SignMessage { message } => assert_eq!(message, "hello world"),
            other => panic!("expected SignMessage, got {other:?}"),
        }
    }

    #[test]
    fn parse_sign_alias() {
        let cmd = Command::parse("sign test").unwrap();
        assert!(matches!(cmd, Command::SignMessage { .. }));
    }

    #[test]
    fn parse_sign_message_missing() {
        assert!(Command::parse("sign_message").is_err());
    }

    #[test]
    fn parse_verify_message() {
        let cmd = Command::parse("verify hello sig123 pk456").unwrap();
        match cmd {
            Command::VerifyMessage {
                message,
                signature,
                public_key,
            } => {
                assert_eq!(message, "hello");
                assert_eq!(signature, "sig123");
                assert_eq!(public_key, "pk456");
            }
            other => panic!("expected VerifyMessage, got {other:?}"),
        }
    }

    #[test]
    fn parse_verify_missing_args() {
        assert!(Command::parse("verify").is_err());
        assert!(Command::parse("verify hello").is_err());
        assert!(Command::parse("verify hello sig").is_err());
    }

    #[test]
    fn parse_notarize() {
        let cmd = Command::parse("notarize hello world").unwrap();
        match cmd {
            Command::Notarize { message } => assert_eq!(message, "hello world"),
            other => panic!("expected Notarize, got {other:?}"),
        }
    }

    #[test]
    fn parse_notarize_single_word() {
        let cmd = Command::parse("notarize test").unwrap();
        match cmd {
            Command::Notarize { message } => assert_eq!(message, "test"),
            other => panic!("expected Notarize, got {other:?}"),
        }
    }

    #[test]
    fn parse_notarize_missing_message() {
        assert!(Command::parse("notarize").is_err());
    }

    #[test]
    fn transfer_token_leading_dot_accepted() {
        let input = format!("transfer {TEST_ADDR} .5 usdt");
        let result = Command::parse(&input);
        assert!(result.is_ok(), ".5 should pass character-class validation");
    }
}
