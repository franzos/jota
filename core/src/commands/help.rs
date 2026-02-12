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
            "transfer <address|name.iota> <amount> [token]\n  Send IOTA or tokens to an address or .iota name.\n  Token can be a symbol (e.g. 'usdt') or full coin type.\n  Default: IOTA.\n  Examples: transfer franz.iota 1.5\n           transfer franz.iota 50 usdt\n  Alias: send".to_string()
        }
        Some("sweep_all") | Some("sweep") => {
            "sweep_all <address|name.iota> [token]\n  Sweep entire balance to an address or .iota name.\n  Optionally specify a token to sweep (default: IOTA).\n  Examples: sweep_all franz.iota\n           sweep_all franz.iota usdt\n  Alias: sweep".to_string()
        }
        Some("show_transfers") | Some("transfers") | Some("txs") => {
            "show_transfers [in|out|all]\n  Show transaction history.\n  Filter: 'in' (received), 'out' (sent), 'all' (default).\n  Aliases: transfers, txs".to_string()
        }
        Some("show_transfer") | Some("tx") => {
            "show_transfer <digest>\n  Look up a specific transaction by its digest.\n  Alias: tx".to_string()
        }
        Some("stake") => {
            "stake <validator_address|name.iota> <amount>\n  Stake IOTA to a validator (address or .iota name).\n  Amount is in IOTA (e.g. '1.5' for 1.5 IOTA).\n  Find validators at https://explorer.iota.org/validators".to_string()
        }
        Some("unstake") => {
            "unstake <staked_object_id>\n  Unstake a previously staked IOTA object.\n  Use 'stakes' to find object IDs.".to_string()
        }
        Some("stakes") => {
            "stakes\n  Show all active stakes for this wallet.".to_string()
        }
        Some("tokens") | Some("token_balances") => {
            "tokens\n  Show all coin/token balances for this wallet.\n  Alias: token_balances".to_string()
        }
        Some("status") => {
            "status [node_url]\n  Show current epoch, gas price, network, and node URL.\n  Optionally query a different node.".to_string()
        }
        Some("faucet") => {
            "faucet\n  Request test tokens from the faucet.\n  Only available on testnet and devnet.".to_string()
        }
        Some("seed") => {
            "seed\n  Display the wallet's seed phrase (mnemonic).\n  Keep this secret!".to_string()
        }
        Some("account") | Some("acc") => {
            "account [index]\n  Show current account and known accounts, or switch.\n  Example: account 3\n  Each account derives a unique address from the same seed.\n  Alias: acc".to_string()
        }
        Some("password") | Some("passwd") => {
            "password\n  Change the wallet's encryption password.\n  Alias: passwd".to_string()
        }
        Some("notarize") => {
            "notarize <message>\n  Create a locked notarization on-chain.\n  Posts a timestamped record via the IOTA notarization Move module.\n  Requires --notarization-package or IOTA_NOTARIZATION_PKG_ID.".to_string()
        }
        Some("sign_message") | Some("sign") => {
            "sign_message <message>\n  Sign a message with the wallet's private key.\n  Returns base64-encoded signature and public key.\n  Alias: sign".to_string()
        }
        Some("verify_message") | Some("verify") => {
            "verify <message> <signature_b64> <public_key_b64>\n  Verify a signed message.\n  All three arguments are required.\n  Alias: verify".to_string()
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
             \x20 transfer         Send IOTA or tokens to an address or .iota name\n\
             \x20 sweep_all        Sweep entire balance (IOTA or token) to an address\n\
             \x20 show_transfers   Show transaction history\n\
             \x20 show_transfer    Look up a transaction by digest\n\
             \x20 stake            Stake IOTA to a validator\n\
             \x20 unstake          Unstake a staked IOTA object\n\
             \x20 stakes           Show active stakes\n\
             \x20 tokens           Show token balances\n\
             \x20 status           Show network status\n\
             \x20 faucet           Request testnet/devnet tokens\n\
             \x20 sign_message      Sign a message with your key\n\
             \x20 verify            Verify a signed message\n\
             \x20 notarize          Notarize a message on-chain\n\
             \x20 seed             Show seed phrase\n\
             \x20 account          Show or switch account\n\
             \x20 password         Change wallet password\n\
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
    fn help_text_general() {
        let text = help_text(None);
        assert!(text.contains("balance"));
        assert!(text.contains("transfer"));
        assert!(text.contains("faucet"));
    }

    #[test]
    fn help_text_specific() {
        let text = help_text(Some("transfer"));
        assert!(text.contains("<address|name.iota>"));
        assert!(text.contains("<amount>"));
    }

    #[test]
    fn help_text_unknown() {
        let text = help_text(Some("nonexistent"));
        assert!(text.contains("Unknown command"));
    }
}
