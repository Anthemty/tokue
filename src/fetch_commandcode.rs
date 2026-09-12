// fetch_commandcode.rs — Command Code (commandcode.ai) credit usage.
//
// Reads the CLI's own credentials from ~/.commandcode/auth.json (never writes)
// and replays the same four calls its /usage overlay makes against
// https://api.commandcode.ai (Bearer api-key auth):
//
//   GET /alpha/whoami                         -> org id (null on personal accounts)
//   GET /alpha/billing/credits[?orgId=]       -> credits + windowLimits (5h / weekly)
//   GET /alpha/billing/subscriptions[?orgId=] -> plan, status, period end
//
// windowLimits carries used/cap per window plus a resetAt, which maps straight
// onto the same 5h/weekly meters the other providers show.
//
// Cloudflare fronts this API and rejects some non-browser TLS stacks (error
// 1010), so every call falls back to the system curl when reqwest is refused.

use std::time::Duration;

use serde::Deserialize;

use crate::config::Config;
use crate::providers::{format_duration, ProviderFetchResult, UsageMeter};

const API_BASE: &str = "https://api.commandcode.ai";
const AUTH_SUBPATH: &str = ".commandcode/auth.json";
const USER_AGENT: &str = "commandcode/0.5.0";

#[derive(Deserialize)]
struct AuthFile {
    #[serde(rename = "apiKey", default)]
    api_key: String,
    #[serde(rename = "userName", default)]
    user_name: String,
}

/// planId → monthly credits, from the CLI's own plan table.
fn plan_monthly_credits(plan_id: &str) -> Option<f64> {
    let credits = match plan_id {
        "individual-go" => 10.0,
        "individual-goat" => 70.0,
        "individual-pro" | "individual-pro-v1" => 30.0,
        "individual-provider" => 15.0,
        "individual-max" => 150.0,
        "individual-ultra" => 300.0,
        "teams-pro" => 40.0,
        _ => return None,
    };
    Some(credits)
}

/// Numeric field with a floor of zero.
fn f(value: &serde_json::Value, key: &str) -> f64 {
    value.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0).max(0.0)
}

