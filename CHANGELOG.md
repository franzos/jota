# Changelog

## [0.1.1] - 2026-02-10

### Added
- Staking support: `stake`, `unstake`, and `stakes` commands
- `sweep_all` command to send entire balance minus gas
- `show_transfer` command to look up a transaction by digest
- `tokens` command to show all coin/token balances
- `status` command: shows current epoch, gas price, network, and node URL; accepts optional custom node
- GUI frontend (`iota-wallet-gui`) using iced

### Changed
- Transfer and stake now require confirmation before signing
- Transaction history sorted by epoch and lamport version (newest first)

### Fixed
- Wallet address no longer printed twice on creation/recovery
