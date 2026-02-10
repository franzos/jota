# IOTA Wallet

A Monero-inspired REPL/GUI wallet for IOTA Rebased. Supports interactive mode, one-shot commands for scripting, a native GUI, and encrypted wallet files.

NOTE: This is an early prototype. Use at your own risk. Always test with small amounts first and verify transactions on the explorer.

## Install

**From source:**

```bash
cargo install --git https://github.com/franzos/iota-wallet
```

**Pre-built binaries:**

Download the latest release from [GitHub Releases](https://github.com/franzos/iota-wallet/releases):

```bash
# Linux (x86_64)
curl -sL https://github.com/franzos/iota-wallet/releases/latest/download/iota-wallet-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv iota-wallet /usr/local/bin/

# macOS (Apple Silicon)
curl -sL https://github.com/franzos/iota-wallet/releases/latest/download/iota-wallet-aarch64-apple-darwin.tar.gz | tar xz
sudo mv iota-wallet /usr/local/bin/

# macOS (Intel)
curl -sL https://github.com/franzos/iota-wallet/releases/latest/download/iota-wallet-x86_64-apple-darwin.tar.gz | tar xz
sudo mv iota-wallet /usr/local/bin/
```

## Quick start

```bash
# Launch the REPL (testnet by default)
iota-wallet

# Use a named wallet
iota-wallet --wallet mywallet

# One-shot commands for scripting
iota-wallet --cmd "balance"
iota-wallet --cmd "address" --json
iota-wallet --wallet mywallet --password-stdin --cmd "balance" < password.txt
```

On first launch you'll be prompted to create a new wallet or recover from a seed phrase. The wallet file is encrypted with your password (argon2id + AES-256-GCM).

## GUI

Launch the GUI with:

```bash
iota-wallet-gui
iota-wallet-gui --mainnet
iota-wallet-gui --devnet
```

The GUI supports wallet creation, recovery, sending/receiving IOTA, transaction history with pagination, staking/unstaking, a balance chart, and password changes. The GUI requires X11 or Wayland on Linux.

## Commands

| Command | Aliases | Description |
|---------|---------|-------------|
| `balance` | `bal` | Show wallet balance |
| `address` | `addr` | Show wallet address |
| `transfer <addr> <amount>` | `send` | Send IOTA to an address |
| `sweep_all <addr>` | `sweep` | Send entire balance minus gas |
| `show_transfers [in\|out\|all]` | `transfers`, `txs` | Show transaction history |
| `show_transfer <digest>` | `tx` | Look up a transaction by digest |
| `stake <validator> <amount>` | | Stake IOTA to a [validator](https://explorer.iota.org/validators) |
| `unstake <object_id>` | | Unstake a staked IOTA object |
| `stakes` | | Show active stakes and rewards |
| `tokens` | `token_balances` | Show all coin/token balances |
| `status [node_url]` | | Show epoch, gas price, network, and node URL |
| `faucet` | | Request testnet/devnet tokens |
| `seed` | | Display seed phrase (requires confirmation) |
| `password` | `passwd` | Change wallet encryption password |
| `help [cmd]` | | Show help |
| `exit` | `quit`, `q` | Exit the wallet |

Amounts are in IOTA (e.g. `1.5` for 1.5 IOTA). Tab completion is available in the REPL. All commands support `--json` output.

## Network

Testnet by default. Override with flags:

```bash
iota-wallet --mainnet
iota-wallet --devnet
iota-wallet --node https://custom-graphql-endpoint.example.com
iota-wallet --node http://localhost:9125/graphql --insecure
```

The `--insecure` flag allows plain HTTP connections (for local development). The network is stored in the wallet file. CLI flags override the stored config if explicitly set.

## Storage

Wallet files live in `~/.iota-wallet/`. Transactions are cached locally for pagination and balance history.

```
~/.iota-wallet/
├── default.wallet    # encrypted (mode 0600)
└── mywallet.wallet

# Linux: ~/.local/share/iota-wallet/
# macOS: ~/Library/Application Support/iota-wallet/
└── transactions.db   # SQLite cache (mode 0600)
```

File format: argon2id-derived key + AES-256-GCM. Override the wallet directory with `--wallet-dir`.

## Building

This is a Cargo workspace with three crates: `core` (shared library), `cli`, and `gui`.

```bash
cargo build --release
```

Run tests:

```bash
# Unit tests
cargo test

# Integration tests (hits testnet/devnet)
cargo test -- --ignored
```

## License

MIT
