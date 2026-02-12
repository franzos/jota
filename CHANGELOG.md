## [0.1.5] - 2026-02-12

### Added
- Early support for NFT's
- Early support for L1 (future USDT?)
- Message signing and verification

## [0.1.4] - 2026-02-11

### Added
- Multi-account support: derive multiple addresses from the same seed

### Fixed
- GUI: Copying to clipboard on Wayland

## [0.1.3] - 2026-02-10

### Changed
- GUI: Reuse network client across operations instead of creating one per request
- GUI: Cache theme object instead of allocating every frame
- GUI: Use existing derived key instead of re-deriving from mnemonic on wallet open
- Transaction deduplication uses HashSet for O(n) instead of O(n*m) linear scan

### Fixed
- GUI: Change Password now works after creating or recovering a wallet in the same session
- GUI: Expanded transaction detail resets on page navigation
- GUI: Account page no longer shows stale data after browsing history pages
- Transaction cache database now has restrictive file permissions (0o600/0o700) on Unix
- Faucet command now checks runtime network config, not just stored wallet config
- Removed unnecessary JSON deep clones in staking and token balance queries
- CLI no longer panics if `$HOME` is unset; returns an actionable error instead

## [0.1.2] - 2026-02-10

### Added
- `password` command to change wallet encryption password (CLI + GUI)
- Custom node URLs validated for HTTPS; `--insecure` flag to allow plain HTTP
- GUI: Transaction history pagination with cursor-based page navigation

### Changed
- Moved `validate_wallet_name` and `list_wallets` to core (shared by CLI and GUI)
- Extracted `sign_and_execute` helper — deduplicated transaction signing across 4 methods

### Fixed
- Transaction execution now errors on failure instead of showing "sent!" with a failure status
- Token balance display no longer truncates `u128` values to `u64`
- Sweep gas cost handled correctly for negative (rebate) values
- Library `expect()` panics replaced with `Result` propagation
- Atomic wallet file writes (write→fsync→rename) to prevent corruption on crash
- File permissions set atomically via `OpenOptions` on Unix
- GUI: secret fields (passwords, mnemonic) now zeroized from memory instead of just cleared
- GUI: wallet name validated against path traversal
- GUI: mnemonic recovery input masked on screen

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
