use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub name: String,
    pub address: String,
    pub iota_name: Option<String>,
    pub added_at: i64,
}

/// Global address-book stored as unencrypted JSON.
///
/// Path: `data_dir()/contacts.json`
pub struct ContactStore {
    path: PathBuf,
    contacts: Vec<Contact>,
}

impl ContactStore {
    /// Open (or create) the contact store at the default data directory.
    pub fn open() -> Result<Self> {
        let path = crate::data_dir()?.join("contacts.json");
        Self::open_at(path)
    }

    /// Open (or create) the contact store at a specific path.
    pub fn open_at(path: PathBuf) -> Result<Self> {
        let contacts = if path.exists() {
            let data = std::fs::read_to_string(&path).context("Failed to read contacts.json")?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(Self { path, contacts })
    }

    /// Add a new contact. Name must be unique (case-insensitive).
    /// Address is normalized to lowercase `0x`-prefixed hex.
    pub fn add(&mut self, name: &str, address: &str, iota_name: Option<&str>) -> Result<()> {
        let name = name.trim().to_string();
        if name.is_empty() {
            bail!("Contact name cannot be empty.");
        }
        let canonical = canonical_address(address)?;

        if self
            .contacts
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&name))
        {
            bail!("A contact named '{name}' already exists.");
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.contacts.push(Contact {
            name,
            address: canonical,
            iota_name: iota_name.map(|s| s.to_string()),
            added_at: now,
        });
        self.save()
    }

    /// Remove a contact by name (case-insensitive).
    pub fn remove(&mut self, name: &str) -> Result<()> {
        let before = self.contacts.len();
        self.contacts.retain(|c| !c.name.eq_ignore_ascii_case(name));
        if self.contacts.len() == before {
            bail!("No contact named '{name}' found.");
        }
        self.save()
    }

    /// Get a contact by exact name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<&Contact> {
        self.contacts
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// List all contacts.
    pub fn list(&self) -> &[Contact] {
        &self.contacts
    }

    /// Find contacts whose address matches (case-insensitive prefix or full match).
    pub fn find_by_address(&self, address: &str) -> Option<&Contact> {
        let lower = address.to_lowercase();
        self.contacts.iter().find(|c| c.address == lower)
    }

    /// Find contacts whose name contains the query (case-insensitive).
    pub fn find_by_name(&self, query: &str) -> Vec<&Contact> {
        let lower = query.to_lowercase();
        self.contacts
            .iter()
            .filter(|c| c.name.to_lowercase().contains(&lower))
            .collect()
    }

    /// Export contacts as pretty-printed JSON.
    pub fn export(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.contacts).context("Failed to serialize contacts")
    }

    /// Import contacts from a JSON string.
    /// Merges by address — existing entries are kept, new ones added.
    pub fn import(&mut self, json: &str) -> Result<usize> {
        let incoming: Vec<Contact> = serde_json::from_str(json).context("Invalid contacts JSON")?;
        let mut added = 0;
        for c in incoming {
            let canonical = match canonical_address(&c.address) {
                Ok(a) => a,
                Err(_) => continue,
            };
            if self
                .contacts
                .iter()
                .any(|existing| existing.address == canonical)
            {
                continue;
            }
            // Also skip if name already taken
            if self
                .contacts
                .iter()
                .any(|existing| existing.name.eq_ignore_ascii_case(&c.name))
            {
                continue;
            }
            self.contacts.push(Contact {
                name: c.name,
                address: canonical,
                iota_name: c.iota_name,
                added_at: c.added_at,
            });
            added += 1;
        }
        if added > 0 {
            self.save()?;
        }
        Ok(added)
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let json =
            serde_json::to_string_pretty(&self.contacts).context("Failed to serialize contacts")?;
        std::fs::write(&self.path, json).context("Failed to write contacts.json")?;
        Ok(())
    }
}

