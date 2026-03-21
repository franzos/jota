use std::path::PathBuf;

use anyhow::{bail, Context, Result};

use super::{MultisigConfig, TransactionProposal};

/// Validate that a string is safe for use as a filename (hex chars only).
fn validate_hex_filename(s: &str) -> Result<()> {
    if s.is_empty() {
        bail!("Empty identifier.");
    }
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("Invalid characters in identifier '{s}'. Expected hex only.");
    }
    Ok(())
}

pub struct MultisigStore {
    dir: PathBuf,
}

impl MultisigStore {
    /// Open the default multisig store at `data_dir()/multisig/`.
    pub fn open() -> Result<Self> {
        let dir = crate::data_dir()?.join("multisig");
        Self::open_at(dir)
    }

    /// Open (or create) the multisig store at a specific directory.
    pub fn open_at(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create multisig dir: {}", dir.display()))?;
        std::fs::create_dir_all(dir.join("proposals"))
            .with_context(|| format!("Failed to create proposals dir: {}", dir.display()))?;
        Ok(Self { dir })
    }

    // ── Config CRUD ──────────────────────────────────────────────────

    pub fn save_config(&self, config: &MultisigConfig) -> Result<()> {
        crate::validate_wallet_name(&config.name)?;
        let path = self.config_path(&config.name);
        let json =
            serde_json::to_string_pretty(config).context("Failed to serialize multisig config")?;
        Self::atomic_write(&path, json.as_bytes())
    }

    pub fn load_config(&self, name: &str) -> Result<MultisigConfig> {
        let path = self.config_path(name);
        let data = std::fs::read_to_string(&path)
            .with_context(|| format!("No multisig config named '{name}'"))?;
        serde_json::from_str(&data).context("Failed to parse multisig config")
    }

    pub fn list_configs(&self) -> Result<Vec<MultisigConfig>> {
        let mut configs = Vec::new();
        let entries = std::fs::read_dir(&self.dir).context("Failed to read multisig directory")?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) && path.file_stem().is_some()
            {
                // Skip the proposals subdirectory entries
                if path.is_file() {
                    if let Ok(data) = std::fs::read_to_string(&path) {
                        if let Ok(config) = serde_json::from_str::<MultisigConfig>(&data) {
                            configs.push(config);
                        }
                    }
                }
            }
        }
        configs.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(configs)
    }

    pub fn remove_config(&self, name: &str) -> Result<()> {
        let path = self.config_path(name);
        if !path.exists() {
            bail!("No multisig config named '{name}'.");
        }
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove multisig config '{name}'"))
    }

    // ── Proposal CRUD ────────────────────────────────────────────────

    pub fn save_proposal(&self, proposal: &TransactionProposal) -> Result<()> {
        validate_hex_filename(&proposal.tx_digest)?;
        let path = self.proposal_path(&proposal.tx_digest);
        let json = serde_json::to_string_pretty(proposal)
            .context("Failed to serialize transaction proposal")?;
        Self::atomic_write(&path, json.as_bytes())
    }

    pub fn load_proposal(&self, tx_digest: &str) -> Result<TransactionProposal> {
        validate_hex_filename(tx_digest)?;
        let path = self.proposal_path(tx_digest);
        let data = std::fs::read_to_string(&path)
            .with_context(|| format!("No proposal with digest '{tx_digest}'"))?;
        serde_json::from_str(&data).context("Failed to parse transaction proposal")
    }

    pub fn list_proposals(&self) -> Result<Vec<TransactionProposal>> {
        let mut proposals = Vec::new();
        let dir = self.dir.join("proposals");
        let entries = std::fs::read_dir(&dir).context("Failed to read proposals directory")?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) && path.is_file() {
                if let Ok(data) = std::fs::read_to_string(&path) {
                    if let Ok(proposal) = serde_json::from_str::<TransactionProposal>(&data) {
                        proposals.push(proposal);
                    }
                }
            }
        }
        proposals.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(proposals)
    }

    pub fn list_proposals_for(&self, address: &str) -> Result<Vec<TransactionProposal>> {
        let all = self.list_proposals()?;
        let lower = address.to_lowercase();
        Ok(all
            .into_iter()
            .filter(|p| p.multisig_address.to_lowercase() == lower)
            .collect())
    }

    /// Find a proposal by a prefix of its `tx_digest`.
    /// Returns an error if none or more than one match.
    pub fn find_proposal_by_prefix(&self, prefix: &str) -> Result<TransactionProposal> {
        if !prefix.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("Invalid characters in prefix '{prefix}'.");
        }
        let lower = prefix.to_lowercase();
        let dir = self.dir.join("proposals");
        let entries = std::fs::read_dir(&dir).context("Failed to read proposals directory")?;

        let mut matches = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(stem) = path.file_stem() {
                let name = stem.to_string_lossy().to_lowercase();
                if name.starts_with(&lower) && path.is_file() {
                    matches.push(path);
                }
            }
        }

        match matches.len() {
            0 => bail!("No proposal found matching prefix '{prefix}'."),
            1 => {
                let data =
                    std::fs::read_to_string(&matches[0]).context("Failed to read proposal file")?;
                serde_json::from_str(&data).context("Failed to parse proposal")
            }
            n => {
                bail!("Ambiguous prefix '{prefix}' matches {n} proposals. Provide more characters.")
            }
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────

    fn config_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.json"))
    }

    fn proposal_path(&self, tx_digest: &str) -> PathBuf {
        self.dir.join("proposals").join(format!("{tx_digest}.json"))
    }

    /// Write data atomically: write to `.tmp`, fsync on unix, then rename.
    fn atomic_write(path: &std::path::Path, data: &[u8]) -> Result<()> {
        let tmp_path = path.with_extension("json.tmp");

        #[cfg(unix)]
        {
            use std::io::Write;
            let mut file = std::fs::File::create(&tmp_path)
                .with_context(|| format!("Failed to create temp file: {}", tmp_path.display()))?;
            file.write_all(data)?;
            file.sync_all()?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&tmp_path, data)
                .with_context(|| format!("Failed to write temp file: {}", tmp_path.display()))?;
        }

        std::fs::rename(&tmp_path, path)
            .with_context(|| format!("Failed to rename temp file to {}", path.display()))?;
        Ok(())
    }
}
