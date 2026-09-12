// fetch_opencode.rs — fetch usage from opencode.ai.
//
// Faithful port of opencode.go: scrapes Solid.js serialized state ($R[N]={...})
// from the workspace page HTML, with redirect validation for session expiry.

use std::time::Duration;

use crate::config::Config;
use crate::providers::{format_duration, ProviderFetchResult, UsageMeter};

use regex::Regex;

const DEFAULT_WORKSPACE: &str = "wrk_01KJHHDX0J71PAM2N7VDV6TKQF";

// $R[N]={status:"ok",resetInSec:123,usagePercent:42}
// Captures: label, status, resetInSec, usagePercent.
fn usage_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(rollingUsage|weeklyUsage|monthlyUsage):\$R\[\d+\]=\{status:"(\w+)",resetInSec:(\d+),usagePercent:(\d+)\}"#,
        ).unwrap()
    })
}

#[derive(Clone)]
struct Meter {
    percent: i32,
    reset_in_sec: i64,
}

pub fn fetch(cfg: &Config) -> ProviderFetchResult {
    let cookie = &cfg.opencode.auth_cookie;
    if cookie.is_empty() {
        return ProviderFetchResult::err("not configured");
    }

    let ws = if cfg.opencode.workspace_id.is_empty() {
        DEFAULT_WORKSPACE
    } else {
        &cfg.opencode.workspace_id
    };
    let url = format!("https://opencode.ai/workspace/{}/go", ws);

    // Build a blocking client. reqwest's redirect::Policy::custom gives us each
    // hop, mirroring Go's CheckRedirect callback for session-expiry detection.
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 3 {
                attempt.error("too many redirects — session may be expired")
            } else if !attempt.previous().is_empty()
                && !attempt.url().as_str().contains("opencode.ai/workspace")
            {
                attempt.error("redirected — session expired or invalid cookie")
            } else {
                attempt.follow()
            }
        }))
        .build()
    {
        Ok(c) => c,
        Err(e) => return ProviderFetchResult::err(format!("request failed: {}", e)),
    };

    let resp = match client
        .get(&url)
        .header("User-Agent", "ocg/1.0")
        .header("Cookie", cookie.as_str())
        .send()
    {
        Ok(r) => r,
        Err(e) => return ProviderFetchResult::err(format!("request failed: {}", e)),
    };

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return ProviderFetchResult::err(format!("HTTP {}: {}", status.as_u16(), body.trim()));
    }

    let body = match resp.text() {
        Ok(b) => b,
        Err(e) => return ProviderFetchResult::err(format!("read body: {}", e)),
    };

    let re = usage_re();
    let mut usage: std::collections::HashMap<&str, Meter> = std::collections::HashMap::new();
    for caps in re.captures_iter(&body) {
        let label = caps.get(1).unwrap().as_str();
        let reset_in_sec: i64 = caps.get(3).unwrap().as_str().parse().unwrap_or(0);
        let percent: i32 = caps.get(4).unwrap().as_str().parse().unwrap_or(0);
        usage.insert(
            label,
            Meter { percent, reset_in_sec },
        );
    }

    if usage.is_empty() {
        return ProviderFetchResult::err("could not find usage data in page");
    }

    let r = usage.get("rollingUsage").cloned().unwrap_or(Meter { percent: 0, reset_in_sec: 0 });
    let w = usage.get("weeklyUsage").cloned().unwrap_or(Meter { percent: 0, reset_in_sec: 0 });
    let mo = usage.get("monthlyUsage").cloned().unwrap_or(Meter { percent: 0, reset_in_sec: 0 });

    let max_pct = r.percent.max(w.percent).max(mo.percent);

    let meters = vec![
        UsageMeter::new("Rolling", r.percent, format_duration(r.reset_in_sec)),
        UsageMeter::new("Weekly", w.percent, format_duration(w.reset_in_sec)),
        UsageMeter::new("Monthly", mo.percent, format_duration(mo.reset_in_sec)),
    ];

    ProviderFetchResult::ok(max_pct, meters)
}
