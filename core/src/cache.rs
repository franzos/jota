/// SQLite-backed transaction cache, shared across wallets.
///
/// Keyed by (network, address) so multiple wallets can share one DB.
use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::network::{TransactionDirection, TransactionFilter, TransactionSummary};

pub struct TransactionCache {
    conn: Connection,
}

pub struct TransactionPage {
    pub transactions: Vec<TransactionSummary>,
    pub total: u32,
    pub offset: u32,
    pub limit: u32,
}

impl TransactionPage {
    pub fn has_next(&self) -> bool {
        self.offset + self.limit < self.total
    }

    pub fn has_prev(&self) -> bool {
        self.offset > 0
    }
}

/// Default DB location: platform data directory + `iota-wallet/transactions.db`
/// (Linux: `~/.local/share`, macOS: `~/Library/Application Support`)
fn default_db_path() -> Result<PathBuf> {
    let data_dir = dirs::data_dir().context("Cannot determine data directory")?;
    Ok(data_dir.join("iota-wallet").join("transactions.db"))
}

impl TransactionCache {
    /// Open (or create) the shared transaction cache.
    pub fn open() -> Result<Self> {
        let path = default_db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create cache directory")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let conn = Connection::open(&path).context("Failed to open transaction cache database")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        let cache = Self { conn };
        cache.init_schema()?;
        Ok(cache)
    }

    /// Open an in-memory cache (for testing).
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("Failed to open in-memory database")?;
        let cache = Self { conn };
        cache.init_schema()?;
        Ok(cache)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS transactions (
                network         TEXT    NOT NULL,
                address         TEXT    NOT NULL,
                digest          TEXT    NOT NULL,
                direction       TEXT,
                sender          TEXT,
                recipient       TEXT,
                amount          INTEGER,
                fee             INTEGER,
                epoch           INTEGER NOT NULL,
                lamport_version INTEGER NOT NULL,
                checkpoint      INTEGER,
                timestamp       INTEGER,
                kind            TEXT,
                function        TEXT,
                status          TEXT,
                input_objects   TEXT,
                changed_objects TEXT,
                bcs             BLOB,
                PRIMARY KEY (network, address, digest)
            );

            CREATE INDEX IF NOT EXISTS idx_tx_order
                ON transactions (network, address, epoch DESC, lamport_version DESC);

            CREATE INDEX IF NOT EXISTS idx_tx_checkpoint
                ON transactions (network, address, checkpoint);

            CREATE INDEX IF NOT EXISTS idx_tx_sender
                ON transactions (network, sender);

            CREATE INDEX IF NOT EXISTS idx_tx_recipient
                ON transactions (network, recipient);

            CREATE INDEX IF NOT EXISTS idx_tx_kind
                ON transactions (network, address, kind);

