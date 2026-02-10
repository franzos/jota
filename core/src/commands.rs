/// Command definitions and parsing for the wallet REPL and one-shot mode.
use anyhow::{Result, bail};
use iota_sdk::types::{Address, Digest, ObjectId};

use crate::display;
use crate::network::{NetworkClient, TransactionFilter};
use crate::wallet::Wallet;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Show wallet balance
    Balance,
    /// Show wallet address
    Address,
    /// Transfer IOTA to another address: transfer <address> <amount>
    Transfer { recipient: Address, amount: u64 },
    /// Sweep entire balance minus gas to an address: sweep_all <address>
    SweepAll { recipient: Address },
    /// Show transaction history: show_transfers [in|out|all]
    ShowTransfers { filter: TransactionFilter },
    /// Look up a transaction by digest: show_transfer <digest>
    ShowTransfer { digest: Digest },
    /// Show current reference gas price
    Fee,
    /// Request faucet tokens (testnet/devnet only)
    Faucet,
    /// Stake IOTA to a validator: stake <validator_address> <amount>
    Stake { validator: Address, amount: u64 },
    /// Unstake a staked IOTA object: unstake <staked_object_id>
    Unstake { staked_object_id: ObjectId },
    /// Show all active stakes
    Stakes,
    /// Show seed phrase (mnemonic)
    Seed,
    /// Print help
    Help { command: Option<String> },
    /// Exit the wallet
    Exit,
}

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
                        "Missing recipient address. Usage: transfer <address> <amount>"
                    )
                })?;
                let amount_str = arg2.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing amount. Usage: transfer <address> <amount>"
                    )
                })?;

                let recipient = Address::from_hex(addr_str).map_err(|e| {
                    anyhow::anyhow!("Invalid recipient address '{addr_str}': {e}")
                })?;

                let amount = display::parse_iota_amount(amount_str).map_err(|e| {
                    anyhow::anyhow!("Invalid amount '{amount_str}': {e}")
                })?;

                if amount == 0 {
                    bail!("Cannot send 0 IOTA.");
                }

                Ok(Command::Transfer { recipient, amount })
            }

            "sweep_all" | "sweep" => {
                let addr_str = arg1.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing recipient address. Usage: sweep_all <address>"
                    )
                })?;

                let recipient = Address::from_hex(addr_str).map_err(|e| {
                    anyhow::anyhow!("Invalid recipient address '{addr_str}': {e}")
                })?;

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

            "fee" | "gas" => Ok(Command::Fee),

            "stake" => {
                let addr_str = arg1.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing validator address. Usage: stake <validator_address> <amount>\n  Find validators at https://explorer.iota.org/validators"
                    )
                })?;
                let amount_str = arg2.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing amount. Usage: stake <validator_address> <amount>"
                    )
                })?;

                let validator = Address::from_hex(addr_str).map_err(|e| {
                    anyhow::anyhow!("Invalid validator address '{addr_str}': {e}")
                })?;

                let amount = display::parse_iota_amount(amount_str).map_err(|e| {
                    anyhow::anyhow!("Invalid amount '{amount_str}': {e}")
                })?;

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

            "faucet" => Ok(Command::Faucet),

            "seed" => Ok(Command::Seed),

            "help" | "?" => Ok(Command::Help {
                command: arg1.map(|s| s.to_string()),
            }),

            "exit" | "quit" | "q" => Ok(Command::Exit),

            other => bail!(
                "Unknown command: '{other}'. Type 'help' for a list of commands."
            ),
        }
    }

    /// Returns a confirmation prompt if this command should ask before executing.
    pub fn confirmation_prompt(&self) -> Option<String> {
        match self {
            Command::Transfer { recipient, amount } => Some(format!(
                "Send {} to {}?",
                display::format_balance(*amount),
                recipient,
            )),
            Command::SweepAll { recipient } => Some(format!(
                "Sweep entire balance to {}?",
                recipient,
            )),
            Command::Stake { validator, amount } => Some(format!(
                "Stake {} to validator {}?",
                display::format_balance(*amount),
                validator,
            )),
            Command::Seed => Some("This will display sensitive data. Continue?".to_string()),
            _ => None,
        }
    }

    /// Execute a command and return the output string.
    pub async fn execute(
        &self,
        wallet: &Wallet,
        network: &NetworkClient,
        json_output: bool,
    ) -> Result<String> {
        match self {
            Command::Balance => {
                let nanos = network.balance(wallet.address()).await?;
                if json_output {
                    Ok(display::format_balance_json(nanos))
                } else {
                    Ok(display::format_balance(nanos))
                }
            }

            Command::Address => {
                let addr = wallet.address().to_string();
                if json_output {
                    Ok(display::format_address_json(&addr))
                } else {
                    Ok(addr)
                }
            }

            Command::Transfer { recipient, amount } => {
                let result = network
                    .send_iota(
                        wallet.private_key(),
                        wallet.address(),
                        *recipient,
                        *amount,
                    )
                    .await?;

                if json_output {
                    Ok(serde_json::json!({
                        "digest": result.digest,
                        "status": result.status,
                        "amount_nanos": amount,
                        "amount_iota": display::nanos_to_iota(*amount),
                        "recipient": recipient.to_string(),
                    })
                    .to_string())
                } else {
                    Ok(format!(
                        "Transaction sent!\n  Digest: {}\n  Status: {}\n  Amount: {} -> {}",
                        result.digest,
                        result.status,
                        display::format_balance(*amount),
                        recipient,
                    ))
                }
            }

            Command::SweepAll { recipient } => {
                let (result, amount) = network
                    .sweep_all(
                        wallet.private_key(),
                        wallet.address(),
                        *recipient,
                    )
                    .await?;

                if json_output {
                    Ok(serde_json::json!({
                        "digest": result.digest,
                        "status": result.status,
                        "amount_nanos": amount,
                        "amount_iota": display::nanos_to_iota(amount),
                        "recipient": recipient.to_string(),
                    })
                    .to_string())
                } else {
                    Ok(format!(
                        "Sweep sent!\n  Digest: {}\n  Status: {}\n  Amount: {} -> {}",
                        result.digest,
                        result.status,
                        display::format_balance(amount),
                        recipient,
                    ))
                }
            }

            Command::ShowTransfers { filter } => {
                let txs = network.transactions(wallet.address(), filter.clone()).await?;
                if json_output {
                    let json_txs: Vec<serde_json::Value> = txs
                        .iter()
                        .map(|tx| {
                            serde_json::json!({
                                "digest": tx.digest,
                                "direction": tx.direction.map(|d| d.to_string()),
                                "timestamp": tx.timestamp,
                                "sender": tx.sender,
                                "amount": tx.amount,
                                "fee": tx.fee,
                            })
                        })
                        .collect();
                    Ok(serde_json::to_string_pretty(&json_txs)?)
                } else {
                    Ok(display::format_transactions(&txs))
                }
            }

            Command::ShowTransfer { digest } => {
                let details = network.transaction_details(digest).await?;
                if json_output {
                    Ok(serde_json::json!({
                        "digest": details.digest,
                        "status": details.status,
                        "sender": details.sender,
                        "recipient": details.recipient,
                        "amount": details.amount,
                        "fee": details.fee,
                    })
                    .to_string())
                } else {
                    Ok(display::format_transaction_details(&details))
                }
            }

            Command::Fee => {
                let gas_price = network.reference_gas_price().await?;
                if json_output {
                    Ok(serde_json::json!({
                        "reference_gas_price": gas_price,
                    })
                    .to_string())
                } else {
                    Ok(display::format_gas_price(gas_price))
                }
            }

            Command::Stake { validator, amount } => {
                let result = network
                    .stake_iota(
                        wallet.private_key(),
                        wallet.address(),
                        *validator,
                        *amount,
                    )
                    .await?;

                if json_output {
                    Ok(serde_json::json!({
                        "digest": result.digest,
                        "status": result.status,
                        "amount_nanos": amount,
                        "amount_iota": display::nanos_to_iota(*amount),
                        "validator": validator.to_string(),
                    })
                    .to_string())
                } else {
                    Ok(format!(
                        "Stake sent!\n  Digest: {}\n  Status: {}\n  Amount: {} -> {}",
                        result.digest,
                        result.status,
                        display::format_balance(*amount),
                        validator,
                    ))
                }
            }

            Command::Unstake { staked_object_id } => {
                let result = network
                    .unstake_iota(
                        wallet.private_key(),
                        wallet.address(),
                        *staked_object_id,
                    )
                    .await?;

                if json_output {
                    Ok(serde_json::json!({
                        "digest": result.digest,
                        "status": result.status,
                        "staked_object_id": staked_object_id.to_string(),
                    })
                    .to_string())
                } else {
                    Ok(format!(
                        "Unstake sent!\n  Digest: {}\n  Status: {}",
                        result.digest,
                        result.status,
                    ))
                }
            }

            Command::Stakes => {
                let stakes = network.get_stakes(wallet.address()).await?;
                if json_output {
                    let json_stakes: Vec<serde_json::Value> = stakes
                        .iter()
                        .map(|s| {
                            serde_json::json!({
                                "object_id": s.object_id.to_string(),
                                "pool_id": s.pool_id.to_string(),
                                "principal_nanos": s.principal,
                                "principal_iota": display::nanos_to_iota(s.principal),
                                "stake_activation_epoch": s.stake_activation_epoch,
                            })
                        })
                        .collect();
                    Ok(serde_json::to_string_pretty(&json_stakes)?)
                } else {
                    Ok(display::format_stakes(&stakes))
                }
            }

            Command::Faucet => {
                if wallet.is_mainnet() {
                    bail!("Faucet is not available on mainnet.");
                }
                network.faucet(wallet.address()).await?;
                let addr = wallet.address().to_string();
                if json_output {
                    Ok(serde_json::json!({
                        "status": "ok",
                        "address": addr,
                    })
                    .to_string())
                } else {
                    Ok(format!("Faucet tokens requested for {addr}. It may take a moment to arrive."))
                }
            }

            Command::Seed => {
                if json_output {
                    Ok(serde_json::json!({
                        "mnemonic": wallet.mnemonic(),
                    })
                    .to_string())
                } else {
                    Ok(format!(
                        "Seed phrase (keep this secret!):\n  {}",
                        wallet.mnemonic()
                    ))
                }
            }

            Command::Help { command } => Ok(help_text(command.as_deref())),

            Command::Exit => Ok(String::new()),
        }
    }
}

