/// Output formatting — IOTA denomination conversion and display helpers.
///
/// IOTA uses 9 decimal places (nanos). 1 IOTA = 1_000_000_000 nanos.
use std::sync::OnceLock;

use anyhow::{anyhow, bail, Result};
use num_format::{SystemLocale, ToFormattedString};

use crate::network::{
    CoinMeta, NetworkStatus, NftSummary, StakeStatus, StakedIotaSummary, TokenBalance,
    TransactionDetailsSummary, TransactionDirection, TransactionSummary,
};

/// Cached system locale for number formatting.
fn system_locale() -> &'static SystemLocale {
    static LOCALE: OnceLock<SystemLocale> = OnceLock::new();
    LOCALE.get_or_init(|| SystemLocale::default().unwrap_or_else(|_| {
        // Fallback: build from en locale
        SystemLocale::from_name("en_US").unwrap_or_else(|_| {
            SystemLocale::default().unwrap_or_else(|_| unreachable!())
        })
    }))
}

/// Format a raw token amount using the given number of decimal places,
/// with locale-aware thousands grouping and decimal separator.
///
/// Trailing zeros are trimmed but at least one fractional digit is kept.
/// E.g. format_amount(1_500_000_000, 9) -> "1.5" (en_US) or "1,5" (de_DE)
#[must_use]
pub fn format_amount(value: u128, decimals: u8) -> String {
    let locale = system_locale();
    format_amount_with_locale(value, decimals, locale.separator(), locale.decimal())
}

/// Format with explicit separators — used by tests for deterministic output.
fn format_amount_with_locale(value: u128, decimals: u8, thousands_sep: &str, decimal_sep: &str) -> String {
    if decimals == 0 {
        return format_integer_grouped(value, thousands_sep);
    }
    let divisor = 10u128.pow(decimals as u32);
    let whole = value / divisor;
    let frac = value % divisor;

    let whole_str = format_integer_grouped(whole, thousands_sep);
    let frac_str = format!("{frac:0>width$}", width = decimals as usize);

    // Trim trailing zeros, keep at least 1 fractional digit
    let trimmed = frac_str.trim_end_matches('0');
    let frac_display = if trimmed.is_empty() { "0" } else { trimmed };

    format!("{whole_str}{decimal_sep}{frac_display}")
}

/// Group an integer with thousands separators.
fn format_integer_grouped(value: u128, separator: &str) -> String {
    let digits = value.to_string();
    if digits.len() <= 3 {
        return digits;
    }
    let mut result = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            result.push_str(separator);
        }
        result.push(ch);
    }
    result
}

/// Format a float value (already in token units) for chart Y-axis labels.
/// Uses locale thousands separator with 0-2 decimal places.
#[must_use]
pub fn format_chart_label(val: f64) -> String {
    let locale = system_locale();
    let decimal_sep = locale.decimal();

    let whole = val.trunc() as i64;
    let whole_str = if let Ok(v) = u64::try_from(whole) {
        v.to_formatted_string(locale)
    } else {
        whole.to_string()
    };

    let frac = (val.fract().abs() * 100.0).round() as u64;
    if frac == 0 {
        whole_str
    } else if frac % 10 == 0 {
        format!("{whole_str}{decimal_sep}{}", frac / 10)
    } else {
        format!("{whole_str}{decimal_sep}{frac:02}")
    }
}

/// Format a raw token amount with its symbol appended.
#[must_use]
pub fn format_balance_with_symbol(value: u128, decimals: u8, symbol: &str) -> String {
    format!("{} {symbol}", format_amount(value, decimals))
}

