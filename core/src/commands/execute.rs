use anyhow::{bail, Context, Result};
use base64ct::{Base64, Encoding};
use iota_sdk::types::MultisigMemberPublicKey;

use super::help::help_text;
use super::{Command, ContactSubcommand, MultisigSubcommand};
use crate::cache::TransactionCache;
use crate::contacts::ContactStore;
use crate::display;
use crate::multisig::formats::{
    MemberEntry, MultisigFile, ProposalFile, SignatureEntry, SignatureFile,
};
use crate::multisig::store::MultisigStore;
use crate::multisig::{CollectedSignature, ProposalStatus, TransactionProposal};
use crate::network::NetworkClient;
use crate::recipient::ResolvedRecipient;
use crate::service::WalletService;
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

            Command::Transfer {
                recipient,
                amount,
                token,
                raw_amount,
            } => {
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
                    let result = service
                        .send_token(res.address, &meta.coin_type, token_amount)
                        .await?;
                    let display_amount = display::format_balance_with_symbol(
                        token_amount as u128,
                        meta.decimals,
                        &meta.symbol,
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
                            result.digest, result.status, display_amount, res,
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
                    let (result, total) = service
                        .sweep_all_token(res.address, &meta.coin_type)
                        .await?;
                    let display_amount =
                        display::format_balance_with_symbol(total, meta.decimals, &meta.symbol);

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
                            result.digest, result.status, display_amount, res,
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
                service.sync_transactions(7).await?;
                let txs = {
                    let cache = TransactionCache::open()?;
                    let network_str = service.network_name();
                    let address_str = service.address().to_string();
                    cache
                        .query(network_str, &address_str, filter, 25, 0)?
                        .transactions
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
                        result.digest, result.status,
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
                    Some(url) => {
                        NetworkClient::new_custom(url, allow_insecure)?
                            .status()
                            .await?
                    }
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
                    Ok(format!(
                        "Faucet tokens requested for {addr}. It may take a moment to arrive."
                    ))
                }
            }

            Command::Seed => {
                if wallet.is_hardware() {
                    bail!("Seed phrase is not available for hardware wallets.");
                }
                let mnemonic = wallet
                    .mnemonic()
                    .ok_or_else(|| anyhow::anyhow!("No mnemonic available."))?;
                if json_output {
                    Ok(serde_json::json!({
                        "mnemonic": mnemonic,
                    })
                    .to_string())
                } else {
                    Ok(format!("Seed phrase (keep this secret!):\n  {}", mnemonic))
                }
            }

            Command::Account { index } => match index {
                None => {
                    let idx = wallet.account_index();
                    let addr = wallet.address().to_string();
                    let is_hardware = wallet.is_hardware();
                    let resolve_addr = |account_index: u64| -> String {
                        if is_hardware {
                            if account_index == idx {
                                addr.clone()
                            } else {
                                "?".to_string()
                            }
                        } else {
                            wallet
                                .derive_address_for(account_index)
                                .map(|a| a.to_string())
                                .unwrap_or_default()
                        }
                    };
                    if json_output {
                        let known: Vec<serde_json::Value> = wallet
                            .known_accounts()
                            .iter()
                            .map(|a| {
                                let a_addr = resolve_addr(a.index);
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
                        let type_label = match wallet.hardware_kind() {
                            Some(kind) => format!(" ({kind})"),
                            None => String::new(),
                        };
                        let mut out = format!("Account #{idx}{type_label}\n  {addr}\n");
                        let known = wallet.known_accounts();
                        if !known.is_empty() {
                            out.push_str("\nKnown accounts:\n");
                            for a in known {
                                let a_addr = resolve_addr(a.index);
                                let short = if a_addr.len() > 20 {
                                    format!("{}...{}", &a_addr[..10], &a_addr[a_addr.len() - 8..])
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
            },

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

            Command::VerifyMessage {
                message,
                signature,
                public_key,
            } => {
                let valid =
                    crate::signer::verify_message(message.as_bytes(), signature, public_key)?;
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

            Command::SendNft {
                object_id,
                recipient,
            } => {
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
                        result.digest, result.status, object_id, res,
                    ))
                }
            }

            Command::Contact { subcommand } => {
                let mut store = ContactStore::open()?;
                match subcommand {
                    ContactSubcommand::List => {
                        let contacts = store.list();
                        if contacts.is_empty() {
                            if json_output {
                                Ok("[]".to_string())
                            } else {
                                Ok(
                                    "No contacts yet. Add one with: contacts add <name> <address>"
                                        .to_string(),
                                )
                            }
                        } else if json_output {
                            Ok(store.export()?)
                        } else {
                            let mut out = String::new();
                            for c in contacts {
                                let name_part = if let Some(ref iota_name) = c.iota_name {
                                    format!("{} ({})", c.name, iota_name)
                                } else {
                                    c.name.clone()
                                };
                                out.push_str(&format!("  {:<20} {}\n", name_part, c.address));
                            }
                            Ok(out.trim_end().to_string())
                        }
                    }
                    ContactSubcommand::Add { name, address } => {
                        // If address looks like an .iota name, store it as iota_name
                        // and resolve the actual address later (for now store as-is).
                        let (addr, iota_name) = if address.ends_with(".iota") {
                            // For CLI, we need a resolved address. Try to resolve.
                            let r = crate::recipient::Recipient::parse(address)?;
                            match resolved {
                                Some(res) => (res.address.to_string(), Some(address.clone())),
                                None => {
                                    // Try resolving via the service
                                    let res = service.resolve_recipient(&r).await?;
                                    (res.address.to_string(), Some(address.clone()))
                                }
                            }
                        } else {
                            (address.clone(), None)
                        };
                        store.add(name, &addr, iota_name.as_deref())?;
                        if json_output {
                            Ok(serde_json::json!({"status": "ok", "name": name}).to_string())
                        } else {
                            Ok(format!("Contact '{name}' added."))
                        }
                    }
                    ContactSubcommand::Remove { name } => {
                        store.remove(name)?;
                        if json_output {
                            Ok(serde_json::json!({"status": "ok", "name": name}).to_string())
                        } else {
                            Ok(format!("Contact '{name}' removed."))
                        }
                    }
                    ContactSubcommand::Export => Ok(store.export()?),
                    ContactSubcommand::Import { file } => {
                        let json =
                            std::fs::read_to_string(file).context("Failed to read import file")?;
                        let added = store.import(&json)?;
                        if json_output {
                            Ok(serde_json::json!({"status": "ok", "added": added}).to_string())
                        } else {
                            Ok(format!("{added} contact(s) imported."))
                        }
                    }
                }
            }

            Command::Multisig { subcommand } => {
                let store = MultisigStore::open()?;
                let network_name = service.network_name();

                match subcommand {
                    MultisigSubcommand::Create { .. } => {
                        bail!("The multisig create wizard requires interactive mode. Use the REPL instead of --cmd.")
                    }

                    MultisigSubcommand::List => {
                        let configs = store.list_configs()?;
                        if configs.is_empty() {
                            Ok("No multisig addresses configured. Import one with: multisig import <file>".to_string())
                        } else {
                            let mut out = String::new();
                            for c in &configs {
                                let addr = c.address();
                                let n_members = c.committee.members().len();
                                let threshold = c.committee.threshold();
                                let total_weight: u16 = c
                                    .committee
                                    .members()
                                    .iter()
                                    .map(|m| m.weight() as u16)
                                    .sum();
                                out.push_str(&format!(
                                    "  {:<20} {} ({}/{} weight, {} members)\n",
                                    c.name, addr, threshold, total_weight, n_members,
                                ));
                            }
                            Ok(out.trim_end().to_string())
                        }
                    }

                    MultisigSubcommand::Show { name } => {
                        let config = store.load_config(name)?;
                        let addr = config.address();
                        let balance = service.network().balance(&addr).await.unwrap_or(0);
                        let balance_str = display::format_balance(balance);

                        let total_weight: u16 = config
                            .committee
                            .members()
                            .iter()
                            .map(|m| m.weight() as u16)
                            .sum();
                        let mut out = format!(
                            "Multisig: {}\nAddress:  {}\nNetwork:  {:?}\nBalance:  {}\nThreshold: {} of {} total weight\n\nParticipants:\n",
                            config.name, addr, config.network, balance_str,
                            config.committee.threshold(), total_weight,
                        );

                        for (i, member) in config.committee.members().iter().enumerate() {
                            let label = config.labels.get(i).map(|s| s.as_str()).unwrap_or("?");
                            let is_me = config.my_key.as_ref() == Some(member.public_key());
                            let me_marker = if is_me { " (you)" } else { "" };
                            out.push_str(&format!(
                                "  {}. {:<15} weight {} {}{}\n",
                                i + 1,
                                label,
                                member.weight(),
                                format_scheme(member.public_key()),
                                me_marker,
                            ));
                        }
                        Ok(out.trim_end().to_string())
                    }

                    MultisigSubcommand::Import { file } => {
                        let json = std::fs::read_to_string(file).context("Failed to read file")?;

                        let ms_file: MultisigFile =
                            serde_json::from_str(&json).context("Failed to parse multisig file")?;
                        ms_file.validate(network_name)?;

                        let committee = ms_file.to_committee()?;
                        let addr = committee.derive_address();
                        let labels: Vec<String> =
                            ms_file.members.iter().map(|m| m.label.clone()).collect();

                        // Derive name from filename
                        let name = file
                            .trim_end_matches(".jota-multisig")
                            .rsplit('/')
                            .next()
                            .unwrap_or("imported")
                            .to_string();

                        let mut config = crate::multisig::MultisigConfig {
                            name: name.clone(),
                            committee,
                            labels,
                            network: wallet.network_config().network,
                            my_key: None,
                        };

                        // Auto-detect which participant we are
                        if let Ok(pk_bytes) = service.signer().public_key_bytes() {
                            config.detect_my_key(&pk_bytes);
                        }

                        store.save_config(&config)?;

                        let my_label = config.my_key.as_ref().and_then(|mk| {
                            config
                                .committee
                                .members()
                                .iter()
                                .enumerate()
                                .find(|(_, m)| m.public_key() == mk)
                                .and_then(|(i, _)| config.labels.get(i))
                                .map(|l| l.as_str())
                        });

                        let mut out = format!("Imported: {} ({})\n", name, addr);
                        if let Some(label) = my_label {
                            out.push_str(&format!("  You are: {}\n", label));
                        }
                        out.push_str(&format!(
                            "  Threshold: {} of {}",
                            config.committee.threshold(),
                            config.committee.members().len(),
                        ));
                        Ok(out)
                    }

                    MultisigSubcommand::Export { name } => {
                        let config = store.load_config(name)?;
                        let ms_file = MultisigFile {
                            version: 1,
                            file_type: "multisig-address".to_string(),
                            network: network_name.to_string(),
                            members: config
                                .committee
                                .members()
                                .iter()
                                .enumerate()
                                .map(|(i, m)| MemberEntry {
                                    public_key: m.public_key().clone(),
                                    weight: m.weight(),
                                    label: config.labels.get(i).cloned().unwrap_or_default(),
                                })
                                .collect(),
                            threshold: config.committee.threshold(),
                        };
                        let json = serde_json::to_string_pretty(&ms_file)?;
                        let filename = format!("{name}.jota-multisig");
                        std::fs::write(&filename, &json)
                            .with_context(|| format!("Failed to write {filename}"))?;
                        Ok(format!("Exported: {filename}"))
                    }

                    MultisigSubcommand::Remove { name } => {
                        store.remove_config(name)?;
                        Ok(format!("Removed multisig config '{name}'."))
                    }

                    MultisigSubcommand::Send {
                        name,
                        recipient,
                        amount,
                    } => {
                        let mut config = store.load_config(name)?;

                        // Auto-detect key if needed
                        if config.my_key.is_none() {
                            if let Ok(pk_bytes) = service.signer().public_key_bytes() {
                                config.detect_my_key(&pk_bytes);
                            }
                        }

                        let multisig_addr = config.address();

                        // Parse recipient
                        let resolved_recipient = service
                            .resolve_recipient(&crate::recipient::Recipient::parse(recipient)?)
                            .await?;

                        // Parse amount
                        let amount_nanos = display::parse_iota_amount(amount)
                            .with_context(|| format!("Invalid amount '{amount}'"))?;
                        if amount_nanos == 0 {
                            bail!("Cannot send 0 IOTA.");
                        }

                        // Check balance
                        let balance = service.network().balance(&multisig_addr).await?;
                        if balance < amount_nanos {
                            bail!(
                                "Insufficient multisig balance: {} available, {} needed.",
                                display::format_balance(balance),
                                display::format_balance(amount_nanos),
                            );
                        }

                        // Build unsigned transaction
                        let tx = crate::multisig::build_transfer(
                            service.network(),
                            &multisig_addr,
                            resolved_recipient.address,
                            amount_nanos,
                        )
                        .await?;
                        let tx_bytes =
                            bcs::to_bytes(&tx).context("Failed to serialize transaction")?;
                        let tx_digest = crate::multisig::compute_tx_digest(&tx_bytes)?;
                        let short_id = &tx_digest[..8.min(tx_digest.len())];

                        let now = timestamp_now();

                        // Determine proposer label
                        let proposer = config
                            .my_key
                            .as_ref()
                            .and_then(|mk| {
                                config
                                    .committee
                                    .members()
                                    .iter()
                                    .enumerate()
                                    .find(|(_, m)| m.public_key() == mk)
                                    .and_then(|(i, _)| config.labels.get(i))
                                    .cloned()
                            })
                            .unwrap_or_else(|| "unknown".to_string());

                        let mut proposal = TransactionProposal {
                            tx_digest: tx_digest.clone(),
                            multisig_address: multisig_addr.to_string(),
                            tx_bytes: tx_bytes.clone(),
                            proposer: proposer.clone(),
                            created_at: now,
                            expires_at: Some(now + 7 * 24 * 3600),
                            signatures: Vec::new(),
                            status: ProposalStatus::Pending,
                        };

                        // Sign with local key if we're a participant
                        if config.my_key.is_some() {
                            let (member_key, member_sig) = crate::multisig::sign_proposal(
                                service.network(),
                                service.signer().as_ref(),
                                &tx_bytes,
                            )
                            .await?;
                            proposal.signatures.push(CollectedSignature {
                                member_key,
                                signature: member_sig,
                                signed_at: now,
                            });
                            crate::multisig::update_proposal_status(
                                &config.committee,
                                &mut proposal,
                            );
                        }

                        store.save_proposal(&proposal)?;

                        // Export proposal file
                        let proposal_file = ProposalFile {
                            version: 1,
                            file_type: "proposal".to_string(),
                            network: network_name.to_string(),
                            multisig: MultisigFile {
                                version: 1,
                                file_type: "multisig-address".to_string(),
                                network: network_name.to_string(),
                                members: config
                                    .committee
                                    .members()
                                    .iter()
                                    .enumerate()
                                    .map(|(i, m)| MemberEntry {
                                        public_key: m.public_key().clone(),
                                        weight: m.weight(),
                                        label: config.labels.get(i).cloned().unwrap_or_default(),
                                    })
                                    .collect(),
                                threshold: config.committee.threshold(),
                            },
                            tx_bytes: Base64::encode_string(&tx_bytes),
                            proposer: proposer.clone(),
                            created_at: format_timestamp(now),
                            signatures: proposal
                                .signatures
                                .iter()
                                .map(|s| SignatureEntry {
                                    public_key: s.member_key.clone(),
                                    signature: s.signature.clone(),
                                    signed_at: format_timestamp(s.signed_at),
                                })
                                .collect(),
                        };
                        let proposal_json = serde_json::to_string_pretty(&proposal_file)?;
                        let export_filename = format!("{short_id}.jota-proposal");
                        std::fs::write(&export_filename, &proposal_json)
                            .with_context(|| format!("Failed to write {export_filename}"))?;

                        let collected_weight: u16 = proposal
                            .signatures
                            .iter()
                            .filter_map(|s| {
                                config
                                    .committee
                                    .members()
                                    .iter()
                                    .find(|m| m.public_key() == &s.member_key)
                                    .map(|m| m.weight() as u16)
                            })
                            .sum();
                        let threshold = config.committee.threshold();

                        let mut out = format!(
                            "Proposing from: {} ({})\nTransfer {} -> {}\n\nProposal saved: {}\nExported: {}\n\nSignatures: {}/{} weight",
                            name, multisig_addr,
                            display::format_balance(amount_nanos), resolved_recipient,
                            short_id, export_filename,
                            collected_weight, threshold,
                        );

                        if proposal.status == ProposalStatus::Ready {
                            out.push_str("\nThreshold met! Submit with: multisig submit ");
                            out.push_str(short_id);
                        } else {
                            let needed = threshold.saturating_sub(collected_weight);
                            out.push_str(&format!(
                                "\nNeed {} more weight to meet threshold.",
                                needed
                            ));
                        }

                        Ok(out)
                    }

                    MultisigSubcommand::Proposals { name } => {
                        let proposals = match name {
                            Some(n) => {
                                let config = store.load_config(n)?;
                                store.list_proposals_for(&config.address().to_string())?
                            }
                            None => store.list_proposals()?,
                        };

                        if proposals.is_empty() {
                            Ok("No pending proposals.".to_string())
                        } else {
                            let mut out = format!(
                                "{:<10} {:<20} {:<30} {:<10}\n",
                                "ID", "Multisig", "Action", "Status"
                            );
                            let configs = store.list_configs()?;
                            for p in &proposals {
                                let short_id = &p.tx_digest[..8.min(p.tx_digest.len())];
                                let desc = crate::multisig::describe_transaction(&p.tx_bytes).ok();
                                let action = match &desc {
                                    Some(d) => {
                                        let amt = d
                                            .amount
                                            .map(display::format_balance)
                                            .unwrap_or_default();
                                        let rcpt = d
                                            .recipient
                                            .as_deref()
                                            .map(|r| {
                                                if r.len() > 10 {
                                                    format!("{}...", &r[..10])
                                                } else {
                                                    r.to_string()
                                                }
                                            })
                                            .unwrap_or_default();
                                        if !amt.is_empty() && !rcpt.is_empty() {
                                            format!("{} -> {}", amt, rcpt)
                                        } else {
                                            "Transaction".to_string()
                                        }
                                    }
                                    None => "Transaction".to_string(),
                                };
                                let status = match &p.status {
                                    ProposalStatus::Pending => "pending",
                                    ProposalStatus::Ready => "ready",
                                    ProposalStatus::Submitted { .. } => "submitted",
                                    ProposalStatus::Failed { .. } => "failed",
                                    ProposalStatus::Stale { .. } => "stale",
                                    ProposalStatus::Cancelled => "cancelled",
                                };
                                let ms_name = configs
                                    .iter()
                                    .find(|c| {
                                        c.address().to_string().to_lowercase()
                                            == p.multisig_address.to_lowercase()
                                    })
                                    .map(|c| c.name.clone())
                                    .unwrap_or_else(|| {
                                        let addr = &p.multisig_address;
                                        if addr.len() > 10 {
                                            format!("{}...", &addr[..10])
                                        } else {
                                            addr.to_string()
                                        }
                                    });
                                out.push_str(&format!(
                                    "{:<10} {:<20} {:<30} {:<10}\n",
                                    short_id, ms_name, action, status,
                                ));
                            }
                            Ok(out.trim_end().to_string())
                        }
                    }

                    MultisigSubcommand::Proposal { id } => {
                        let proposal = store.find_proposal_by_prefix(id)?;
                        let short_id = &proposal.tx_digest[..8.min(proposal.tx_digest.len())];
                        let desc = crate::multisig::describe_transaction(&proposal.tx_bytes).ok();

                        // Find config for this address
                        let config = store.list_configs()?.into_iter().find(|c| {
                            c.address().to_string().to_lowercase()
                                == proposal.multisig_address.to_lowercase()
                        });

                        let mut out = format!(
                            "Proposal: {}\nDigest:   {}\nFrom:     {}\n",
                            short_id, proposal.tx_digest, proposal.multisig_address,
                        );

                        if let Some(d) = &desc {
                            if let Some(amt) = d.amount {
                                out.push_str(&format!(
                                    "Action:   Transfer {} ",
                                    display::format_balance(amt)
                                ));
                                if let Some(ref r) = d.recipient {
                                    out.push_str(&format!("-> {}", r));
                                }
                                out.push('\n');
                            }
                            out.push_str(&format!("Gas:      {} nanos\n", d.gas_budget));
                        }

                        let status = match &proposal.status {
                            ProposalStatus::Pending => "pending",
                            ProposalStatus::Ready => "ready",
                            ProposalStatus::Submitted { digest } => {
                                out.push_str(&format!("Tx:       {}\n", digest));
                                "submitted"
                            }
                            ProposalStatus::Failed { reason } => {
                                out.push_str(&format!("Error:    {}\n", reason));
                                "failed"
                            }
                            ProposalStatus::Stale { reason } => {
                                out.push_str(&format!("Stale:    {}\n", reason));
                                "stale"
                            }
                            ProposalStatus::Cancelled => "cancelled",
                        };
                        out.push_str(&format!(
                            "Status:   {}\nProposer: {}\n",
                            status, proposal.proposer,
                        ));

                        if let Some(config) = &config {
                            let collected_weight: u16 = proposal
                                .signatures
                                .iter()
                                .filter_map(|s| {
                                    config
                                        .committee
                                        .members()
                                        .iter()
                                        .find(|m| m.public_key() == &s.member_key)
                                        .map(|m| m.weight() as u16)
                                })
                                .sum();
                            out.push_str(&format!(
                                "\nSignatures ({}/{} threshold):\n",
                                collected_weight,
                                config.committee.threshold(),
                            ));

                            for (i, member) in config.committee.members().iter().enumerate() {
                                let label = config.labels.get(i).map(|s| s.as_str()).unwrap_or("?");
                                let signed = proposal
                                    .signatures
                                    .iter()
                                    .any(|s| &s.member_key == member.public_key());
                                let marker = if signed { "[x]" } else { "[ ]" };
                                let is_me = config.my_key.as_ref() == Some(member.public_key());
                                let me_str = if is_me { " <- you" } else { "" };
                                out.push_str(&format!(
                                    "  {} {:<15} (weight {}){}\n",
                                    marker,
                                    label,
                                    member.weight(),
                                    me_str,
                                ));
                            }
                        }

                        Ok(out.trim_end().to_string())
                    }

                    MultisigSubcommand::Cancel { id } => {
                        let mut proposal = store.find_proposal_by_prefix(id)?;
                        proposal.status = ProposalStatus::Cancelled;
                        store.save_proposal(&proposal)?;
                        let short_id = &proposal.tx_digest[..8.min(proposal.tx_digest.len())];
                        Ok(format!("Proposal {} cancelled locally.", short_id))
                    }

                    MultisigSubcommand::AddSig { id, file } => {
                        let mut proposal = store.find_proposal_by_prefix(id)?;

                        // Find the config for this proposal's multisig address
                        let config = store
                            .list_configs()?
                            .into_iter()
                            .find(|c| {
                                c.address().to_string().to_lowercase()
                                    == proposal.multisig_address.to_lowercase()
                            })
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "No multisig config found for address {}",
                                    proposal.multisig_address
                                )
                            })?;

                        let json = std::fs::read_to_string(file)
                            .context("Failed to read signature file")?;

                        if file.ends_with(".jota-sig") {
                            // Single signature file
                            let sig_file: SignatureFile = serde_json::from_str(&json)
                                .context("Failed to parse .jota-sig file")?;
                            sig_file.validate()?;

                            // Verify digest matches
                            if sig_file.tx_digest != proposal.tx_digest {
                                bail!(
                                    "Signature is for a different transaction (digest mismatch)."
                                );
                            }

                            // Validate and add
                            crate::multisig::validate_signature(
                                &config.committee,
                                &proposal.tx_bytes,
                                &sig_file.public_key,
                                &sig_file.signature,
                                &proposal.signatures,
                            )?;

                            let now = timestamp_now();

                            proposal.signatures.push(CollectedSignature {
                                member_key: sig_file.public_key.clone(),
                                signature: sig_file.signature,
                                signed_at: now,
                            });

                            crate::multisig::update_proposal_status(
                                &config.committee,
                                &mut proposal,
                            );
                            store.save_proposal(&proposal)?;

                            // Find label
                            let label = config
                                .committee
                                .members()
                                .iter()
                                .enumerate()
                                .find(|(_, m)| m.public_key() == &sig_file.public_key)
                                .and_then(|(i, _)| config.labels.get(i))
                                .map(|l| l.as_str())
                                .unwrap_or("unknown");

                            let mut out = format!("Signature verified ({}).\n", label);

                            if proposal.status == ProposalStatus::Ready {
                                let short_id =
                                    &proposal.tx_digest[..8.min(proposal.tx_digest.len())];
                                out.push_str("Threshold met! Submit with: multisig submit ");
                                out.push_str(short_id);
                            }

                            Ok(out)
                        } else if file.ends_with(".jota-proposal") {
                            // Updated proposal file with more signatures
                            let prop_file: ProposalFile = serde_json::from_str(&json)
                                .context("Failed to parse .jota-proposal file")?;
                            prop_file.validate(network_name)?;

                            let added = crate::multisig::merge_signatures(
                                &config.committee,
                                &mut proposal,
                                &prop_file.signatures,
                            )?;

                            store.save_proposal(&proposal)?;

                            let mut out = format!("{} new signature(s) merged.\n", added);
                            if proposal.status == ProposalStatus::Ready {
                                let short_id =
                                    &proposal.tx_digest[..8.min(proposal.tx_digest.len())];
                                out.push_str("Threshold met! Submit with: multisig submit ");
                                out.push_str(short_id);
                            }
                            Ok(out)
                        } else {
                            bail!("Unrecognized file format. Expected .jota-sig or .jota-proposal file.")
                        }
                    }

                    MultisigSubcommand::Submit { id } => {
                        let mut proposal = store.find_proposal_by_prefix(id)?;

                        let config = store
                            .list_configs()?
                            .into_iter()
                            .find(|c| {
                                c.address().to_string().to_lowercase()
                                    == proposal.multisig_address.to_lowercase()
                            })
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "No multisig config found for address {}",
                                    proposal.multisig_address
                                )
                            })?;

                        // Check status
                        match &proposal.status {
                            ProposalStatus::Submitted { digest } => {
                                bail!("Already submitted (tx: {digest}).")
                            }
                            ProposalStatus::Cancelled => bail!("Proposal is cancelled."),
                            ProposalStatus::Stale { reason } => {
                                bail!("Proposal is stale: {reason}")
                            }
                            _ => {}
                        }

                        // Check expiration
                        if let Some(expires_at) = proposal.expires_at {
                            let now = timestamp_now();
                            if now > expires_at {
                                bail!("Proposal has expired.");
                            }
                        }

                        let result = crate::multisig::aggregate_and_submit(
                            service.network(),
                            &config.committee,
                            &mut proposal,
                        )
                        .await?;

                        store.save_proposal(&proposal)?;

                        Ok(format!(
                            "Transaction submitted!\n  Digest: {}\n  Status: {}",
                            result.digest, result.status,
                        ))
                    }

                    MultisigSubcommand::Sign { file } => {
                        let json = std::fs::read_to_string(file)
                            .context("Failed to read proposal file")?;
                        let prop_file: ProposalFile = serde_json::from_str(&json)
                            .context("Failed to parse .jota-proposal file")?;
                        prop_file.validate(network_name)?;

                        let committee = prop_file.multisig.to_committee()?;
                        let multisig_addr = committee.derive_address();

                        // Decode and describe the transaction
                        let tx_bytes = Base64::decode_vec(&prop_file.tx_bytes)
                            .context("Invalid base64 in proposal tx_bytes")?;
                        let desc = crate::multisig::describe_transaction(&tx_bytes)?;

                        let mut out =
                            format!("Transaction Proposal\n  From:    {}\n", multisig_addr);
                        if let Some(amt) = desc.amount {
                            out.push_str(&format!(
                                "  Action:  Transfer {} ",
                                display::format_balance(amt)
                            ));
                            if let Some(ref r) = desc.recipient {
                                out.push_str(&format!("-> {}", r));
                            }
                            out.push('\n');
                        }
                        out.push_str(&format!(
                            "  Gas:     {} nanos\n  Network: {}\n",
                            desc.gas_budget, network_name,
                        ));

                        // Sign with local key
                        let (member_key, member_sig) = crate::multisig::sign_proposal(
                            service.network(),
                            service.signer().as_ref(),
                            &tx_bytes,
                        )
                        .await?;

                        // Find our label
                        let our_label = committee
                            .members()
                            .iter()
                            .enumerate()
                            .find(|(_, m)| m.public_key() == &member_key)
                            .and_then(|(i, _)| prop_file.multisig.members.get(i))
                            .map(|m| m.label.as_str())
                            .unwrap_or("unknown");

                        out.push_str(&format!("  Signed as: {}\n", our_label));

                        // Export .jota-sig
                        let tx_digest = crate::multisig::compute_tx_digest(&tx_bytes)?;
                        let short_id = &tx_digest[..8.min(tx_digest.len())];
                        let now = timestamp_now();

                        let sig_file = SignatureFile {
                            version: 1,
                            file_type: "signature".to_string(),
                            multisig_address: multisig_addr.to_string(),
                            tx_digest: tx_digest.clone(),
                            public_key: member_key,
                            signature: member_sig,
                            signed_at: format_timestamp(now),
                        };

                        let sig_json = serde_json::to_string_pretty(&sig_file)?;
                        let sig_filename = format!("{}-{}.jota-sig", our_label, short_id);
                        std::fs::write(&sig_filename, &sig_json)
                            .with_context(|| format!("Failed to write {sig_filename}"))?;

                        out.push_str(&format!(
                            "Exported: {} — send this back to the proposer",
                            sig_filename
                        ));

                        // Also update the proposal locally if we have it
                        if let Ok(local_store) = MultisigStore::open() {
                            let has_config = local_store.list_configs()?.iter().any(|c| {
                                c.address().to_string().to_lowercase()
                                    == multisig_addr.to_string().to_lowercase()
                            });

                            if has_config {
                                if let Ok(mut local_proposal) =
                                    local_store.find_proposal_by_prefix(short_id)
                                {
                                    let _ = crate::multisig::merge_signatures(
                                        &committee,
                                        &mut local_proposal,
                                        &prop_file.signatures,
                                    );
                                    let _ = local_store.save_proposal(&local_proposal);
                                }
                            }
                        }

                        Ok(out)
                    }
                }
            }

            Command::Help { command } => Ok(help_text(command.as_deref())),

            // Handled directly in the REPL loop (needs mutable service access)
            Command::Reconnect => Ok(String::new()),

            Command::Exit => Ok(String::new()),
        }
    }
}

/// Human-readable label for a multisig member's key scheme.
fn format_scheme(pk: &MultisigMemberPublicKey) -> &'static str {
    match pk {
        MultisigMemberPublicKey::Ed25519(_) => "ed25519",
        MultisigMemberPublicKey::Secp256k1(_) => "secp256k1",
        MultisigMemberPublicKey::Secp256r1(_) => "secp256r1",
        _ => "unknown",
    }
}

/// Format a unix timestamp as ISO 8601 UTC.
fn format_timestamp(unix_secs: i64) -> String {
    let secs = unix_secs as u64;
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Civil calendar date from days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Current unix timestamp in seconds.
fn timestamp_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