/// Normalize an address to lowercase `0x`-prefixed hex.
fn canonical_address(addr: &str) -> Result<String> {
    let addr = addr.trim().to_lowercase();
    if !addr.starts_with("0x") || addr.len() < 4 {
        bail!("Invalid address: must be 0x-prefixed hex.");
    }
    // Validate hex characters after 0x
    if !addr[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("Invalid address: contains non-hex characters.");
    }
    Ok(addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> ContactStore {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("contacts.json");
        ContactStore::open_at(path).unwrap()
    }

    #[test]
    fn add_and_list() {
        let mut store = temp_store();
        store
            .add("Alice", "0xabcd1234", Some("alice.iota"))
            .unwrap();
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].name, "Alice");
        assert_eq!(store.list()[0].address, "0xabcd1234");
        assert_eq!(store.list()[0].iota_name.as_deref(), Some("alice.iota"));
    }

    #[test]
    fn name_uniqueness_case_insensitive() {
        let mut store = temp_store();
        store.add("Alice", "0xabcd1234", None).unwrap();
        let err = store.add("alice", "0xdead5678", None).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn remove_contact() {
        let mut store = temp_store();
        store.add("Alice", "0xabcd1234", None).unwrap();
        store.remove("alice").unwrap();
        assert!(store.list().is_empty());
    }

    #[test]
    fn remove_nonexistent() {
        let mut store = temp_store();
        let err = store.remove("nobody").unwrap_err();
        assert!(err.to_string().contains("No contact"));
    }

    #[test]
    fn get_by_name() {
        let mut store = temp_store();
        store.add("Alice", "0xabcd1234", None).unwrap();
        assert!(store.get("alice").is_some());
        assert!(store.get("bob").is_none());
    }

    #[test]
    fn find_by_address() {
        let mut store = temp_store();
        store.add("Alice", "0xABCD1234", None).unwrap();
        assert!(store.find_by_address("0xabcd1234").is_some());
        assert!(store.find_by_address("0xdead").is_none());
    }

    #[test]
    fn find_by_name_substring() {
        let mut store = temp_store();
        store.add("Alice Wonderland", "0xabcd1234", None).unwrap();
        store.add("Bob Builder", "0xdead5678", None).unwrap();
        let results = store.find_by_name("alice");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Alice Wonderland");
    }

    #[test]
    fn canonical_address_lowercase() {
        let mut store = temp_store();
        store.add("Alice", "0xABCD1234", None).unwrap();
        assert_eq!(store.list()[0].address, "0xabcd1234");
    }

    #[test]
    fn import_merge_dedup() {
        let mut store = temp_store();
        store.add("Alice", "0xabcd1234", None).unwrap();

        let import_json = r#"[
            {"name": "Alice", "address": "0xabcd1234", "iota_name": null, "added_at": 0},
            {"name": "Bob", "address": "0xdead5678", "iota_name": null, "added_at": 0}
        ]"#;
        let added = store.import(import_json).unwrap();
        assert_eq!(added, 1);
        assert_eq!(store.list().len(), 2);
    }

    #[test]
    fn export_valid_json() {
        let mut store = temp_store();
        store.add("Alice", "0xabcd1234", None).unwrap();
        let json = store.export().unwrap();
        let parsed: Vec<Contact> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn persistence_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("contacts.json");

        let mut store = ContactStore::open_at(path.clone()).unwrap();
        store.add("Alice", "0xabcd1234", None).unwrap();
        drop(store);

        let store2 = ContactStore::open_at(path).unwrap();
        assert_eq!(store2.list().len(), 1);
        assert_eq!(store2.list()[0].name, "Alice");
    }

    #[test]
    fn invalid_address_rejected() {
        let mut store = temp_store();
        assert!(store.add("Bad", "not-an-address", None).is_err());
        assert!(store.add("Bad", "0x", None).is_err());
        assert!(store.add("Bad", "0xGGGG", None).is_err());
    }

    #[test]
    fn empty_name_rejected() {
        let mut store = temp_store();
        assert!(store.add("", "0xabcd1234", None).is_err());
        assert!(store.add("  ", "0xabcd1234", None).is_err());
    }
}