#[must_use]
pub fn help_text(command: Option<&str>) -> String {
    match command {
        Some("balance") | Some("bal") => {
            "balance\n  Show the IOTA balance for this wallet.\n  Alias: bal".to_string()
        }
        Some("address") | Some("addr") => {
            "address\n  Show the wallet's primary address.\n  Alias: addr".to_string()
        }
        Some("transfer") | Some("send") => {
            "transfer <address> <amount>\n  Send IOTA to another address.\n  Amount is in IOTA (e.g. '1.5' for 1.5 IOTA).\n  Alias: send".to_string()
        }
        Some("sweep_all") | Some("sweep") => {
            "sweep_all <address>\n  Send entire balance minus gas to an address.\n  Alias: sweep".to_string()
        }
        Some("show_transfers") | Some("transfers") | Some("txs") => {
            "show_transfers [in|out|all]\n  Show transaction history.\n  Filter: 'in' (received), 'out' (sent), 'all' (default).\n  Aliases: transfers, txs".to_string()
        }
        Some("show_transfer") | Some("tx") => {
            "show_transfer <digest>\n  Look up a specific transaction by its digest.\n  Alias: tx".to_string()
        }
        Some("fee") | Some("gas") => {
            "fee\n  Show the current reference gas price.\n  Alias: gas".to_string()
        }
        Some("stake") => {
            "stake <validator_address> <amount>\n  Stake IOTA to a validator.\n  Amount is in IOTA (e.g. '1.5' for 1.5 IOTA).\n  Find validators at https://explorer.iota.org/validators".to_string()
        }
        Some("unstake") => {
            "unstake <staked_object_id>\n  Unstake a previously staked IOTA object.\n  Use 'stakes' to find object IDs.".to_string()
        }
        Some("stakes") => {
            "stakes\n  Show all active stakes for this wallet.".to_string()
        }
        Some("faucet") => {
            "faucet\n  Request test tokens from the faucet.\n  Only available on testnet and devnet.".to_string()
        }
        Some("seed") => {
            "seed\n  Display the wallet's seed phrase (mnemonic).\n  Keep this secret!".to_string()
        }
        Some("exit") | Some("quit") | Some("q") => {
            "exit\n  Exit the wallet.\n  Aliases: quit, q".to_string()
        }
        Some(other) => format!("Unknown command: '{other}'. Type 'help' for a list."),
        None => {
            "Available commands:\n\
             \n\
             \x20 balance          Show wallet balance\n\
             \x20 address          Show wallet address\n\
             \x20 transfer         Send IOTA to an address\n\
             \x20 sweep_all        Sweep entire balance to an address\n\
             \x20 show_transfers   Show transaction history\n\
             \x20 show_transfer    Look up a transaction by digest\n\
             \x20 fee              Show current reference gas price\n\
             \x20 stake            Stake IOTA to a validator\n\
             \x20 unstake          Unstake a staked IOTA object\n\
             \x20 stakes           Show active stakes\n\
             \x20 faucet           Request testnet/devnet tokens\n\
             \x20 seed             Show seed phrase\n\
             \x20 help [cmd]       Show help for a command\n\
             \x20 exit             Exit the wallet\n\
             \n\
             Type 'help <command>' for detailed help on a specific command."
                .to_string()
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
    fn help_text_general() {
        let text = help_text(None);
        assert!(text.contains("balance"));
        assert!(text.contains("transfer"));
        assert!(text.contains("faucet"));
    }

    #[test]
    fn help_text_specific() {
        let text = help_text(Some("transfer"));
        assert!(text.contains("<address>"));
        assert!(text.contains("<amount>"));
    }

    #[test]
    fn help_text_unknown() {
        let text = help_text(Some("nonexistent"));
        assert!(text.contains("Unknown command"));
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
    fn stake_requires_confirmation() {
        let cmd = Command::Stake {
            validator: Address::ZERO,
            amount: 1_000_000_000,
        };
        assert!(cmd.confirmation_prompt().is_some());
    }

    #[test]
    fn transfer_requires_confirmation() {
        let cmd = Command::Transfer {
            recipient: Address::ZERO,
            amount: 1_000_000_000,
        };
        let prompt = cmd.confirmation_prompt().unwrap();
        assert!(prompt.contains("1.000000000 IOTA"));
    }

    #[test]
    fn sweep_all_requires_confirmation() {
        let cmd = Command::SweepAll {
            recipient: Address::ZERO,
        };
        assert!(cmd.confirmation_prompt().is_some());
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
    fn parse_fee() {
        assert_eq!(Command::parse("fee").unwrap(), Command::Fee);
        assert_eq!(Command::parse("gas").unwrap(), Command::Fee);
    }

    #[test]
    fn parse_case_insensitive() {
        assert_eq!(Command::parse("BALANCE").unwrap(), Command::Balance);
        assert_eq!(Command::parse("Balance").unwrap(), Command::Balance);
        assert_eq!(Command::parse("EXIT").unwrap(), Command::Exit);
    }

    #[test]
    fn seed_requires_confirmation() {
        assert!(Command::Seed.confirmation_prompt().is_some());
        assert!(Command::Balance.confirmation_prompt().is_none());
        assert!(Command::Address.confirmation_prompt().is_none());
    }
}