pub fn fetch(cfg: &Config) -> ProviderFetchResult {
    let auth = match read_auth() {
        Some(a) => a,
        None => return ProviderFetchResult::err("not logged in — run `commandcode login`"),
    };
    if auth.api_key.is_empty() {
        return ProviderFetchResult::err("no API key in ~/.commandcode/auth.json");
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(USER_AGENT)
        .build()
        .ok();

    let whoami = match get(&client, &auth.api_key, "/alpha/whoami") {
        Ok(v) => v,
        Err(e) => return ProviderFetchResult::err(e),
    };
    let org_id = whoami
        .pointer("/org/id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let org_query = if org_id.is_empty() {
        String::new()
    } else {
        format!("?orgId={}", org_id)
    };

    let summary = match get(&client, &auth.api_key, &format!("/alpha/usage/summary{}", org_query)) {
        Ok(v) => v,
        Err(e) => return ProviderFetchResult::err(e),
    };
    let summary_total = f(&summary, "totalCost");
    let billing = match get(&client, &auth.api_key, &format!("/alpha/billing/credits{}", org_query)) {
        Ok(v) => v,
        Err(e) => return ProviderFetchResult::err(e),
    };
    let credits = billing.get("credits").cloned().unwrap_or(serde_json::Value::Null);
    let window_limits = billing.get("windowLimits").cloned().unwrap_or(serde_json::Value::Null);
    let subscription = match get(
        &client,
        &auth.api_key,
        &format!("/alpha/billing/subscriptions{}", org_query),
    ) {
        Ok(v) => v.get("data").cloned().unwrap_or(serde_json::Value::Null),
        Err(e) => return ProviderFetchResult::err(e),
    };

    let show_remaining = cfg.codex.show_remaining;
    let mut title = if auth.user_name.is_empty() {
        "Command Code".to_string()
    } else {
        auth.user_name.clone()
    };
    let plan_id = subscription.get("planId").and_then(|v| v.as_str()).unwrap_or("");
    if !plan_id.is_empty() {
        title.push_str(" · ");
        title.push_str(&plan_display(plan_id));
    }

    let status = subscription.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let monthly_remaining = f(&credits, "monthlyCredits");
    let purchased_remaining = f(&credits, "purchasedCredits");
    let free_remaining = f(&credits, "freeCredits");
    let total_remaining = monthly_remaining + purchased_remaining + free_remaining;

    let mut meters: Vec<UsageMeter> = Vec::new();
    let mut worst = 0i32;
    let limits = Some(&window_limits);
    for (name, window) in [
        ("5h", limits.and_then(|w| w.get("fiveHour"))),
        ("Weekly", limits.and_then(|w| w.get("weekly"))),
    ] {
        let window = match window {
            Some(w) if w.is_object() => w,
            _ => continue,
        };
        let used = window.get("used").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let cap = window.get("cap").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let used_percent = window_percent(used, cap);
        let display = if show_remaining {
            100.0 - used_percent
        } else {
            used_percent
        };
        worst = worst.max(used_percent.round() as i32);

        // resetAt is already Unix milliseconds — format_reset_clock takes ms too.
        let reset_at = window.get("resetAt").and_then(|v| v.as_f64()).map(|ms| ms as i64);
        let reset = reset_at
            .map(|at| format!("→ {}", format_reset_clock(at)))
            .unwrap_or_default();
        let mut meter = UsageMeter::grouped(
            title.clone(),
            name.to_string(),
            display.round() as i32,
            format!("{:.1} / {:.0} cr · {}", used, cap, reset),
        );
        meter.severity = Some(used_percent.round() as i32);
        meter.badge = Some(plan_display(plan_id));
        meters.push(meter);
    }

    // Monthly pool: an active subscription caps the plan's monthly credits.
    // Same pool math as the CLI's projectUsageView.
    let plan_monthly = if status == "active" {
        plan_monthly_credits(plan_id)
    } else {
        None
    };
    let total_pool = match plan_monthly {
        Some(plan) => plan.max(monthly_remaining) + purchased_remaining + free_remaining,
        None => summary_total + total_remaining,
    };
    if total_pool > 0.0 {
        let monthly_used = (total_pool - total_remaining).max(0.0);
        let used_percent = (monthly_used / total_pool * 100.0).clamp(0.0, 100.0);
        let display = if show_remaining {
            100.0 - used_percent
        } else {
            used_percent
        };
        let days_left = subscription
            .get("currentPeriodEnd")
            .and_then(period_end_days)
            .map(|d| d.to_string())
            .unwrap_or_else(|| "?".to_string());
        let mut meter = UsageMeter::grouped(
            title.clone(),
            "Monthly",
            display.round() as i32,
            format!(
                "{:.1} / {:.0} cr · renews in {}d",
                total_remaining, total_pool, days_left
            ),
        );
        meter.severity = Some(used_percent.round() as i32);
        meters.push(meter);
    }

    // Credit balance is informational: monthly/purchased/free remaining.
    let mut parts: Vec<String> = vec![format!("{:.1} monthly", monthly_remaining)];
    if purchased_remaining > 0.0 {
        parts.push(format!("{:.1} purchased", purchased_remaining));
    }
    if free_remaining > 0.0 {
        parts.push(format!("{:.1} free", free_remaining));
    }
    let mut meter = UsageMeter::grouped(title, "Credits", 0, parts.join(" · "));
    meter.severity = None;
    meters.push(meter);

    if meters.is_empty() {
        return ProviderFetchResult::err("no usage data returned");
    }
    ProviderFetchResult::ok(worst, meters)
}

fn read_auth() -> Option<AuthFile> {
    let path = dirs::home_dir()?.join(AUTH_SUBPATH);
    let data = std::fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

fn get(
    client: &Option<reqwest::blocking::Client>,
    api_key: &str,
    endpoint: &str,
) -> Result<serde_json::Value, String> {
    if let Some(client) = client {
        if let Ok(v) = request(client, api_key, endpoint) {
            return Ok(v);
        }
        // Cloudflare may reject the rustls fingerprint; curl (native TLS) passes.
    }
    curl_get(api_key, endpoint)
}

fn request(
    client: &reqwest::blocking::Client,
    api_key: &str,
    endpoint: &str,
) -> Result<serde_json::Value, String> {
    let resp = client
        .get(format!("{}{}", API_BASE, endpoint))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .map_err(|e| format!("request failed: {}", e))?;
    let status = resp.status();
    let body = resp.text().map_err(|e| format!("read body: {}", e))?;
    check_status(status.as_u16(), &body)?;
    serde_json::from_str(&body).map_err(|e| format!("parse: {}", e))
}

fn check_status(status: u16, body: &str) -> Result<(), String> {
    if status == 401 || status == 403 {
        // Cloudflare challenges and expired keys both land here; the curl
        // fallback gets a second opinion.
        return Err(format!(
            "HTTP {} {}",
            status,
            body.trim().chars().take(80).collect::<String>()
        ));
    }
    if (200..300).contains(&status) {
        return Ok(());
    }
    Err(format!(
        "HTTP {} {}",
        status,
        body.trim().chars().take(120).collect::<String>()
    ))
}

/// System curl with native TLS; headers go in via stdin so the API key never
/// reaches the process table.
fn curl_get(api_key: &str, endpoint: &str) -> Result<serde_json::Value, String> {
    let config_text = format!(
        "header = \"Authorization: Bearer {}\"\nheader = \"Accept: application/json\"\nheader = \"User-Agent: {}\"\n",
        api_key, USER_AGENT
    );
    let mut child = std::process::Command::new("/usr/bin/curl")
        .args([
            "-sS",
            "--max-time",
            "20",
            "--config",
            "-",
            &format!("{}{}", API_BASE, endpoint),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl: {}", e))?;
    use std::io::Write;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(config_text.as_bytes())
            .map_err(|e| format!("curl stdin: {}", e))?;
    }
    let out = child.wait_with_output().map_err(|e| format!("curl wait: {}", e))?;
    let body = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "curl failed: {}",
            err.trim().chars().take(100).collect::<String>()
        ));
    }
    serde_json::from_str(&body).map_err(|e| format!("parse: {}", e))
}