/// Parse a human-readable token amount into its smallest unit for the given decimals.
/// E.g. parse_token_amount("1.5", 6) -> Ok(1_500_000)
pub fn parse_token_amount(input: &str, decimals: u8) -> Result<u128> {
    let input = input.trim();
    if input.is_empty() {
        bail!("Amount cannot be empty");
    }
    if input.starts_with('-') {
        bail!("Amount must be positive");
    }

    let dec = decimals as usize;
    let multiplier = 10u128.pow(decimals as u32);

    // Bare integer — treat as human-readable units
    if let Ok(whole) = input.parse::<u128>() {
        return whole
            .checked_mul(multiplier)
            .ok_or_else(|| anyhow!("Amount too large"));
    }

    let parts: Vec<&str> = input.split('.').collect();
    if parts.len() > 2 {
        bail!("Invalid amount format.");
    }

    let whole: u128 = parts[0]
        .parse()
        .map_err(|_| anyhow!("Invalid whole part: '{}'", parts[0]))?;

    let frac_units = if parts.len() == 2 {
        let frac_str = parts[1];
        if frac_str.is_empty() {
            0u128
        } else if frac_str.len() > dec && dec > 0 {
            bail!("Too many decimal places. This token supports up to {dec}.");
        } else if dec == 0 {
            bail!("This token has no decimal places.");
        } else {
            let padded = format!("{:0<width$}", frac_str, width = dec);
            padded
                .parse::<u128>()
                .map_err(|_| anyhow!("Invalid fractional part: '{frac_str}'"))?
        }
    } else {
        0
    };

    whole
        .checked_mul(multiplier)
        .and_then(|w| w.checked_add(frac_units))
        .ok_or_else(|| anyhow!("Amount too large"))
}

/// Convert nanos to a human-readable IOTA string.
/// Examples: 1_500_000_000 -> "1.500000000", 0 -> "0.000000000"
#[must_use]
pub fn nanos_to_iota(nanos: impl Into<u128>) -> String {
    format_amount(nanos.into(), 9)
}

/// Format a balance for display.
#[must_use]
pub fn format_balance(nanos: impl Into<u128>) -> String {
    format!("{} IOTA", nanos_to_iota(nanos))
}