            CREATE TABLE IF NOT EXISTS sync_state (
                network         TEXT    NOT NULL,
                address         TEXT    NOT NULL,
                last_epoch      INTEGER NOT NULL DEFAULT 0,
                last_synced_at  INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (network, address)
            );",
            )
            .context("Failed to initialize cache schema")?;
        Ok(())
    }

    /// Insert or update transactions. Sent direction takes priority over received
    /// (if you signed a tx, it's "out" even if you also received change).
    pub fn insert(&self, network: &str, address: &str, txs: &[TransactionSummary]) -> Result<()> {
        let tx = self
            .conn
            .unchecked_transaction()
            .context("Failed to begin transaction")?;

        {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT INTO transactions (
                    network, address, digest, direction, sender, recipient,
                    amount, fee, epoch, lamport_version
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ON CONFLICT (network, address, digest) DO UPDATE SET
                    direction = CASE
                        WHEN excluded.direction = 'out' THEN 'out'
                        ELSE transactions.direction
                    END,
                    sender = COALESCE(excluded.sender, transactions.sender),
                    amount = COALESCE(excluded.amount, transactions.amount),
                    fee = COALESCE(excluded.fee, transactions.fee)",
                )
                .context("Failed to prepare insert statement")?;

            for tx_summary in txs {
                let dir = tx_summary.direction.as_ref().map(|d| d.to_string());
                stmt.execute(params![
                    network,
                    address,
                    tx_summary.digest,
                    dir,
                    tx_summary.sender,
                    Option::<String>::None, // recipient — populated later if needed
                    tx_summary.amount.map(|a| a as i64),
                    tx_summary.fee.map(|f| f as i64),
                    tx_summary.epoch as i64,
                    tx_summary.lamport_version as i64,
                ])
                .context("Failed to insert transaction")?;
            }
        }

        tx.commit().context("Failed to commit transaction batch")?;
        Ok(())
    }

    /// Query cached transactions with filter + pagination.
    pub fn query(
        &self,
        network: &str,
        address: &str,
        filter: &TransactionFilter,
        limit: u32,
        offset: u32,
    ) -> Result<TransactionPage> {
        let dir_clause = match filter {
            TransactionFilter::All => "",
            TransactionFilter::In => "AND direction = 'in'",
            TransactionFilter::Out => "AND direction = 'out'",
        };

        let count_sql = format!(
            "SELECT COUNT(*) FROM transactions WHERE network = ?1 AND address = ?2 {dir_clause}"
        );
        let total: u32 = self
            .conn
            .query_row(&count_sql, params![network, address], |row| row.get(0))
            .context("Failed to count transactions")?;

        let query_sql = format!(
            "SELECT digest, direction, sender, amount, fee, epoch, lamport_version, timestamp
             FROM transactions
             WHERE network = ?1 AND address = ?2 {dir_clause}
             ORDER BY epoch DESC, lamport_version DESC
             LIMIT ?3 OFFSET ?4"
        );

        let mut stmt = self
            .conn
            .prepare(&query_sql)
            .context("Failed to prepare query")?;

        let rows = stmt
            .query_map(params![network, address, limit, offset], |row| {
                let dir_str: Option<String> = row.get(1)?;
                let direction = dir_str.as_deref().map(|s| match s {
                    "out" => TransactionDirection::Out,
                    _ => TransactionDirection::In,
                });
                let amount: Option<i64> = row.get(3)?;
                let fee: Option<i64> = row.get(4)?;
                let epoch: i64 = row.get(5)?;
                let lamport: i64 = row.get(6)?;
                let timestamp: Option<i64> = row.get(7)?;

                Ok(TransactionSummary {
                    digest: row.get(0)?,
                    direction,
                    timestamp: timestamp.map(|t| t.to_string()),
                    sender: row.get(2)?,
                    amount: amount.map(|a| a as u64),
                    fee: fee.map(|f| f as u64),
                    epoch: epoch as u64,
                    lamport_version: lamport as u64,
                })
            })
            .context("Failed to query transactions")?;

        let transactions: Vec<TransactionSummary> = rows
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to read transaction rows")?;

        Ok(TransactionPage {
            transactions,
            total,
            offset,
            limit,
        })
    }

    /// Get per-epoch balance deltas for charting.
    /// Returns `(epoch, net_change_in_nanos)` sorted by epoch ASC.
    ///
    /// Assumes all balance-affecting transactions have a direction set and
    /// the cache contains a complete sync — partial history will skew the
    /// backward-walk reconstruction in the GUI chart.
    pub fn query_epoch_deltas(&self, network: &str, address: &str) -> Result<Vec<(u64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT epoch,
                    SUM(CASE WHEN direction = 'in' THEN COALESCE(amount, 0) ELSE 0 END)
                  - SUM(CASE WHEN direction = 'out' THEN COALESCE(amount, 0) + COALESCE(fee, 0) ELSE 0 END)
             FROM transactions
             WHERE network = ?1 AND address = ?2
             GROUP BY epoch
             ORDER BY epoch ASC"
        ).context("Failed to prepare epoch deltas query")?;

        let rows = stmt
            .query_map(params![network, address], |row| {
                let epoch: i64 = row.get(0)?;
                let delta: i64 = row.get(1)?;
                Ok((epoch as u64, delta))
            })
            .context("Failed to query epoch deltas")?;

        rows.collect::<Result<Vec<_>, _>>()
            .context("Failed to read epoch delta rows")
    }

    /// Get the set of known transaction digests for an (network, address) pair.
    pub fn known_digests(&self, network: &str, address: &str) -> Result<HashSet<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT digest FROM transactions WHERE network = ?1 AND address = ?2")
            .context("Failed to prepare known_digests query")?;

        let rows = stmt
            .query_map(params![network, address], |row| row.get::<_, String>(0))
            .context("Failed to query known digests")?;

        let mut digests = HashSet::new();
        for row in rows {
            digests.insert(row.context("Failed to read digest")?);
        }
        Ok(digests)
    }

    /// Get the last synced epoch for an (network, address) pair.
    pub fn get_sync_epoch(&self, network: &str, address: &str) -> Result<u64> {
        let result = self.conn.query_row(
            "SELECT last_epoch FROM sync_state WHERE network = ?1 AND address = ?2",
            params![network, address],
            |row| row.get::<_, i64>(0),
        );
        match result {
            Ok(epoch) => Ok(epoch as u64),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
            Err(e) => Err(e).context("Failed to query sync state"),
        }
    }

    /// Update the sync state after a successful sync.
    pub fn set_sync_epoch(&self, network: &str, address: &str, epoch: u64) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        self.conn
            .execute(
                "INSERT INTO sync_state (network, address, last_epoch, last_synced_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (network, address) DO UPDATE SET
                 last_epoch = excluded.last_epoch,
                 last_synced_at = excluded.last_synced_at",
                params![network, address, epoch as i64, now],
            )
            .context("Failed to update sync state")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::TransactionDirection;

    fn sample_txs() -> Vec<TransactionSummary> {
        vec![
            TransactionSummary {
                digest: "0xaaa".to_string(),
                direction: Some(TransactionDirection::Out),
                timestamp: None,
                sender: Some("0xsender1".to_string()),
                amount: Some(1_000_000_000),
                fee: Some(500_000),
                epoch: 10,
                lamport_version: 100,
            },
            TransactionSummary {
                digest: "0xbbb".to_string(),
                direction: Some(TransactionDirection::In),
                timestamp: None,
                sender: Some("0xsender2".to_string()),
                amount: Some(2_000_000_000),
                fee: None,
                epoch: 10,
                lamport_version: 101,
            },
        ]
    }

    #[test]
    fn insert_and_query() {
        let cache = TransactionCache::open_in_memory().unwrap();
        cache.insert("testnet", "0xme", &sample_txs()).unwrap();

        let page = cache
            .query("testnet", "0xme", &TransactionFilter::All, 25, 0)
            .unwrap();
        assert_eq!(page.transactions.len(), 2);
        assert_eq!(page.total, 2);
        // Sorted by epoch DESC, lamport DESC
        assert_eq!(page.transactions[0].digest, "0xbbb");
        assert_eq!(page.transactions[1].digest, "0xaaa");
    }

    #[test]
    fn filter_in_out() {
        let cache = TransactionCache::open_in_memory().unwrap();
        cache.insert("testnet", "0xme", &sample_txs()).unwrap();

        let page_in = cache
            .query("testnet", "0xme", &TransactionFilter::In, 25, 0)
            .unwrap();
        assert_eq!(page_in.transactions.len(), 1);
        assert_eq!(page_in.transactions[0].digest, "0xbbb");

        let page_out = cache
            .query("testnet", "0xme", &TransactionFilter::Out, 25, 0)
            .unwrap();
        assert_eq!(page_out.transactions.len(), 1);
        assert_eq!(page_out.transactions[0].digest, "0xaaa");
    }

    #[test]
    fn pagination() {
        let cache = TransactionCache::open_in_memory().unwrap();
        cache.insert("testnet", "0xme", &sample_txs()).unwrap();

        let page1 = cache
            .query("testnet", "0xme", &TransactionFilter::All, 1, 0)
            .unwrap();
        assert_eq!(page1.transactions.len(), 1);
        assert_eq!(page1.total, 2);
        assert!(page1.has_next());
        assert!(!page1.has_prev());

        let page2 = cache
            .query("testnet", "0xme", &TransactionFilter::All, 1, 1)
            .unwrap();
        assert_eq!(page2.transactions.len(), 1);
        assert!(!page2.has_next());
        assert!(page2.has_prev());
    }

    #[test]
    fn upsert_direction_priority() {
        let cache = TransactionCache::open_in_memory().unwrap();
        // Insert as "in" first
        let recv = vec![TransactionSummary {
            digest: "0xdup".to_string(),
            direction: Some(TransactionDirection::In),
            timestamp: None,
            sender: Some("0xsender".to_string()),
            amount: Some(1_000_000_000),
            fee: None,
            epoch: 5,
            lamport_version: 50,
        }];
        cache.insert("testnet", "0xme", &recv).unwrap();

        // Insert same digest as "out" — should win
        let sent = vec![TransactionSummary {
            digest: "0xdup".to_string(),
            direction: Some(TransactionDirection::Out),
            timestamp: None,
            sender: Some("0xsender".to_string()),
            amount: Some(1_000_000_000),
            fee: Some(100_000),
            epoch: 5,
            lamport_version: 50,
        }];
        cache.insert("testnet", "0xme", &sent).unwrap();

        let page = cache
            .query("testnet", "0xme", &TransactionFilter::All, 25, 0)
            .unwrap();
        assert_eq!(page.transactions.len(), 1);
        assert_eq!(
            page.transactions[0].direction,
            Some(TransactionDirection::Out)
        );
    }

    #[test]
    fn known_digests() {
        let cache = TransactionCache::open_in_memory().unwrap();
        cache.insert("testnet", "0xme", &sample_txs()).unwrap();

        let digests = cache.known_digests("testnet", "0xme").unwrap();
        assert!(digests.contains("0xaaa"));
        assert!(digests.contains("0xbbb"));
        assert!(!digests.contains("0xccc"));
    }

    #[test]
    fn sync_state() {
        let cache = TransactionCache::open_in_memory().unwrap();
        assert_eq!(cache.get_sync_epoch("testnet", "0xme").unwrap(), 0);
        cache.set_sync_epoch("testnet", "0xme", 42).unwrap();
        assert_eq!(cache.get_sync_epoch("testnet", "0xme").unwrap(), 42);
    }

    #[test]
    fn epoch_deltas() {
        let cache = TransactionCache::open_in_memory().unwrap();
        cache.insert("testnet", "0xme", &sample_txs()).unwrap();

        let deltas = cache.query_epoch_deltas("testnet", "0xme").unwrap();
        // sample_txs: epoch 10 has out(1_000_000_000, fee 500_000) + in(2_000_000_000)
        // net = 2_000_000_000 - (1_000_000_000 + 500_000) = 999_500_000
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0], (10, 999_500_000));

        // Empty for unknown address
        let empty = cache.query_epoch_deltas("testnet", "0xother").unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn isolation_by_network_and_address() {
        let cache = TransactionCache::open_in_memory().unwrap();
        cache.insert("testnet", "0xme", &sample_txs()).unwrap();

        let page = cache
            .query("mainnet", "0xme", &TransactionFilter::All, 25, 0)
            .unwrap();
        assert_eq!(page.total, 0);

        let page = cache
            .query("testnet", "0xother", &TransactionFilter::All, 25, 0)
            .unwrap();
        assert_eq!(page.total, 0);
    }
}