/// used / cap → percent of the window consumed.
fn window_percent(used: f64, cap: f64) -> f64 {
    if cap <= 0.0 {
        return 0.0;
    }
    (used / cap * 100.0).clamp(0.0, 100.0)
}

/// planId → display name, matching the CLI's plan naming.
fn plan_display(plan_id: &str) -> String {
    let trimmed = plan_id
        .trim_start_matches("individual-")
        .trim_start_matches("teams-");
    let pretty = match trimmed {
        "go" => "Go",
        "goat" => "Goat",
        "pro" => "Pro",
        "max" => "Max",
        "ultra" => "Ultra",
        "provider" => "Provider",
        other => other,
    };
    if plan_id.starts_with("teams-") {
        format!("Teams {}", pretty)
    } else {
        pretty.to_string()
    }
}

/// resetAt is Unix milliseconds → "HH:MM" when today, "M/D HH:MM" otherwise.
fn format_reset_clock(reset_at_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    let dt = match Local.timestamp_millis_opt(reset_at_ms).single() {
        Some(dt) => dt,
        None => return format_duration(reset_at_ms / 1000),
    };
    if dt.date_naive() == Local::now().date_naive() {
        dt.format("%H:%M").to_string()
    } else {
        dt.format("%-m/%-d %H:%M").to_string()
    }
}

/// currentPeriodEnd（ISO 字符串或 epoch 毫秒）→ 距今整天数。
fn period_end_days(value: &serde_json::Value) -> Option<i64> {
    let end_ms = match value {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => {
            let parsed = chrono::DateTime::parse_from_rfc3339(s).ok()?;
            Some(parsed.timestamp_millis() as f64)
        }
        _ => None,
    }?;
    let now_ms = chrono::Utc::now().timestamp_millis() as f64;
    Some(((end_ms - now_ms) / 86_400_000.0).ceil() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_percent_bounds() {
        assert_eq!(window_percent(0.0, 14.0), 0.0);
        assert_eq!(window_percent(7.0, 14.0), 50.0);
        assert_eq!(window_percent(6.52, 35.0).round() as i64, 19);
        assert_eq!(window_percent(20.0, 14.0), 100.0, "overuse clamps");
        assert_eq!(window_percent(5.0, 0.0), 0.0, "no cap means no percent");
    }

    #[test]
    fn formats_reset_clock() {
        use chrono::{Local, TimeZone};
        // Singapore sat at +07:30 before 1981, so a fixed reference must use a
        // modern date or the historical offset breaks the expectation. The date
        // also has to be in the future: today takes the clock-only branch.
        let ms = Local.with_ymd_and_hms(2027, 3, 5, 9, 30, 0).unwrap().timestamp_millis();
        assert_eq!(format_reset_clock(ms), "3/5 09:30", "another day shows the date");
        let ms = Local.with_ymd_and_hms(2026, 9, 17, 18, 41, 0).unwrap().timestamp_millis();
        assert_eq!(format_reset_clock(ms), "9/17 18:41");
    }

    #[test]
    fn monthly_pool_follows_the_cli_math() {
        // Goat = 70 credits；月度剩余 50.37，无购入/免费 → 已用 ≈ 19.63
        let plan = plan_monthly_credits("individual-goat").unwrap();
        let monthly_remaining = 50.3734994493f64;
        let pool = plan.max(monthly_remaining) + 0.0 + 0.0;
        let used = (pool - monthly_remaining).max(0.0);
        assert!((pool - 70.0).abs() < 0.01);
        assert!((used - 19.63).abs() < 0.01);
        assert!(((used / pool * 100.0) - 28.05).abs() < 0.1);
    }

    #[test]
    fn plan_names() {
        assert_eq!(plan_display("individual-pro"), "Pro");
        assert_eq!(plan_display("teams-pro"), "Teams Pro");
        assert_eq!(plan_display("individual-goat"), "Goat");
    }
}
