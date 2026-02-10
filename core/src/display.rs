/// Output formatting — IOTA denomination conversion and display helpers.
///
/// IOTA uses 9 decimal places (nanos). 1 IOTA = 1_000_000_000 nanos.
use crate::network::{NetworkStatus, StakeStatus, StakedIotaSummary, TransactionDetailsSummary, TransactionDirection, TransactionSummary};

const NANOS_PER_IOTA: u64 = 1_000_000_000;

/// Convert nanos to a human-readable IOTA string.
/// Examples: 1_500_000_000 -> "1.500000000", 0 -> "0.000000000"
#[must_use]
pub fn nanos_to_iota(nanos: u64) -> String {
    let whole = nanos / NANOS_PER_IOTA;
    let frac = nanos % NANOS_PER_IOTA;
    format!("{whole}.{frac:09}")
}

/// Format a balance for display.
#[must_use]
pub fn format_balance(nanos: u64) -> String {
    format!("{} IOTA", nanos_to_iota(nanos))
}

/// Parse a human-readable IOTA amount string into nanos.
/// Accepts: "1.5" -> 1_500_000_000, "1" -> 1_000_000_000, "0.001" -> 1_000_000
#[must_use = "parsing result should be checked"]
pub fn parse_iota_amount(input: &str) -> Result<u64, String> {
    let input = input.trim();

    if input.is_empty() {
        return Err("Amount cannot be empty".to_string());
    }

    if input.starts_with('-') {
        return Err("Amount must be positive".to_string());
    }

    // Check if it's purely numeric (nanos)
    if let Ok(nanos) = input.parse::<u64>() {
        // If the number is very large, assume it's nanos. If small, assume IOTA.
        // To avoid ambiguity, we always treat bare integers as IOTA.
        return Ok(nanos.checked_mul(NANOS_PER_IOTA).ok_or_else(|| {
            "Amount too large".to_string()
        })?);
    }

    // Try parsing as decimal IOTA
    let parts: Vec<&str> = input.split('.').collect();
    if parts.len() > 2 {
        return Err("Invalid amount format. Use IOTA units like '1.5' or '0.001'.".to_string());
    }

    let whole: u64 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid whole part: '{}'", parts[0]))?;

    let frac_nanos = if parts.len() == 2 {
        let frac_str = parts[1];
        if frac_str.is_empty() {
            // Trailing dot: "1." is treated as "1.0"
            0
        } else if frac_str.len() > 9 {
            return Err("Too many decimal places. IOTA supports up to 9.".to_string());
        } else {
            // Pad to 9 digits
            let padded = format!("{:0<9}", frac_str);
            padded
                .parse::<u64>()
                .map_err(|_| format!("Invalid fractional part: '{frac_str}'"))?
        }
    } else {
        0
    };

    let total = whole
        .checked_mul(NANOS_PER_IOTA)
        .and_then(|w| w.checked_add(frac_nanos))
        .ok_or_else(|| "Amount too large".to_string())?;

    Ok(total)
}

/// Format a list of transactions for display.
#[must_use]
pub fn format_transactions(txs: &[TransactionSummary]) -> String {
    if txs.is_empty() {
        return "No transactions found.".to_string();
    }

    let mut lines = Vec::with_capacity(txs.len());
    for tx in txs {
        let dir = match tx.direction {
            Some(d) => format!("{:<3}", d),
            None => "   ".to_string(),
        };
        let addr = tx.sender.as_deref().unwrap_or("-");
        let amount = tx
            .amount
            .map(|a| nanos_to_iota(a))
            .unwrap_or_else(|| "-".to_string());
        let fee = match (tx.direction, tx.fee) {
            (Some(TransactionDirection::Out), Some(f)) => nanos_to_iota(f),
            _ => "-".to_string(),
        };
        lines.push(format!("{dir}  {addr}  {amount}  {fee}  {}", tx.digest));
    }
    lines.join("\n")
}

