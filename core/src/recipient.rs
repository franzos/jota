use std::fmt;

use anyhow::{anyhow, bail};
use iota_sdk::types::Address;

/// A recipient that may be a raw address or an IOTA Name (e.g. `franz.iota`).
/// Name resolution is deferred to execute-time where we have async + network access.
#[derive(Debug, Clone, PartialEq)]
pub enum Recipient {
    Address(Address),
    Name(String),
}

impl Recipient {
    /// Parse user input as either a hex address or an `.iota` name.
    /// Tries `Address::from_hex` first, then checks for a valid `.iota` suffix.
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        let input = input.trim();
        if input.is_empty() {
            bail!("Recipient cannot be empty.");
        }

        // Try hex address first
        if input.starts_with("0x") || input.starts_with("0X") {
            return Address::from_hex(input)
                .map(Recipient::Address)
                .map_err(|e| anyhow!("Invalid address '{input}': {e}"));
        }

        // Check for .iota name (case-insensitive suffix)
        let lower = input.to_lowercase();
        if lower.ends_with(".iota") && lower.len() > 5 {
            let name_part = &lower[..lower.len() - 5];
            // Basic validation: non-empty, no spaces, reasonable characters
            if name_part.is_empty() || name_part.contains(' ') {
                bail!("Invalid IOTA name '{input}'.");
            }
            return Ok(Recipient::Name(lower));
        }

        bail!("Invalid recipient '{input}'. Expected a 0x address or a .iota name.");
    }
}

impl fmt::Display for Recipient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Recipient::Address(addr) => write!(f, "{addr}"),
            Recipient::Name(name) => write!(f, "{name}"),
        }
    }
}

/// The result of resolving a `Recipient` — always has an address, optionally
/// retains the original name for display purposes.
#[derive(Debug, Clone)]
pub struct ResolvedRecipient {
    pub address: Address,
    pub name: Option<String>,
}

impl fmt::Display for ResolvedRecipient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(name) => write!(f, "{name} ({addr})", addr = self.address),
            None => write!(f, "{}", self.address),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_address() {
        let input = "0x0000a4984bd495d4346fa208ddff4f5d5e5ad48c21dec631ddebc99809f16900";
        let r = Recipient::parse(input).unwrap();
        assert!(matches!(r, Recipient::Address(_)));
        assert_eq!(r.to_string(), input);
    }

    #[test]
    fn parse_iota_name() {
        let r = Recipient::parse("franz.iota").unwrap();
        assert_eq!(r, Recipient::Name("franz.iota".into()));
        assert_eq!(r.to_string(), "franz.iota");
    }

    #[test]
    fn parse_subdomain() {
        let r = Recipient::parse("wallet.franz.iota").unwrap();
        assert_eq!(r, Recipient::Name("wallet.franz.iota".into()));
    }

    #[test]
    fn parse_case_insensitive() {
        let r = Recipient::parse("Franz.IOTA").unwrap();
        assert_eq!(r, Recipient::Name("franz.iota".into()));
    }

    #[test]
    fn reject_bare_string() {
        assert!(Recipient::parse("franz").is_err());
    }

    #[test]
    fn reject_empty() {
        assert!(Recipient::parse("").is_err());
        assert!(Recipient::parse("  ").is_err());
    }

    #[test]
    fn reject_dot_iota_only() {
        assert!(Recipient::parse(".iota").is_err());
    }

    #[test]
    fn reject_invalid_hex() {
        assert!(Recipient::parse("0xZZZZ").is_err());
    }

    #[test]
    fn resolved_display_with_name() {
        let resolved = ResolvedRecipient {
            address: Address::ZERO,
            name: Some("franz.iota".into()),
        };
        let display = resolved.to_string();
        assert!(display.contains("franz.iota"));
        assert!(display.contains("0x"));
    }

    #[test]
    fn resolved_display_without_name() {
        let resolved = ResolvedRecipient {
            address: Address::ZERO,
            name: None,
        };
        let display = resolved.to_string();
        assert!(display.starts_with("0x"));
        assert!(!display.contains("("));
    }
}
