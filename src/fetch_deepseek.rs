// fetch_deepseek.rs — fetch balance from api.deepseek.com.
//
// Faithful port of deepseek.go: GET /user/balance, parse monetary balance
// (string fields), compute criticality as (1 − total/50)×100.

use std::time::Duration;

use serde::Deserialize;

use crate::config::Config;
use crate::providers::{ProviderFetchResult, UsageMeter};

#[derive(Deserialize)]
struct BalanceResp {
    #[serde(rename = "is_available", default)]
    is_available: bool,
    #[serde(rename = "balance_infos", default)]
    balance_infos: Vec<BalanceInfo>,
}

#[derive(Deserialize)]
struct BalanceInfo {
    #[serde(default)]
    currency: String,
    #[serde(rename = "total_balance", default)]
    total_balance: String,
    #[serde(rename = "granted_balance", default)]
    granted_balance: String,
    #[serde(rename = "topped_up_balance", default)]
    topped_up_balance: String,
}

pub fn fetch(cfg: &Config) -> ProviderFetchResult {
    let key = &cfg.deepseek.api_key;
    if key.is_empty() {
        return ProviderFetchResult::err("not configured");
    }

    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => return ProviderFetchResult::err(format!("request failed: {}", e)),
    };

    let resp = match client
        .get("https://api.deepseek.com/user/balance")
        .header("Authorization", format!("Bearer {}", key))
        .send()
    {
        Ok(r) => r,
        Err(e) => return ProviderFetchResult::err(format!("request failed: {}", e)),
    };

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return ProviderFetchResult::err(format!("HTTP {}: {}", status.as_u16(), body));
    }

    let body = match resp.text() {
        Ok(b) => b,
        Err(e) => return ProviderFetchResult::err(format!("read body: {}", e)),
    };

    let bal: BalanceResp = match serde_json::from_str(&body) {
        Ok(b) => b,
        Err(e) => return ProviderFetchResult::err(format!("parse: {}", e)),
    };

    if !bal.is_available {
        return ProviderFetchResult::err("account is not available");
    }
    if bal.balance_infos.is_empty() {
        return ProviderFetchResult::err("no balance info");
    }

    let info = &bal.balance_infos[0];
    let total = parse_float(&info.total_balance);

    let cur = match info.currency.as_str() {
        "CNY" => "¥".to_string(),
        "USD" => "$".to_string(),
        other => format!("{} ", other),
    };

    // Criticality: 100 if total <= 0; 0 if total >= 50; else (1 − total/50)×100.
    let criticality = if total <= 0.0 {
        100
    } else if total >= 50.0 {
        0
    } else {
        let c = ((1.0 - total / 50.0) * 100.0) as i32;
        if c < 0 { 0 } else { c }
    };

    let meters = vec![
        UsageMeter::new("Balance", criticality, format!("{}{} left", cur, info.total_balance)),
        UsageMeter::new("Granted", 0, format!("{}{}", cur, info.granted_balance)),
        UsageMeter::new("Topped", 0, format!("{}{}", cur, info.topped_up_balance)),
    ];

    ProviderFetchResult::ok(criticality, meters)
}

/// Parse a JSON numeric string (e.g. "10.00") to f64; 0 on failure.
/// Matches Go's parseFloat (json.Unmarshal of a string into float64).
fn parse_float(s: &str) -> f64 {
    serde_json::from_str::<f64>(s).unwrap_or(0.0)
}