/// Format a list of staked IOTA objects for display.
#[must_use]
pub fn format_stakes(stakes: &[StakedIotaSummary]) -> String {
    if stakes.is_empty() {
        return "No active stakes.".to_string();
    }

    let mut lines = Vec::with_capacity(stakes.len() + 2);
    let mut total_principal: u64 = 0;
    let mut total_reward: u64 = 0;
    for s in stakes {
        total_principal = total_principal.saturating_add(s.principal);
        let reward_str = match s.estimated_reward {
            Some(r) => {
                total_reward = total_reward.saturating_add(r);
                format!("  reward {}", format_balance(r))
            }
            None => String::new(),
        };
        let status_str = match s.status {
            StakeStatus::Active => "",
            StakeStatus::Pending => "  [pending]",
            StakeStatus::Unstaked => "  [unstaked]",
        };
        lines.push(format!(
            "  {}  {}  epoch {}  pool {}{}{}",
            s.object_id,
            format_balance(s.principal),
            s.stake_activation_epoch,
            s.pool_id,
            reward_str,
            status_str,
        ));
    }
    lines.push(format!(
        "\nTotal staked: {}  rewards: {}",
        format_balance(total_principal),
        format_balance(total_reward),
    ));
    lines.join("\n")
}

/// Format a single transaction's details for display.
#[must_use]
pub fn format_transaction_details(tx: &TransactionDetailsSummary) -> String {
    let mut lines = Vec::new();
    lines.push(format!("  Digest:    {}", tx.digest));
    lines.push(format!("  Status:    {}", tx.status));
    lines.push(format!("  Sender:    {}", tx.sender));
    if let Some(ref recipient) = tx.recipient {
        lines.push(format!("  Recipient: {}", recipient));
    }
    if let Some(amount) = tx.amount {
        lines.push(format!("  Amount:    {}", format_balance(amount)));
    }
    if let Some(fee) = tx.fee {
        lines.push(format!("  Gas fee:   {}", format_balance(fee)));
    }
    lines.join("\n")
}

/// Format network status for display.
#[must_use]
pub fn format_status(status: &NetworkStatus) -> String {
    format!(
        "  Network:   {}\n  Epoch:     {}\n  Gas price: {} nanos/unit\n  Node:      {}",
        status.network,
        status.epoch,
        status.reference_gas_price,
        status.node_url,
    )
}

/// Format balance as JSON.
#[must_use]
pub fn format_balance_json(nanos: u64) -> String {
    serde_json::json!({
        "balance_nanos": nanos,
        "balance_iota": nanos_to_iota(nanos),
    })
    .to_string()
}

