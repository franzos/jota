use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Per-wallet origin permissions, persisted as JSON at `~/.iota-wallet/permissions.json`.
///
/// Keyed by wallet address (stable across renames).
/// Each address maps to a list of origins that have been granted `connect` access.
#[derive(Debug)]
pub(crate) struct Permissions {
    path: PathBuf,
    /// address → [origin, ...]
    data: HashMap<String, Vec<String>>,
}

impl Permissions {
    pub(crate) fn load(wallet_dir: &Path) -> Self {
        let path = wallet_dir.join("permissions.json");
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self { path, data }
    }

    pub(crate) fn is_allowed(&self, address: &str, origin: &str) -> bool {
        self.data
            .get(address)
            .map(|origins| origins.iter().any(|o| o == origin))
            .unwrap_or(false)
    }

    pub(crate) fn grant(&mut self, address: &str, origin: &str) {
        let origins = self.data.entry(address.to_string()).or_default();
        if !origins.iter().any(|o| o == origin) {
            origins.push(origin.to_string());
            self.save();
        }
    }

    pub(crate) fn revoke(&mut self, address: &str, origin: &str) {
        if let Some(origins) = self.data.get_mut(address) {
            origins.retain(|o| o != origin);
            if origins.is_empty() {
                self.data.remove(address);
            }
            self.save();
        }
    }

    pub(crate) fn connected_sites(&self, address: &str) -> Vec<String> {
        self.data.get(address).cloned().unwrap_or_default()
    }

    fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.data) {
            let _ = std::fs::write(&self.path, json);
        }
    }
}