/// Parse a human-readable IOTA amount string into nanos.
/// Accepts: "1.5" -> 1_500_000_000, "1" -> 1_000_000_000, "0.001" -> 1_000_000
pub fn parse_iota_amount(input: &str) -> Result<u64> {
    let raw = parse_token_amount(input, 9)?;
    u64::try_from(raw).map_err(|_| anyhow!("Amount too large"))
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
            .map(nanos_to_iota)
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

/// Format token balances for display. When `CoinMeta` is available for a
/// token, uses the symbol and correct decimal formatting.
#[must_use]
pub fn format_token_balances(balances: &[TokenBalance]) -> String {
    format_token_balances_with_meta(balances, &[])
}

/// Format token balances with optional metadata for proper symbol/decimal display.
#[must_use]
pub fn format_token_balances_with_meta(balances: &[TokenBalance], meta: &[CoinMeta]) -> String {
    if balances.is_empty() {
        return "No token balances found.".to_string();
    }

    let mut lines = Vec::with_capacity(balances.len());
    for b in balances {
        let objects = if b.coin_object_count == 1 {
            "1 object".to_string()
        } else {
            format!("{} objects", b.coin_object_count)
        };

        // Try to find metadata for this coin type
        let coin_meta = meta.iter().find(|m| m.coin_type == b.coin_type);

        let (label, amount) = if b.coin_type == "0x2::iota::IOTA" {
            (
                "IOTA".to_string(),
                format_balance_with_symbol(b.total_balance, 9, "IOTA"),
            )
        } else if let Some(m) = coin_meta {
            (
                m.symbol.clone(),
                format_balance_with_symbol(b.total_balance, m.decimals, &m.symbol),
            )
        } else {
            // Fallback: show raw coin type and raw amount
            let short = b
                .coin_type
                .split("::")
                .last()
                .unwrap_or(&b.coin_type)
                .to_string();
            (short, b.total_balance.to_string())
        };

        lines.push(format!("  {:<8} {}  ({})", label, amount, objects));
    }
    lines.join("\n")
}

/// Format a list of NFTs for display.
#[must_use]
pub fn format_nfts(nfts: &[NftSummary]) -> String {
    if nfts.is_empty() {
        return "No NFTs found.".to_string();
    }

    let mut lines = Vec::with_capacity(nfts.len());
    for nft in nfts {
        let name = nft.name.as_deref().unwrap_or("(unnamed)");
        let desc = nft.description.as_deref().unwrap_or("");
        let desc_display = if desc.is_empty() {
            String::new()
        } else if desc.chars().count() > 40 {
            let truncated: String = desc.chars().take(37).collect();
            format!("  \"{truncated}...\"")
        } else {
            format!("  \"{desc}\"")
        };
        lines.push(format!("  {}  {}{}", nft.object_id, name, desc_display));
    }
    lines.join("\n")
}

/// Format network status for display.
#[must_use]
pub fn format_status(status: &NetworkStatus) -> String {
    format!(
        "  Network:   {}\n  Epoch:     {}\n  Gas price: {} nanos/unit\n  Node:      {}",
        status.network, status.epoch, status.reference_gas_price, status.node_url,
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

    /// Helper: format with en_US-style separators for deterministic tests.
    fn fmt_en(value: u128, decimals: u8) -> String {
        format_amount_with_locale(value, decimals, ",", ".")
    }

    // -- format_amount_with_locale (deterministic) tests --

    #[test]
    fn format_amount_zero() {
        assert_eq!(fmt_en(0, 9), "0.0");
    }

    #[test]
    fn format_amount_one_iota() {
        assert_eq!(fmt_en(1_000_000_000, 9), "1.0");
    }

    #[test]
    fn format_amount_fractional() {
        assert_eq!(fmt_en(1_500_000_000, 9), "1.5");
    }

    #[test]
    fn format_amount_small() {
        assert_eq!(fmt_en(1, 9), "0.000000001");
    }

    #[test]
    fn format_amount_large() {
        assert_eq!(fmt_en(123_456_789_012, 9), "123.456789012");
    }

    #[test]
    fn format_amount_thousands_grouping() {
        assert_eq!(fmt_en(100_000_500_000_000, 9), "100,000.5");
    }

    #[test]
    fn format_amount_u128_large() {
        let big: u128 = u64::MAX as u128 + 1_000_000_000;
        let result = fmt_en(big, 9);
        assert!(result.contains('.'), "should have decimal");
        assert!(result.contains(','), "should have thousands grouping");
    }

    #[test]
    fn format_amount_trimming_keeps_one_digit() {
        // 2.000000000 -> "2.0", not "2."
        assert_eq!(fmt_en(2_000_000_000, 9), "2.0");
    }

    #[test]
    fn format_amount_trimming_preserves_significant() {
        // 1.123456789 stays fully intact
        assert_eq!(fmt_en(1_123_456_789, 9), "1.123456789");
    }

    #[test]
    fn format_amount_de_locale() {
        let result = format_amount_with_locale(100_000_500_000_000, 9, ".", ",");
        assert_eq!(result, "100.000,5");
    }

    // -- format_amount (public, locale-dependent) sanity checks --

    #[test]
    fn format_amount_does_not_panic() {
        let result = format_amount(u128::MAX, 9);
        assert!(!result.is_empty());
    }

    #[test]
    fn format_amount_zero_decimals() {
        // Zero decimals should not have a decimal point (locale-independent)
        let result = format_amount(100, 0);
        assert!(result.contains("100"));
    }

    // -- format_amount multi-token tests --

    #[test]
    fn format_amount_usdt_like() {
        assert_eq!(fmt_en(1_500_000, 6), "1.5");
    }

    #[test]
    fn format_amount_zero_with_decimals() {
        assert_eq!(fmt_en(0, 6), "0.0");
    }

    #[test]
    fn format_amount_18_decimals() {
        assert_eq!(fmt_en(1_000_000_000_000_000_000, 18), "1.0");
    }

    #[test]
    fn format_amount_value_smaller_than_one_unit() {
        assert_eq!(fmt_en(123, 8), "0.00000123");
    }

    // -- parse tests (unchanged — parsing is not locale-dependent) --

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
        assert!(
            result.is_err(),
            "negative decimal amount should be rejected"
        );
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
        // The formatted string now trims trailing zeros
        let iota_str = v["balance_iota"].as_str().unwrap();
        assert!(iota_str.contains("1"), "should contain 1");
        assert!(iota_str.contains("5"), "should contain 5");
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
        // Fee only shown for outgoing; in fee is "-"
        let in_line = output.lines().find(|l| l.starts_with("in ")).unwrap();
        let out_line = output.lines().find(|l| l.starts_with("out")).unwrap();
        assert!(in_line.contains("  -  "));
        assert!(!out_line.contains("  -  "));
    }

    #[test]
    fn format_transactions_no_amount() {
        let txs = vec![TransactionSummary {
            digest: "0xshortdigest".to_string(),
            direction: None,
            timestamp: None,
            sender: None,
            amount: None,
            fee: None,
            epoch: 0,
            lamport_version: 0,
        }];
        let output = format_transactions(&txs);
        assert!(output.contains("-"));
    }

    // -- format_balance_with_symbol tests --

    #[test]
    fn format_balance_with_symbol_zero_decimals() {
        assert_eq!(format_balance_with_symbol(0, 0, "NFT"), "0 NFT");
    }

    // -- parse_token_amount multi-token tests --

    #[test]
    fn parse_token_amount_decimal_6() {
        assert_eq!(parse_token_amount("1.5", 6).unwrap(), 1_500_000);
    }

    #[test]
    fn parse_token_amount_whole_with_decimals() {
        assert_eq!(parse_token_amount("1", 6).unwrap(), 1_000_000);
    }

    #[test]
    fn parse_token_amount_smallest_unit() {
        assert_eq!(parse_token_amount("0.000001", 6).unwrap(), 1);
    }

    #[test]
    fn parse_token_amount_zero_decimals_integer() {
        assert_eq!(parse_token_amount("100", 0).unwrap(), 100);
    }

    #[test]
    fn parse_token_amount_zero_decimals_with_dot_fails() {
        assert!(parse_token_amount("1.0", 0).is_err());
    }

    #[test]
    fn parse_token_amount_18_decimals() {
        assert_eq!(
            parse_token_amount("1.5", 18).unwrap(),
            1_500_000_000_000_000_000
        );
    }

    #[test]
    fn parse_token_amount_too_many_frac_digits() {
        assert!(parse_token_amount("0.0000001", 6).is_err());
    }

    #[test]
    fn parse_token_amount_trailing_dot() {
        assert_eq!(parse_token_amount("1.", 6).unwrap(), 1_000_000);
    }

    #[test]
    fn parse_token_amount_empty_fails() {
        assert!(parse_token_amount("", 6).is_err());
    }

    #[test]
    fn parse_token_amount_negative_fails() {
        assert!(parse_token_amount("-5", 6).is_err());
    }

    #[test]
    fn parse_token_amount_max_decimals_38() {
        let result = parse_token_amount("1.5", 38);
        assert!(result.is_ok(), "decimals=38 should be supported");
    }

    // -- format_token_balances_with_meta tests --

    #[test]
    fn format_token_balances_with_meta_empty() {
        let output = format_token_balances_with_meta(&[], &[]);
        assert_eq!(output, "No token balances found.");
    }

    #[test]
    fn format_token_balances_with_meta_single_with_metadata() {
        let balances = vec![TokenBalance {
            coin_type: "0xabc::usdt::USDT".to_string(),
            coin_object_count: 2,
            total_balance: 1_500_000,
        }];
        let meta = vec![crate::network::CoinMeta {
            coin_type: "0xabc::usdt::USDT".to_string(),
            symbol: "USDT".to_string(),
            decimals: 6,
            name: "Tether USD".to_string(),
        }];
        let output = format_token_balances_with_meta(&balances, &meta);
        assert!(output.contains("USDT"), "should show symbol");
        assert!(output.contains("1"), "should contain whole part");
        assert!(output.contains("5"), "should contain fractional part");
        assert!(output.contains("2 objects"), "should show object count");
    }

    #[test]
    fn format_token_balances_with_meta_no_metadata_fallback() {
        let balances = vec![TokenBalance {
            coin_type: "0xabc::mystery::MYSTERY".to_string(),
            coin_object_count: 1,
            total_balance: 42,
        }];
        let output = format_token_balances_with_meta(&balances, &[]);
        // Fallback shows last segment of coin type and raw amount
        assert!(output.contains("MYSTERY"), "should show coin type suffix");
        assert!(output.contains("42"), "should show raw amount");
    }

    // -- format_chart_label tests --

    #[test]
    fn format_chart_label_whole() {
        let label = format_chart_label(1000.0);
        assert!(label.contains("000"), "should format thousands");
    }

    #[test]
    fn format_chart_label_fractional() {
        let label = format_chart_label(1.55);
        assert!(label.contains("1"), "should contain whole part");
        assert!(label.contains("55"), "should contain fractional digits");
    }

    #[test]
    fn format_chart_label_zero() {
        let label = format_chart_label(0.0);
        assert_eq!(label, "0");
    }
}
