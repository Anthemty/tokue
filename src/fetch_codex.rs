// fetch_codex.rs — fetch billing usage from OpenAI API.
//
// Two requests:
//   1. GET /v1/dashboard/billing/subscription  → hard_limit_usd
//   2. GET /v1/dashboard/billing/usage?start_date=…&end_date=…  → total_usage
// Auth: Bearer sk-...; optional OpenAI-Organization header for org-level billing.

use std::time::Duration;

use chrono::Local;

use crate::config::Config;
use crate::providers::{ProviderFetchResult, UsageMeter};

pub fn fetch(cfg: &Config) -> ProviderFetchResult {
    let key = &cfg.codex.api_key;
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

    // Build the base request with optional org header.
    let mut req_builder = client
        .get("https://api.openai.com/v1/dashboard/billing/subscription")
        .header("Authorization", format!("Bearer {}", key));
    if !cfg.codex.org_id.is_empty() {
        req_builder = req_builder.header("OpenAI-Organization", cfg.codex.org_id.as_str());
    }

    // 1. Fetch subscription to get hard_limit_usd.
    let resp = match req_builder.send() {
        Ok(r) => r,
        Err(e) => return ProviderFetchResult::err(format!("request failed: {}", e)),
    };

    let status = resp.status();
    let body = match resp.text() {
        Ok(b) => b,
        Err(e) => return ProviderFetchResult::err(format!("read body: {}", e)),
    };

    if !status.is_success() {
        // If 401/403, key is invalid; if 429, rate-limited.
        return ProviderFetchResult::err(format!("subscription: HTTP {} {}", status.as_u16(), body.trim()));
    }

    let sub: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return ProviderFetchResult::err(format!("parse subscription: {}", e)),
    };

    let hard_limit = sub["hard_limit_usd"].as_f64().unwrap_or(0.0);
    if hard_limit <= 0.0 {
        return ProviderFetchResult::err("no subscription hard limit");
    }

    // 2. Fetch usage for current month.
    let now = Local::now();
    let start = now.format("%Y-%m-01").to_string();
    let end = now.format("%Y-%m-%d").to_string();
    let usage_url = format!(
        "https://api.openai.com/v1/dashboard/billing/usage?start_date={}&end_date={}",
        start, end
    );

    let mut usage_req = client
        .get(&usage_url)
        .header("Authorization", format!("Bearer {}", key));
    if !cfg.codex.org_id.is_empty() {
        usage_req = usage_req.header("OpenAI-Organization", cfg.codex.org_id.as_str());
    }

    let usage_resp = match usage_req.send() {
        Ok(r) => r,
        Err(e) => return ProviderFetchResult::err(format!("request failed: {}", e)),
    };

    let usage_status = usage_resp.status();
    let usage_body = match usage_resp.text() {
        Ok(b) => b,
        Err(e) => return ProviderFetchResult::err(format!("read body: {}", e)),
    };

    if !usage_status.is_success() {
        return ProviderFetchResult::err(format!("usage: HTTP {} {}", usage_status.as_u16(), usage_body.trim()));
    }

    let usage: serde_json::Value = match serde_json::from_str(&usage_body) {
        Ok(v) => v,
        Err(e) => return ProviderFetchResult::err(format!("parse usage: {}", e)),
    };

    let total_usage = usage["total_usage"].as_f64().unwrap_or(0.0);
    if total_usage < 0.0 {
        return ProviderFetchResult::err("invalid usage data");
    }

    // Criticality: fraction of hard limit used, capped at 100.
    let pct = (total_usage / hard_limit * 100.0) as i32;
    let criticality = pct.clamp(0, 100);

    let detail = format!("${:.2} / ${:.0}", total_usage, hard_limit);

    let meters = vec![UsageMeter {
        label: "Usage".into(),
        percent: criticality,
        detail,
    }];

    ProviderFetchResult::ok(criticality, meters)
}
