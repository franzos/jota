use crate::Cli;
/// REPL shell — Reedline-based interactive wallet session.
use anyhow::{Context, Result};
use iota_wallet_core::commands::Command;
use iota_wallet_core::list_wallets;
use iota_wallet_core::network::NetworkClient;
use iota_wallet_core::service::WalletService;
use iota_wallet_core::wallet::Wallet;
use reedline::{DefaultCompleter, DefaultPrompt, DefaultPromptSegment, Reedline, Signal};
use std::sync::Arc;
use zeroize::{Zeroize, Zeroizing};

pub async fn run_repl(cli: &Cli) -> Result<()> {
    println!("IOTA Wallet v{}", env!("CARGO_PKG_VERSION"));
    println!("Network: {}", cli.network_config().network);
    println!();

    let wallet_dir = cli.wallet_dir()?;
    std::fs::create_dir_all(&wallet_dir).context("Failed to create wallet directory")?;

    // List existing wallets
    let wallets = list_wallets(&wallet_dir);
    if !wallets.is_empty() {
        println!("Existing wallets:");
        for entry in &wallets {
            let suffix = match entry.wallet_type {
                iota_wallet_core::WalletType::Hardware(kind) => format!(" ({kind})"),
                _ => String::new(),
            };
            println!("  - {}{suffix}", entry.name);
        }
        println!();
    }

    let wallet_path = cli.wallet_path()?;
    let wallet_name = &cli.wallet;

    let (mut wallet, mut session_password) = if wallet_path.exists() {
        println!("Opening wallet '{wallet_name}'...");
        let password = Zeroizing::new(
            rpassword::prompt_password("Password: ").context("Failed to read password")?,
        );
        #[allow(unused_mut)]
        let mut w = Wallet::open(&wallet_path, password.as_bytes())?;

        // For hardware wallets, verify the device is connected and address matches
        if w.is_hardware() {
            #[cfg(feature = "ledger")]
            {
                use iota_wallet_core::ledger_signer::connect_and_verify;

                let path = iota_wallet_core::bip32_path_for(
                    w.network_config().network,
                    w.account_index() as u32,
                );
                println!("Connecting to hardware wallet...");
                connect_and_verify(path, w.address())?;
            }
            #[cfg(not(feature = "ledger"))]
            {
                anyhow::bail!("Hardware wallet support not compiled in.");
            }
        }

        let pw_bytes = Zeroizing::new(password.as_bytes().to_vec());
        (w, pw_bytes)
    } else {
        println!("Wallet '{wallet_name}' not found. Creating new wallet...");
        let action = prompt_action()?;
        if matches!(action, WalletAction::Quit) {
            println!("Goodbye.");
            return Ok(());
        }

        let password = prompt_new_password()?;
        let pw_bytes = Zeroizing::new(password.as_bytes().to_vec());
        let network_config = cli.network_config();

        let w = match action {
            WalletAction::CreateNew => {
                let w =
                    Wallet::create_new(wallet_path.clone(), password.as_bytes(), network_config)?;
                println!();
                println!("New wallet created in {}", wallet_path.display());
                println!("IMPORTANT: Write down your seed phrase and keep it safe:");
                match w.mnemonic() {
                    Some(mnemonic) => println!("  {}", mnemonic),
                    None => println!("  Mnemonic not available for hardware wallets."),
                }
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
            #[cfg(feature = "ledger")]
            WalletAction::ConnectHardware => {
                use iota_wallet_core::ledger_signer::connect_with_verification;
                use iota_wallet_core::Signer;

                let path = iota_wallet_core::bip32_path_for(network_config.network, 0);
                println!("Connecting to hardware wallet...");
                println!("Verify the address on your device...");
                let signer = connect_with_verification(path)?;
                println!("Address: {}", signer.address());
                println!("Address confirmed.");

                let address = *signer.address();
                let w = Wallet::create_hardware(
                    wallet_path.clone(),
                    password.as_bytes(),
                    address,
                    network_config,
                    iota_wallet_core::HardwareKind::Ledger,
                )?;
                println!();
                println!("Hardware wallet created in {}", wallet_path.display());
                w
            }
            WalletAction::Quit => unreachable!(),
        };
        (w, pw_bytes)
        // password and mnemonic dropped and zeroized here
    };

    // Apply --account override if set
    if let Some(idx) = cli.account {
        wallet.switch_account(idx)?;
        wallet.save(&session_password)?;
    }

    let effective_config = cli.resolve_network_config(wallet.network_config());
    let network = NetworkClient::new(&effective_config, cli.insecure)?;
    let notarization_pkg = cli.notarization_package_id()?;

    let signer: Arc<dyn iota_wallet_core::Signer> = build_repl_signer(&wallet, cli)?;

    let mut service =
        WalletService::new(network, signer).with_notarization_package(notarization_pkg);

    println!("Wallet ready. Address: {}", wallet.address());
    println!("Type 'help' for a list of commands.");
    println!();

    // Build the REPL prompt
    let prompt_str = format!("[wallet {}]", wallet.short_address());
    let mut prompt = DefaultPrompt::new(
        DefaultPromptSegment::Basic(prompt_str),
        DefaultPromptSegment::Empty,
    );

    let commands: Vec<String> = vec![
        "balance".into(),
        "bal".into(),
        "address".into(),
        "addr".into(),
        "transfer".into(),
        "send".into(),
        "sweep_all".into(),
        "sweep".into(),
        "show_transfers".into(),
        "transfers".into(),
        "txs".into(),
        "show_transfer".into(),
        "tx".into(),
        "stake".into(),
        "unstake".into(),
        "stakes".into(),
        "sign_message".into(),
        "sign".into(),
        "verify_message".into(),
        "verify".into(),
        "notarize".into(),
        "nfts".into(),
        "send_nft".into(),
        "tokens".into(),
        "token_balances".into(),
        "status".into(),
        "faucet".into(),
        "seed".into(),
        "account".into(),
        "acc".into(),
        "reconnect".into(),
        "password".into(),
        "passwd".into(),
        "help".into(),
        "exit".into(),
        "quit".into(),
        "q".into(),
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
                    Ok(Command::Account { index: Some(idx) }) => {
                        match wallet.switch_account(idx) {
                            Ok(()) => {
                                // For hardware wallets, reconnect the device with the new path
                                #[cfg(feature = "ledger")]
                                if wallet.is_hardware() {
                                    use iota_wallet_core::ledger_signer::connect_with_verification;
                                    use iota_wallet_core::Signer;
                                    let path = iota_wallet_core::bip32_path_for(
                                        wallet.network_config().network,
                                        idx as u32,
                                    );
                                    match connect_with_verification(path) {
                                        Ok(new_signer) => {
                                            wallet.set_address(*new_signer.address());
                                            if let Err(e) = wallet.save(&session_password) {
                                                eprintln!("Error saving wallet: {e}");
                                                continue;
                                            }
                                            let network = NetworkClient::new(
                                                &effective_config,
                                                cli.insecure,
                                            )?;
                                            service =
                                                WalletService::new(network, Arc::new(new_signer))
                                                    .with_notarization_package(notarization_pkg);
                                        }
                                        Err(e) => {
                                            eprintln!("Error connecting to hardware wallet: {e}");
                                            continue;
                                        }
                                    }
                                    let prompt_str = format!("[wallet {}]", wallet.short_address());
                                    prompt = DefaultPrompt::new(
                                        DefaultPromptSegment::Basic(prompt_str),
                                        DefaultPromptSegment::Empty,
                                    );
                                    println!(
                                        "Switched to account #{idx}. Address: {}",
                                        wallet.address()
                                    );
                                    continue;
                                }

                                if let Err(e) = wallet.save(&session_password) {
                                    eprintln!("Error saving wallet: {e}");
                                    continue;
                                }
                                let network = NetworkClient::new(&effective_config, cli.insecure)?;
                                service = WalletService::new(network, Arc::new(wallet.signer()?))
                                    .with_notarization_package(notarization_pkg);
                                let prompt_str = format!("[wallet {}]", wallet.short_address());
                                prompt = DefaultPrompt::new(
                                    DefaultPromptSegment::Basic(prompt_str),
                                    DefaultPromptSegment::Empty,
                                );
                                println!(
                                    "Switched to account #{idx}. Address: {}",
                                    wallet.address()
                                );
                            }
                            Err(e) => eprintln!("Error: {e}"),
                        }
                    }
                    Ok(Command::Reconnect) => match service.reconnect_signer() {
                        Ok(()) => println!("Device reconnected."),
                        Err(e) => eprintln!("Error: {e}"),
                    },
                    Ok(Command::Password) => {
                        if !prompt_confirm("Change wallet password?") {
                            println!("Cancelled.");
                            continue;
                        }
                        let old_pw = Zeroizing::new(
                            rpassword::prompt_password("Current password: ").unwrap_or_default(),
                        );
                        let new_pw = match prompt_new_password() {
                            Ok(pw) => pw,
                            Err(e) => {
                                eprintln!("Error: {e}");
                                continue;
                            }
                        };
                        println!("Changing password...");
                        match Wallet::change_password(
                            &wallet_path,
                            old_pw.as_bytes(),
                            new_pw.as_bytes(),
                        ) {
                            Ok(()) => {
                                session_password = Zeroizing::new(new_pw.as_bytes().to_vec());
                                println!("Password changed.");
                            }
                            Err(e) => eprintln!("Error: {e}"),
                        }
                    }
                    Ok(cmd) => {
                        // Resolve .iota names before confirmation/execution
                        let resolved = if let Some(r) = cmd.recipient() {
                            match service.resolve_recipient(r).await {
                                Ok(res) => Some(res),
                                Err(e) => {
                                    eprintln!("Error resolving name: {e}");
                                    continue;
                                }
                            }
                        } else {
                            None
                        };

                        if let Some(prompt_msg) = cmd.confirmation_prompt(resolved.as_ref()) {
                            if !prompt_confirm(&prompt_msg) {
                                println!("Cancelled.");
                                continue;
                            }
                        }
                        match cmd
                            .execute(&wallet, &service, false, cli.insecure, resolved.as_ref())
                            .await
                        {
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

enum WalletAction {
    CreateNew,
    Recover,
    #[cfg(feature = "hardware-wallets")]
    ConnectHardware,
    Quit,
}

fn prompt_action() -> Result<WalletAction> {
    println!("  1) Create new wallet");
    println!("  2) Recover from seed phrase");
    #[cfg(feature = "hardware-wallets")]
    println!("  3) Connect hardware wallet");
    #[cfg(feature = "hardware-wallets")]
    println!("  4) Quit");
    #[cfg(not(feature = "hardware-wallets"))]
    println!("  3) Quit");
    loop {
        let mut input = String::new();
        #[cfg(feature = "hardware-wallets")]
        print!("Choice [1/2/3/4]: ");
        #[cfg(not(feature = "hardware-wallets"))]
        print!("Choice [1/2/3]: ");
        use std::io::Write;
        std::io::stdout().flush()?;
        std::io::stdin().read_line(&mut input)?;
        match input.trim() {
            "1" | "" => return Ok(WalletAction::CreateNew),
            "2" => return Ok(WalletAction::Recover),
            #[cfg(feature = "hardware-wallets")]
            "3" => return Ok(WalletAction::ConnectHardware),
            #[cfg(feature = "hardware-wallets")]
            "4" | "q" => return Ok(WalletAction::Quit),
            #[cfg(not(feature = "hardware-wallets"))]
            "3" | "q" => return Ok(WalletAction::Quit),
            _ => println!("Please enter a valid option."),
        }
    }
}

fn prompt_new_password() -> Result<Zeroizing<String>> {
    loop {
        let pass1 = Zeroizing::new(
            rpassword::prompt_password("New password: ").context("Failed to read password")?,
        );
        let pass2 = Zeroizing::new(
            rpassword::prompt_password("Confirm password: ").context("Failed to read password")?,
        );
        if *pass1 != *pass2 {
            println!("Passwords do not match. Try again.");
            continue;
        }
        if pass1.len() < 4 {
            eprintln!("WARNING: This password is very short. A weak password offers little protection if the wallet file is stolen.");
        }
        return Ok(pass1);
    }
}

fn prompt_confirm(prompt: &str) -> bool {
    use std::io::Write;
    print!("{prompt} [y/N]: ");
    std::io::stdout().flush().ok();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).is_ok() && input.trim().eq_ignore_ascii_case("y")
}

fn prompt_mnemonic() -> Result<Zeroizing<String>> {
    println!("Enter your seed phrase (12 or 24 words, space-separated):");
    let mut input =
        rpassword::prompt_password("Seed phrase: ").context("Failed to read seed phrase")?;
    let trimmed = Zeroizing::new(input.trim().to_string());
    input.zeroize();
    if trimmed.split_whitespace().count() < 12 {
        anyhow::bail!("Seed phrase should be at least 12 words.");
    }
    Ok(trimmed)
}

/// Build the appropriate signer for the REPL session.
fn build_repl_signer(wallet: &Wallet, _cli: &Cli) -> Result<Arc<dyn iota_wallet_core::Signer>> {
    if wallet.is_hardware() {
        #[cfg(feature = "ledger")]
        {
            use iota_wallet_core::ledger_signer::connect_and_verify;
            let path = iota_wallet_core::bip32_path_for(
                wallet.network_config().network,
                wallet.account_index() as u32,
            );
            let signer = connect_and_verify(path, wallet.address())?;
            return Ok(Arc::new(signer));
        }
        #[cfg(not(feature = "ledger"))]
        anyhow::bail!("Hardware wallet support not compiled in.");
    }

    Ok(Arc::new(wallet.signer()?))
}