/// Format address as JSON.
#[must_use]
pub fn format_address_json(address: &str) -> String {
    serde_json::json!({
        "address": address,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nanos_to_iota_zero() {
        assert_eq!(nanos_to_iota(0), "0.000000000");
    }

    #[test]
    fn nanos_to_iota_one() {
        assert_eq!(nanos_to_iota(1_000_000_000), "1.000000000");
    }

    #[test]
    fn nanos_to_iota_fractional() {
        assert_eq!(nanos_to_iota(1_500_000_000), "1.500000000");
    }

    #[test]
    fn nanos_to_iota_small() {
        assert_eq!(nanos_to_iota(1), "0.000000001");
    }

    #[test]
    fn nanos_to_iota_large() {
        assert_eq!(nanos_to_iota(123_456_789_012), "123.456789012");
    }

    #[test]
    fn format_balance_display() {
        assert_eq!(format_balance(2_000_000_000), "2.000000000 IOTA");
    }

    #[test]
    fn parse_whole_number() {
        assert_eq!(parse_iota_amount("1").unwrap(), 1_000_000_000);
    }

    #[test]
    fn parse_decimal() {
        assert_eq!(parse_iota_amount("1.5").unwrap(), 1_500_000_000);
    }

    #[test]
    fn parse_small_decimal() {
        assert_eq!(parse_iota_amount("0.001").unwrap(), 1_000_000);
    }

    #[test]
    fn parse_full_precision() {
        assert_eq!(parse_iota_amount("1.123456789").unwrap(), 1_123_456_789);
    }

    #[test]
    fn parse_too_many_decimals() {
        assert!(parse_iota_amount("1.1234567890").is_err());
    }

    #[test]
    fn parse_empty_fails() {
        assert!(parse_iota_amount("").is_err());
    }

    #[test]
    fn parse_garbage_fails() {
        assert!(parse_iota_amount("abc").is_err());
    }

    #[test]
    fn parse_zero() {
        assert_eq!(parse_iota_amount("0").unwrap(), 0);
    }

    #[test]
    fn parse_zero_decimal() {
        assert_eq!(parse_iota_amount("0.0").unwrap(), 0);
    }

    #[test]
    fn parse_negative_integer_fails() {
        let result = parse_iota_amount("-1");
        assert!(result.is_err(), "negative amount should be rejected");
    }

    #[test]
    fn parse_negative_decimal_fails() {
        let result = parse_iota_amount("-0.5");
        assert!(result.is_err(), "negative decimal amount should be rejected");
    }

    #[test]
    fn parse_trailing_dot() {
        assert_eq!(parse_iota_amount("1.").unwrap(), 1_000_000_000);
    }

    #[test]
    fn format_balance_json_output() {
        let json = format_balance_json(1_500_000_000);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["balance_nanos"], 1_500_000_000u64);
        assert_eq!(v["balance_iota"], "1.500000000");
    }

    #[test]
    fn format_empty_transactions() {
        assert_eq!(format_transactions(&[]), "No transactions found.");
    }

    #[test]
    fn format_transactions_compact() {
        let txs = vec![
            TransactionSummary {
                digest: "0xaabbccddee112233".to_string(),
                direction: Some(TransactionDirection::In),
                timestamp: None,
                sender: Some("0x1234567890abcdef".to_string()),
                amount: Some(1_500_000_000),
                fee: Some(1_234_500),
                epoch: 1,
                lamport_version: 100,
            },
            TransactionSummary {
                digest: "0xffeeddccbbaa9988".to_string(),
                direction: Some(TransactionDirection::Out),
                timestamp: None,
                sender: Some("0x9876543210fedcba".to_string()),
                amount: Some(2_000_000_000),
                fee: Some(2_345_600),
                epoch: 1,
                lamport_version: 101,
            },
        ];
        let output = format_transactions(&txs);
        // Direction padded to 3 chars
        assert!(output.contains("in "));
        assert!(output.contains("out"));
        // Full addresses and digests shown
        assert!(output.contains("0x1234567890abcdef"));
        assert!(output.contains("0x9876543210fedcba"));
        assert!(output.contains("0xaabbccddee112233"));
        assert!(output.contains("0xffeeddccbbaa9988"));
        // Amounts shown
        assert!(output.contains("1.500000000"));
        assert!(output.contains("2.000000000"));
        // Fee only shown for outgoing
        assert!(output.contains("0.002345600")); // out fee
        // In fee is "-"
        let in_line = output.lines().find(|l| l.starts_with("in ")).unwrap();
        let out_line = output.lines().find(|l| l.starts_with("out")).unwrap();
        assert!(in_line.contains("  -  "));
        assert!(!out_line.contains("  -  "));
    }

    #[test]
    fn format_transactions_no_amount() {
        let txs = vec![
            TransactionSummary {
                digest: "0xshortdigest".to_string(),
                direction: None,
                timestamp: None,
                sender: None,
                amount: None,
                fee: None,
                epoch: 0,
                lamport_version: 0,
            },
        ];
        let output = format_transactions(&txs);
        assert!(output.contains("-"));
    }
}
