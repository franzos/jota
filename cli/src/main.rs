mod repl;

use anyhow::{Context, Result, bail};
use clap::Parser;
use iota_wallet_core::commands::Command;
use iota_wallet_core::network::NetworkClient;
use iota_wallet_core::service::WalletService;
use iota_wallet_core::validate_wallet_name;
use iota_wallet_core::wallet::{Network, NetworkConfig, Wallet};
use iota_wallet_core::ObjectId;
use std::path::PathBuf;
use std::sync::Arc;
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(name = "iota-wallet", about = "IOTA Wallet — Monero-inspired REPL", version)]
pub(crate) struct Cli {
    /// Wallet name (default: "default")
    #[arg(long, default_value = "default")]
    wallet: String,

    /// Wallet directory (default: ~/.iota-wallet)
    #[arg(long)]
    wallet_dir: Option<PathBuf>,

    /// Read password from stdin (for scripting)
    #[arg(long)]
    password_stdin: bool,

    /// Run a single command and exit
    #[arg(long)]
    cmd: Option<String>,

    /// Use testnet (default)
    #[arg(long)]
    testnet: bool,

    /// Use mainnet
    #[arg(long)]
    mainnet: bool,

    /// Use devnet
    #[arg(long)]
    devnet: bool,

    /// Custom node URL
    #[arg(long)]
    node: Option<String>,

    /// Output in JSON format (useful with --cmd)
    #[arg(long)]
    json: bool,

    /// Allow connecting to non-HTTPS node URLs
    #[arg(long)]
    insecure: bool,

    /// Account index to use (default: stored in wallet, initially 0)
    #[arg(long)]
    account: Option<u64>,

    /// On-chain notarization package ID (or set IOTA_NOTARIZATION_PKG_ID)
    #[arg(long, env = "IOTA_NOTARIZATION_PKG_ID")]
    notarization_package: Option<String>,

}

impl Cli {
    fn wallet_dir(&self) -> Result<PathBuf> {
        match &self.wallet_dir {
            Some(dir) => Ok(dir.clone()),
            None => Ok(dirs::home_dir()
                .context("Cannot determine home directory. Set $HOME or use --wallet-dir.")?
                .join(".iota-wallet")),
        }
    }

    fn wallet_path(&self) -> Result<PathBuf> {
        validate_wallet_name(&self.wallet)?;
        Ok(self.wallet_dir()?.join(format!("{}.wallet", self.wallet)))
    }

    fn network_config(&self) -> NetworkConfig {
        if let Some(url) = &self.node {
            NetworkConfig {
                network: Network::Custom,
                custom_url: Some(url.clone()),
            }
        } else if self.mainnet {
            NetworkConfig {
                network: Network::Mainnet,
                custom_url: None,
            }
        } else if self.devnet {
            NetworkConfig {
                network: Network::Devnet,
                custom_url: None,
            }
        } else {
            // Default: testnet
            NetworkConfig {
                network: Network::Testnet,
                custom_url: None,
            }
        }
    }

    /// Check whether the user explicitly set any network flag on the CLI.
    fn has_explicit_network_flags(&self) -> bool {
        self.testnet || self.mainnet || self.devnet || self.node.is_some()
    }

    /// Validate that at most one network flag is set.
    fn validate_network_flags(&self) -> Result<()> {
        let count = self.testnet as u8
            + self.mainnet as u8
            + self.devnet as u8
            + self.node.is_some() as u8;
        if count > 1 {
            bail!(
                "Conflicting network flags. Use only one of --testnet, --mainnet, --devnet, or --node."
            );
        }
        Ok(())
    }

    fn notarization_package_id(&self) -> Result<Option<ObjectId>> {
        match &self.notarization_package {
            Some(s) => {
                let id = ObjectId::from_hex(s)
                    .map_err(|e| anyhow::anyhow!("Invalid notarization package ID '{s}': {e}"))?;
                Ok(Some(id))
            }
            None => Ok(None),
        }
    }

    /// Resolve the effective network config, preferring explicit CLI flags over
    /// the wallet's stored config. Warns if the CLI overrides a different stored value.
    fn resolve_network_config(&self, stored: &NetworkConfig) -> NetworkConfig {
        if self.has_explicit_network_flags() {
            let cli_config = self.network_config();
            if cli_config != *stored {
                eprintln!(
                    "Warning: CLI network flag ({}) overrides wallet's stored network ({})",
                    cli_config.network, stored.network
                );
            }
            cli_config
        } else {
            stored.clone()
        }
    }
}


fn read_password_stdin() -> Result<Zeroizing<String>> {
    let mut password = String::new();
    std::io::stdin()
        .read_line(&mut password)
        .context("Failed to read password from stdin")?;
    let trimmed = password
        .trim_end_matches('\n')
        .trim_end_matches('\r')
        .to_string();
    use zeroize::Zeroize;
    password.zeroize();
    Ok(Zeroizing::new(trimmed))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.validate_network_flags()?;

    if let Some(cmd_str) = &cli.cmd {
        // One-shot mode
        run_oneshot(&cli, cmd_str).await
    } else {
        // REPL mode
        repl::run_repl(&cli).await
    }
}

async fn run_oneshot(cli: &Cli, cmd_str: &str) -> Result<()> {
    let password = if cli.password_stdin {
        read_password_stdin()?
    } else {
        Zeroizing::new(
            rpassword::prompt_password("Password: ")
                .context("Failed to read password")?,
        )
    };

    let wallet_path = cli.wallet_path()?;
    if !wallet_path.exists() {
        bail!(
            "Wallet file not found: {}. Create one first by running iota-wallet without --cmd.",
            wallet_path.display()
        );
    }

    let mut wallet = Wallet::open(&wallet_path, password.as_bytes())?;
    if let Some(idx) = cli.account {
        wallet.switch_account(idx)?;
    }
    let effective_config = cli.resolve_network_config(wallet.network_config());
    let network = NetworkClient::new(&effective_config, cli.insecure)?;
    let notarization_pkg = cli.notarization_package_id()?;

    let signer: Arc<dyn iota_wallet_core::Signer> = build_signer(&wallet, cli)?;

    let service = WalletService::new(
        network,
        signer,
        effective_config.network.to_string(),
    )
    .with_notarization_package(notarization_pkg);

    let command = Command::parse(cmd_str)?;
    if command == Command::Exit {
        return Ok(());
    }

    // Resolve .iota names before execution
    let resolved = if let Some(r) = command.recipient() {
        Some(service.resolve_recipient(r).await?)
    } else {
        None
    };

    let output = command.execute(&wallet, &service, cli.json, cli.insecure, resolved.as_ref()).await?;
    if !output.is_empty() {
        println!("{output}");
    }

    Ok(())
}

/// Build the appropriate signer based on wallet type.
fn build_signer(wallet: &Wallet, _cli: &Cli) -> Result<Arc<dyn iota_wallet_core::Signer>> {
    if wallet.is_hardware() {
        #[cfg(feature = "ledger")]
        {
            use iota_wallet_core::ledger_signer::connect_and_verify;

            let path = iota_wallet_core::bip32_path_for(
                wallet.network_config().network,
                wallet.account_index() as u32,
            );

            println!("Connecting to hardware wallet...");
            let ledger_signer = connect_and_verify(path, wallet.address())?;
            return Ok(Arc::new(ledger_signer));
        }
        #[cfg(not(feature = "ledger"))]
        anyhow::bail!("Hardware wallet support not compiled in.");
    }

    Ok(Arc::new(wallet.signer()?))
}
