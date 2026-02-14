use anyhow::{Result, bail};

use super::Command;
use super::help::help_text;
use crate::cache::TransactionCache;
use crate::display;
use crate::network::NetworkClient;
use crate::service::WalletService;
use crate::recipient::ResolvedRecipient;
use crate::wallet::Wallet;

impl Command {
    /// Execute a command and return the output string.
    /// When `resolved` is provided (pre-resolved name), uses that address directly.
    /// Otherwise resolves inline if needed.
    pub async fn execute(
        &self,
        wallet: &Wallet,
        service: &WalletService,
        json_output: bool,
        allow_insecure: bool,
        resolved: Option<&ResolvedRecipient>,
    ) -> Result<String> {
        match self {
            Command::Balance => {
                let nanos = service.balance().await?;
                if json_output {
                    Ok(display::format_balance_json(nanos))
                } else {
                    Ok(display::format_balance(nanos))
                }
            }

            Command::Address => {
                let addr = service.address().to_string();
                if json_output {
                    Ok(display::format_address_json(&addr))
                } else {
                    Ok(addr)
                }
            }

            Command::Transfer { recipient, amount, token, raw_amount } => {
                let res = match resolved {
                    Some(r) => r.clone(),
                    None => service.resolve_recipient(recipient).await?,
                };

                if let Some(token_alias) = token {
                    // Token transfer — parse raw_amount with the token's actual decimals
                    let meta = service.resolve_coin_type(token_alias).await?;
                    let parsed = display::parse_token_amount(raw_amount, meta.decimals)?;
                    let token_amount = u64::try_from(parsed)
                        .map_err(|_| anyhow::anyhow!("Amount too large for transfer"))?;
                    if token_amount == 0 {
                        bail!("Cannot send 0 {}.", meta.symbol);
                    }
                    let result = service.send_token(res.address, &meta.coin_type, token_amount).await?;
                    let display_amount = display::format_balance_with_symbol(
                        token_amount as u128, meta.decimals, &meta.symbol,
                    );

                    if json_output {
                        Ok(serde_json::json!({
                            "digest": result.digest,
                            "status": result.status,
                            "amount": token_amount,
                            "amount_display": display_amount,
                            "coin_type": meta.coin_type,
                            "symbol": meta.symbol,
                            "recipient": res.address.to_string(),
                            "name": res.name,
                        })
                        .to_string())
                    } else {
                        Ok(format!(
                            "Transaction sent!\n  Digest: {}\n  Status: {}\n  Amount: {} -> {}",
                            result.digest,
                            result.status,
                            display_amount,
                            res,
                        ))
                    }
                } else {
                    // IOTA transfer (unchanged)
                    let result = service.send(res.address, *amount).await?;

                    if json_output {
                        Ok(serde_json::json!({
                            "digest": result.digest,
                            "status": result.status,
                            "amount_nanos": amount,
                            "amount_iota": display::nanos_to_iota(*amount),
                            "recipient": res.address.to_string(),
                            "name": res.name,
                        })
                        .to_string())
                    } else {
                        Ok(format!(
                            "Transaction sent!\n  Digest: {}\n  Status: {}\n  Amount: {} -> {}",
                            result.digest,
                            result.status,
                            display::format_balance(*amount),
                            res,
                        ))
                    }
                }
            }

            Command::SweepAll { recipient, token } => {
                let res = match resolved {
                    Some(r) => r.clone(),
                    None => service.resolve_recipient(recipient).await?,
                };

                if let Some(token_alias) = token {
                    let meta = service.resolve_coin_type(token_alias).await?;
                    let (result, total) = service.sweep_all_token(res.address, &meta.coin_type).await?;
                    let display_amount = display::format_balance_with_symbol(
                        total, meta.decimals, &meta.symbol,
                    );

                    if json_output {
                        Ok(serde_json::json!({
                            "digest": result.digest,
                            "status": result.status,
                            "amount": total.to_string(),
                            "amount_display": display_amount,
                            "coin_type": meta.coin_type,
                            "symbol": meta.symbol,
                            "recipient": res.address.to_string(),
                            "name": res.name,
                        })
                        .to_string())
                    } else {
                        Ok(format!(
                            "Sweep sent!\n  Digest: {}\n  Status: {}\n  Amount: {} -> {}",
                            result.digest,
                            result.status,
                            display_amount,
                            res,
                        ))
                    }
                } else {
                    let (result, amount) = service.sweep_all(res.address).await?;

                    if json_output {
                        Ok(serde_json::json!({
                            "digest": result.digest,
                            "status": result.status,
                            "amount_nanos": amount,
                            "amount_iota": display::nanos_to_iota(amount),
                            "recipient": res.address.to_string(),
                            "name": res.name,
                        })
                        .to_string())
                    } else {
                        Ok(format!(
                            "Sweep sent!\n  Digest: {}\n  Status: {}\n  Amount: {} -> {}",
                            result.digest,
                            result.status,
                            display::format_balance(amount),
                            res,
                        ))
                    }
                }
            }

            Command::ShowTransfers { filter } => {
                service.sync_transactions().await?;
                let txs = {
                    let cache = TransactionCache::open()?;
                    let network_str = service.network_name();
                    let address_str = service.address().to_string();
                    cache.query(network_str, &address_str, filter, 25, 0)?.transactions
                };
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
                let details = service.transaction_details(digest).await?;
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

            Command::Stake { validator, amount } => {
                let res = match resolved {
                    Some(r) => r.clone(),
                    None => service.resolve_recipient(validator).await?,
                };
                let result = service.stake(res.address, *amount).await?;

                if json_output {
                    Ok(serde_json::json!({
                        "digest": result.digest,
                        "status": result.status,
                        "amount_nanos": amount,
                        "amount_iota": display::nanos_to_iota(*amount),
                        "validator": res.address.to_string(),
                        "name": res.name,
                    })
                    .to_string())
                } else {
                    Ok(format!(
                        "Stake sent!\n  Digest: {}\n  Status: {}\n  Amount: {} -> {}",
                        result.digest,
                        result.status,
                        display::format_balance(*amount),
                        res,
                    ))
                }
            }

            Command::Unstake { staked_object_id } => {
                let result = service.unstake(*staked_object_id).await?;

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
                let stakes = service.get_stakes().await?;
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
                                "estimated_reward_nanos": s.estimated_reward,
                                "estimated_reward_iota": s.estimated_reward.map(display::nanos_to_iota),
                                "status": s.status.to_string(),
                            })
                        })
                        .collect();
                    Ok(serde_json::to_string_pretty(&json_stakes)?)
                } else {
                    Ok(display::format_stakes(&stakes))
                }
            }

            Command::Tokens => {
                let balances = service.get_token_balances().await?;

                // Fetch metadata for non-IOTA tokens
                let futs: Vec<_> = balances
                    .iter()
                    .filter(|b| b.coin_type != "0x2::iota::IOTA")
                    .map(|b| service.resolve_coin_type(&b.coin_type))
                    .collect();
                let results = futures::future::join_all(futs).await;
                let meta: Vec<_> = results.into_iter().filter_map(|r| r.ok()).collect();

                if json_output {
                    let json_balances: Vec<serde_json::Value> = balances
                        .iter()
                        .map(|b| {
                            let coin_meta = meta.iter().find(|m| m.coin_type == b.coin_type);
                            let mut obj = serde_json::json!({
                                "coin_type": b.coin_type,
                                "coin_object_count": b.coin_object_count,
                                "total_balance": b.total_balance.to_string(),
                            });
                            if let Some(m) = coin_meta {
                                obj["symbol"] = serde_json::json!(m.symbol);
                                obj["decimals"] = serde_json::json!(m.decimals);
                                obj["name"] = serde_json::json!(m.name);
                            }
                            obj
                        })
                        .collect();
                    Ok(serde_json::to_string_pretty(&json_balances)?)
                } else {
                    Ok(display::format_token_balances_with_meta(&balances, &meta))
                }
            }

            Command::Status { node_url } => {
                let status = match node_url {
                    Some(url) => NetworkClient::new_custom(url, allow_insecure)?.status().await?,
                    None => service.status().await?,
                };
                if json_output {
                    Ok(serde_json::json!({
                        "epoch": status.epoch,
                        "reference_gas_price": status.reference_gas_price,
                        "network": status.network.to_string(),
                        "node_url": status.node_url,
                    })
                    .to_string())
                } else {
                    Ok(display::format_status(&status))
                }
            }

            Command::Faucet => {
                service.faucet().await?;
                let addr = service.address().to_string();
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
                if wallet.is_ledger() {
                    bail!("Seed phrase is not available for Ledger wallets.");
                }
                let mnemonic = wallet.mnemonic()
                    .ok_or_else(|| anyhow::anyhow!("No mnemonic available."))?;
                if json_output {
                    Ok(serde_json::json!({
                        "mnemonic": mnemonic,
                    })
                    .to_string())
                } else {
                    Ok(format!(
                        "Seed phrase (keep this secret!):\n  {}",
                        mnemonic
                    ))
                }
            }

            Command::Account { index } => {
                match index {
                    None => {
                        let idx = wallet.account_index();
                        let addr = wallet.address().to_string();
                        let is_ledger = wallet.is_ledger();
                        if json_output {
                            let known: Vec<serde_json::Value> = wallet
                                .known_accounts()
                                .iter()
                                .map(|a| {
                                    let a_addr = if is_ledger {
                                        if a.index == idx { addr.clone() } else { "?".to_string() }
                                    } else {
                                        wallet
                                            .derive_address_for(a.index)
                                            .map(|a| a.to_string())
                                            .unwrap_or_default()
                                    };
                                    serde_json::json!({
                                        "index": a.index,
                                        "address": a_addr,
                                        "active": a.index == idx,
                                    })
                                })
                                .collect();
                            Ok(serde_json::json!({
                                "account_index": idx,
                                "address": addr,
                                "known_accounts": known,
                            })
                            .to_string())
                        } else {
                            let type_label = if is_ledger { " (Ledger)" } else { "" };
                            let mut out = format!("Account #{idx}{type_label}\n  {addr}\n");
                            let known = wallet.known_accounts();
                            if !known.is_empty() {
                                out.push_str("\nKnown accounts:\n");
                                for a in known {
                                    let a_addr = if is_ledger {
                                        if a.index == idx { addr.clone() } else { "?".to_string() }
                                    } else {
                                        wallet
                                            .derive_address_for(a.index)
                                            .map(|a| a.to_string())
                                            .unwrap_or_default()
                                    };
                                    let short = if a_addr.len() > 20 {
                                        format!("{}...{}", &a_addr[..10], &a_addr[a_addr.len()-8..])
                                    } else {
                                        a_addr
                                    };
                                    let active = if a.index == idx { "  (active)" } else { "" };
                                    out.push_str(&format!("  #{:<4} {}{}\n", a.index, short, active));
                                }
                            }
                            out.push_str("\nSwitch: account <index>");
                            Ok(out)
                        }
                    }
                    Some(_) => {
                        bail!("Account switching requires interactive mode. Use the REPL instead of --cmd.")
                    }
                }
            }

            Command::Password => {
                bail!("The password command requires interactive mode. Use the REPL instead of --cmd.")
            }

            Command::SignMessage { message } => {
                let signed = service.sign_message(message.as_bytes())?;
                if json_output {
                    Ok(serde_json::to_string_pretty(&signed)?)
                } else {
                    Ok(format!(
                        "Message signed!\n  Message:    {}\n  Signature:  {}\n  Public key: {}\n  Address:    {}",
                        signed.message, signed.signature, signed.public_key, signed.address,
                    ))
                }
            }

            Command::Notarize { message } => {
                let result = service.notarize(message, None).await?;
                let network_query = match service.network_name() {
                    "mainnet" => "",
                    "testnet" => "?network=testnet",
                    "devnet" => "?network=devnet",
                    _ => "?network=testnet",
                };
                let explorer_url = format!(
                    "https://explorer.iota.org/txblock/{}{}",
                    result.digest, network_query,
                );
                if json_output {
                    Ok(serde_json::json!({
                        "digest": result.digest,
                        "status": result.status,
                        "message": message,
                        "explorer_url": explorer_url,
                    })
                    .to_string())
                } else {
                    Ok(format!(
                        "Notarized!\n  Digest:    {}\n  Status:    {}\n  Explorer:  {}",
                        result.digest, result.status, explorer_url,
                    ))
                }
            }

            Command::VerifyMessage { message, signature, public_key } => {
                let valid = crate::signer::verify_message(
                    message.as_bytes(),
                    signature,
                    public_key,
                )?;
                if json_output {
                    Ok(serde_json::json!({
                        "valid": valid,
                        "message": message,
                    })
                    .to_string())
                } else if valid {
                    Ok("Signature is VALID".to_string())
                } else {
                    Ok("Signature is INVALID".to_string())
                }
            }

            Command::Nfts => {
                let nfts = service.get_nfts().await?;
                if json_output {
                    let json_nfts: Vec<serde_json::Value> = nfts
                        .iter()
                        .map(|n| {
                            serde_json::json!({
                                "object_id": n.object_id.to_string(),
                                "object_type": n.object_type,
                                "name": n.name,
                                "description": n.description,
                                "image_url": n.image_url,
                            })
                        })
                        .collect();
                    Ok(serde_json::to_string_pretty(&json_nfts)?)
                } else {
                    Ok(display::format_nfts(&nfts))
                }
            }

            Command::SendNft { object_id, recipient } => {
                let res = match resolved {
                    Some(r) => r.clone(),
                    None => service.resolve_recipient(recipient).await?,
                };
                let result = service.send_nft(*object_id, res.address).await?;

                if json_output {
                    Ok(serde_json::json!({
                        "digest": result.digest,
                        "status": result.status,
                        "object_id": object_id.to_string(),
                        "recipient": res.address.to_string(),
                        "name": res.name,
                    })
                    .to_string())
                } else {
                    Ok(format!(
                        "NFT sent!\n  Digest: {}\n  Status: {}\n  Object: {} -> {}",
                        result.digest,
                        result.status,
                        object_id,
                        res,
                    ))
                }
            }

            Command::Help { command } => Ok(help_text(command.as_deref())),

            // Handled directly in the REPL loop (needs mutable service access)
            Command::Reconnect => Ok(String::new()),

            Command::Exit => Ok(String::new()),
        }
    }
}
