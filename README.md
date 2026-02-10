# iota-wallet

A Monero-inspired REPL wallet for IOTA Rebased. Supports interactive mode, one-shot commands for scripting, and encrypted wallet files.

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

## Commands

| Command | Description |
|---------|-------------|
| `balance` | Show wallet balance |
| `address` | Show wallet address |
| `transfer <addr> <amount>` | Send IOTA to an address |
| `sweep_all <addr>` | Send entire balance minus gas |
| `show_transfers [in\|out\|all]` | Show transaction history |
| `show_transfer <digest>` | Look up a transaction by digest |
| `stake <validator> <amount>` | Stake IOTA to a [validator](https://explorer.iota.org/validators) |
| `unstake <object_id>` | Unstake a staked IOTA object |
| `stakes` | Show active stakes and rewards |
| `tokens` | Show all coin/token balances |
| `status [node_url]` | Show epoch, gas price, network, and node URL |
| `faucet` | Request testnet/devnet tokens |
| `seed` | Display seed phrase (requires confirmation) |
| `help [cmd]` | Show help |
| `exit` | Exit the wallet |

Amounts are in IOTA (e.g. `1.5` for 1.5 IOTA). Tab completion is available in the REPL.

## Network

Testnet by default. Override with flags:

```bash
iota-wallet --mainnet
iota-wallet --devnet
iota-wallet --node https://custom-graphql-endpoint.example.com
```

The network is stored in the wallet file. CLI flags override the stored config if explicitly set.

## Storage

Wallet files live in `~/.iota-wallet/`:

```
~/.iota-wallet/
├── default.wallet    # encrypted (mode 0600)
└── mywallet.wallet
```

File format: argon2id-derived key + AES-256-GCM. Override the directory with `--wallet-dir`.

## Building

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
