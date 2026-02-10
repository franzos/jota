/// REPL shell — Reedline-based interactive wallet session.
use anyhow::{Context, Result};
use iota_wallet_core::commands::Command;
use iota_wallet_core::network::NetworkClient;
use iota_wallet_core::wallet::Wallet;
use reedline::{DefaultCompleter, DefaultPrompt, DefaultPromptSegment, Reedline, Signal};
use zeroize::{Zeroize, Zeroizing};
use crate::Cli;

pub async fn run_repl(cli: &Cli) -> Result<()> {
    println!("IOTA Wallet v{}", env!("CARGO_PKG_VERSION"));
    println!("Network: {}", cli.network_config().network);
    println!();

    let wallet_dir = cli.wallet_dir();
    std::fs::create_dir_all(&wallet_dir)
        .context("Failed to create wallet directory")?;

    // List existing wallets
    let wallets = list_wallets(&wallet_dir);
    if !wallets.is_empty() {
        println!("Existing wallets:");
        for name in &wallets {
            println!("  - {name}");
        }
        println!();
    }

    let wallet_path = cli.wallet_path()?;
    let wallet_name = &cli.wallet;

    let wallet = if wallet_path.exists() {
        println!("Opening wallet '{wallet_name}'...");
        let password = Zeroizing::new(
            rpassword::prompt_password("Password: ")
                .context("Failed to read password")?,
        );
        Wallet::open(&wallet_path, password.as_bytes())?
        // password dropped and zeroized here
    } else {
        println!("Wallet '{wallet_name}' not found. Creating new wallet...");
        let action = prompt_action()?;
        if matches!(action, WalletAction::Quit) {
            println!("Goodbye.");
            return Ok(());
        }

        let password = prompt_new_password()?;
        let network_config = cli.network_config();

        match action {
            WalletAction::CreateNew => {
                let w = Wallet::create_new(
                    wallet_path.clone(),
                    password.as_bytes(),
                    network_config,
                )?;
                println!();
                println!("New wallet created in {}", wallet_path.display());
                println!("IMPORTANT: Write down your seed phrase and keep it safe:");
                println!("  {}", w.mnemonic());
                println!();
                w
            }
            WalletAction::Recover => {
                let mnemonic = prompt_mnemonic()?;
                let w = Wallet::recover_from_mnemonic(
                    wallet_path.clone(),
                    password.as_bytes(),
                    &mnemonic,
                    network_config,
                )?;
                println!();
                println!("Wallet recovered!");
                w
            }
            WalletAction::Quit => unreachable!(),
        }
        // password and mnemonic dropped and zeroized here
    };

    let effective_config = cli.resolve_network_config(wallet.network_config());
    let network = NetworkClient::new(&effective_config)?;

    println!(
        "Wallet ready. Address: {}",
        wallet.address()
    );
    println!("Type 'help' for a list of commands.");
    println!();

    // Build the REPL prompt
    let prompt_str = format!("[wallet {}]", wallet.short_address());
    let prompt = DefaultPrompt::new(
        DefaultPromptSegment::Basic(prompt_str),
        DefaultPromptSegment::Empty,
    );

    let commands: Vec<String> = vec![
        "balance".into(), "bal".into(),
        "address".into(), "addr".into(),
        "transfer".into(), "send".into(),
        "sweep_all".into(), "sweep".into(),
        "show_transfers".into(), "transfers".into(), "txs".into(),
        "show_transfer".into(), "tx".into(),
        "stake".into(), "unstake".into(), "stakes".into(), "status".into(),
        "faucet".into(),
        "seed".into(),
        "help".into(),
        "exit".into(), "quit".into(), "q".into(),
    ];
    let completer = Box::new(DefaultCompleter::new(commands));
    let mut line_editor = Reedline::create().with_completer(completer);

    loop {
        match line_editor.read_line(&prompt) {
            Ok(Signal::Success(line)) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                match Command::parse(line) {
                    Ok(Command::Exit) => {
                        println!("Goodbye.");
                        break;
                    }
                    Ok(cmd) => {
                        if let Some(prompt_msg) = cmd.confirmation_prompt() {
                            print!("{prompt_msg} [y/N]: ");
                            use std::io::Write;
                            std::io::stdout().flush().ok();
                            let mut confirm = String::new();
                            if std::io::stdin().read_line(&mut confirm).is_err() {
                                println!("Cancelled.");
                                continue;
                            }
                            if confirm.trim().to_lowercase() != "y" {
                                println!("Cancelled.");
                                continue;
                            }
                        }
                        match cmd.execute(&wallet, &network, false).await {
                            Ok(output) => {
                                if !output.is_empty() {
                                    println!("{output}");
                                }
                            }
                            Err(e) => {
                                eprintln!("Error: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("{e}");
                    }
                }
            }
            Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
                println!("Goodbye.");
                break;
            }
            Err(e) => {
                eprintln!("Input error: {e}");
                break;
            }
        }
    }

    Ok(())
}

fn list_wallets(dir: &std::path::Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "wallet").unwrap_or(false) {
                if let Some(stem) = path.file_stem() {
                    names.push(stem.to_string_lossy().to_string());
                }
            }
        }
    }
    names.sort();
    names
}

enum WalletAction {
    CreateNew,
    Recover,
    Quit,
}

fn prompt_action() -> Result<WalletAction> {
    println!("  1) Create new wallet");
    println!("  2) Recover from seed phrase");
    println!("  3) Quit");
    loop {
        let mut input = String::new();
        print!("Choice [1/2/3]: ");
        use std::io::Write;
        std::io::stdout().flush()?;
        std::io::stdin().read_line(&mut input)?;
        match input.trim() {
            "1" | "" => return Ok(WalletAction::CreateNew),
            "2" => return Ok(WalletAction::Recover),
            "3" | "q" => return Ok(WalletAction::Quit),
            _ => println!("Please enter 1, 2, or 3."),
        }
    }
}

fn prompt_new_password() -> Result<Zeroizing<String>> {
    loop {
        let pass1 = Zeroizing::new(
            rpassword::prompt_password("New password: ")
                .context("Failed to read password")?,
        );
        let pass2 = Zeroizing::new(
            rpassword::prompt_password("Confirm password: ")
                .context("Failed to read password")?,
        );
        if *pass1 != *pass2 {
            println!("Passwords do not match. Try again.");
            continue;
        }
        return Ok(pass1);
    }
}

fn prompt_mnemonic() -> Result<Zeroizing<String>> {
    println!("Enter your seed phrase (12 or 24 words, space-separated):");
    let mut input = rpassword::prompt_password("Seed phrase: ")
        .context("Failed to read seed phrase")?;
    let trimmed = Zeroizing::new(input.trim().to_string());
    input.zeroize();
    if trimmed.split_whitespace().count() < 12 {
        anyhow::bail!("Seed phrase should be at least 12 words.");
    }
    Ok(trimmed)
}
