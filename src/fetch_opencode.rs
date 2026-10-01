// fetch_opencode.rs — OpenCode Go usage.
//
// Go is a monthly subscription reached through the same gateway as the models
// (https://opencode.ai/zen/go/v1). Two ways in, both read with a bearer:
//   * an API key from the Zen console (Preferences), and
//   * console accounts signed in from OCG with the device code
//     `opencode console login` uses (signin.rs), kept in accounts/opencode.json
//     and refreshed here.
// Per the docs the allowance is a monthly dollar amount, and the shorter
// windows are fractions of it: 5-hour = 20%, weekly = 50%.

use std::time::Duration;

use crate::accounts;
use crate::config::Config;
use crate::providers::{reset_detail, window_meter, ProviderFetchResult, UsageMeter};
use crate::signin::{OPENCODE_CLIENT_ID, OPENCODE_CONSOLE};

const USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";
/// The account read with the API key typed into Preferences.
pub const KEY_ACCOUNT: &str = "opencode:api-key";
const REFRESH_MARGIN_MS: i64 = 5 * 60 * 1000;

struct Account {
    key: String,
    name: String,
    problem: Option<String>,
}

fn accounts_to_read(cfg: &Config) -> Vec<Account> {
    let mut out = all_accounts(cfg);
    out.retain(|a| cfg.account_shown("opencode", &a.key));
    out
}

fn all_accounts(cfg: &Config) -> Vec<Account> {
    let mut out = Vec::new();
    if !cfg.opencode.api_key.is_empty() {
        out.push(Account { key: KEY_ACCOUNT.into(), name: "API key".into(), problem: None });
    }
    for saved in accounts::list("opencode") {
        out.push(Account { key: saved.key.clone(), name: saved.name.clone(), problem: saved.lapsed.clone() });
    }
    out
}

/// Every account, for Preferences.
pub fn listed(cfg: &Config) -> Vec<crate::accounts::Listed> {
    all_accounts(cfg)
        .into_iter()
        .map(|a| crate::accounts::Listed { key: a.key, name: a.name, plan: String::new(), own: false, problem: a.problem })
        .collect()
}

/// A bearer for one account: the API key as it is, a signed-in account's
/// token renewed first when it is about to expire (or `force`d).
fn bearer(cfg: &Config, key: &str, force: bool) -> Result<(String, Option<String>), String> {
    if key == KEY_ACCOUNT {
        return Ok((cfg.opencode.api_key.clone(), None));
    }
    let lock = accounts::key_lock(key);
    let _held = lock.lock().unwrap();
    let saved = accounts::get("opencode", key).ok_or("this account is no longer saved")?;
    if let Some(why) = saved.lapsed {
        return Err(why);
    }
    let field = |k: &str| saved.auth.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let org = Some(field("orgID")).filter(|o| !o.is_empty());
    let expires = saved.auth.get("expires").and_then(|v| v.as_i64()).unwrap_or(0);
    if !force && expires - chrono::Utc::now().timestamp_millis() > REFRESH_MARGIN_MS {
        return Ok((field("access"), org));
    }
    let server = Some(field("server")).filter(|s| !s.is_empty()).unwrap_or_else(|| OPENCODE_CONSOLE.to_string());
    let reply = crate::signin::post_json(
        &format!("{}/auth/device/token", server),
        &serde_json::json!({"grant_type": "refresh_token", "refresh_token": field("refresh"), "client_id": OPENCODE_CLIENT_ID}),
    );
    let tok = match reply {
        Ok(t) => t,
        Err(e) if e.starts_with("HTTP 400") || e.starts_with("HTTP 401") => {
            let why = "signed out — add this account again to sign back in";
            accounts::mark_lapsed("opencode", key, why);
            return Err(why.to_string());
        }
        Err(e) => return Err(format!("token refresh: {}", e)),
    };
    let access = tok.get("access_token").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    if access.is_empty() {
        return Err("token refresh: no token in the reply".to_string());
    }
    let mut auth = saved.auth.clone();
    auth["access"] = access.clone().into();
    if let Some(r) = tok.get("refresh_token").and_then(|v| v.as_str()).filter(|r| !r.is_empty()) {
        auth["refresh"] = r.into();
    }
    let expires_in = tok.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
    auth["expires"] = (chrono::Utc::now().timestamp_millis() + expires_in * 1000).into();
    accounts::update_auth("opencode", key, auth)?;
    Ok((access, org))
}

pub fn fetch(cfg: &Config) -> ProviderFetchResult {
    let list = accounts_to_read(cfg);
    if list.is_empty() {
        return ProviderFetchResult::err("not configured");
    }
    let mut meters: Vec<UsageMeter> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut criticality = 0;
    for acct in &list {
        let label = cfg.account_label("opencode", &acct.key);
        let title = if !label.is_empty() {
            label
        } else if list.len() == 1 && acct.key == KEY_ACCOUNT {
            "OpenCode Go".to_string()
        } else {
            acct.name.clone()
        };
        let r = match &acct.problem {
            Some(p) => ProviderFetchResult::err(p.clone()),
            None => fetch_one(cfg, &acct.key, &title),
        };
        match r.err {
            None => {
                criticality = criticality.max(r.criticality);
                for mut m in r.meters {
                    m.key = Some(acct.key.clone());
                    m.can_start = true;
                    meters.push(m);
                }
            }
            Some(e) => {
                let mut m = UsageMeter::grouped(title, "Login", 0, e.clone());
                m.key = Some(acct.key.clone());
                m.severity = None;
                m.informational = true;
                meters.push(m);
                failures.push(e);
            }
        }
    }
    if failures.len() == list.len() {
        return ProviderFetchResult::err(failures.join("; "));
    }
    ProviderFetchResult::ok(criticality, meters)
}

fn fetch_one(cfg: &Config, key: &str, title: &str) -> ProviderFetchResult {
    let client = match reqwest::blocking::Client::builder().timeout(Duration::from_secs(15)).build() {
        Ok(c) => c,
        Err(e) => return ProviderFetchResult::err(format!("request failed: {}", e)),
    };
    let mut forced = false;
    loop {
        let (token, org) = match bearer(cfg, key, forced) {
            Ok(t) => t,
            Err(e) => return ProviderFetchResult::err(e),
        };
        let mut req = client
            .get(USAGE_URL)
            .header("Authorization", format!("Bearer {}", token))
            .header("Accept", "application/json");
        if let Some(o) = &org {
            req = req.header("x-org-id", o.as_str());
        }
        let resp = match req.send() {
            Ok(r) => r,
            Err(e) => return ProviderFetchResult::err(format!("request failed: {}", e)),
        };
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        if std::env::var("TOKUE_DEBUG_OPENCODE").is_ok() {
            eprintln!("[opencode] {} HTTP {} body={}", key, status.as_u16(), body);
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            if key != KEY_ACCOUNT && !forced {
                forced = true;
                continue;
            }
            return ProviderFetchResult::err(if key == KEY_ACCOUNT {
                "API key rejected — copy it again from the Zen console"
            } else {
                "this account's sign-in is not accepted for Go usage"
            });
        }
        if !status.is_success() {
            return ProviderFetchResult::err(format!(
                "HTTP {}: {}",
                status.as_u16(),
                body.chars().take(80).collect::<String>()
            ));
        }
        let v: serde_json::Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => return ProviderFetchResult::err(format!("parse: {}", e)),
        };
        return build(&v, title, cfg.show_remaining());
    }
}

/// One window as reported by the gateway.
struct Window {
    used_percent: i32,
    reset_at: Option<i64>,
}

/// Read a window from whichever shape the payload uses. The response is not
/// documented, so this accepts a percentage directly or derives one from a
/// used/limit pair, and takes the reset time as unix seconds or milliseconds.
fn parse_window(v: Option<&serde_json::Value>) -> Option<Window> {
    let v = v?;
    if v.is_null() {
        return None;
    }
    let num = |keys: &[&str]| -> Option<f64> {
        keys.iter().find_map(|k| v.get(*k).and_then(|x| x.as_f64()))
    };
    // `percent` is a whole 0-100 figure. Never rescale small values as if they
    // were a 0-1 fraction: a real 1% used would read as 100% and flip the card
    // to "0% left", which is exactly what that guess once did.
    let pct = num(&["percent", "usagePercent", "usage_percent"])
        .or_else(|| {
            let used = num(&["used", "usage", "amount"])?;
            let limit = num(&["limit", "max", "allowance"])?;
            (limit > 0.0).then(|| used / limit * 100.0)
        })?;
    // The gateway sends an ISO-8601 instant ("2026-09-26T20:01:40.815Z");
    // the numeric forms are kept as a fallback in case that ever changes.
    let reset_keys = ["resetsAt", "resets_at", "resetAt", "reset_at"];
    let reset_at = reset_keys
        .iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_str()))
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp())
        .or_else(|| num(&reset_keys).map(|t| if t > 1e11 { (t / 1000.0) as i64 } else { t as i64 }))
        .or_else(|| {
            num(&["resetInSec", "reset_in_sec", "resetsInSeconds"])
                .map(|s| chrono::Utc::now().timestamp() + s as i64)
        });
    Some(Window { used_percent: pct.round().clamp(0.0, 100.0) as i32, reset_at })
}

/// Find a window object by any of its plausible names, at the top level or
/// under a `usage`/`limits` wrapper.
fn window_named<'a>(v: &'a serde_json::Value, names: &[&str]) -> Option<&'a serde_json::Value> {
    let roots = [Some(v), v.get("usage"), v.get("limits"), v.get("data")];
    roots.into_iter().flatten().find_map(|root| names.iter().find_map(|n| root.get(*n)))
}

fn build(v: &serde_json::Value, title: &str, left: bool) -> ProviderFetchResult {
    let windows: [(&str, &[&str]); 3] = [
        ("5h", &["fiveHour", "five_hour", "rolling", "rollingUsage", "5h"]),
        ("Weekly", &["weekly", "weeklyUsage", "week"]),
        ("Monthly", &["monthly", "monthlyUsage", "month"]),
    ];
    let title = title.to_string();
    let mut meters: Vec<UsageMeter> = Vec::new();
    let mut headline: Option<i32> = None;
    for (label, names) in windows {
        let Some(w) = parse_window(window_named(v, names)) else { continue };
        let detail = w.reset_at.map(reset_detail).unwrap_or_default();
        let mut meter = window_meter(label, w.used_percent, detail, left);
        meter.group = Some(title.clone());
        // Shortest window present is the headline, as everywhere else.
        headline.get_or_insert(w.used_percent);
        meters.push(meter);
    }
    if meters.is_empty() {
        return ProviderFetchResult::err("no usage windows in response");
    }
    ProviderFetchResult::ok(headline.unwrap_or(0), meters)
}

/// Start the rolling window now: one billed, one-token request through the
/// gateway. Free models are skipped — they do not count against the
/// allowance, so they would not start anything — and a "flash" model is
/// preferred as the cheapest tier. The model comes from the live list, since
/// that list turns over.
///
/// The gateway refuses a request without `x-opencode-session` ("cannot be
/// routed efficiently"). It is a routing key, not a credential: any fresh id
/// is accepted, so each press sends a new one.
pub fn start_window(cfg: &Config, account: &str) -> Result<(), String> {
    let (key, _) = bearer(cfg, account, false)?;
    let key = &key;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent("tokue/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let models_body = client
        .get("https://opencode.ai/zen/go/v1/models")
        .header("Authorization", format!("Bearer {}", key))
        .send()
        .and_then(|r| r.text())
        .map_err(|e| format!("model list: {}", e))?;
    let models: serde_json::Value =
        serde_json::from_str(&models_body).map_err(|e| format!("model list: {}", e))?;
    let billed: Vec<&str> = models
        .get("data")
        .and_then(|d| d.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|m| m.get("id").and_then(|i| i.as_str()))
                .filter(|id| !id.ends_with("-free"))
                .collect()
        })
        .unwrap_or_default();
    let model = billed
        .iter()
        .find(|id| id.contains("flash"))
        .or_else(|| billed.first())
        .ok_or("no billed model available")?
        .to_string();

    let session = format!("tokue-{:x}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "Reply with the single word OK."}],
        "max_tokens": 1,
    });
    let resp = client
        .post("https://opencode.ai/zen/go/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", key))
        .header("Content-Type", "application/json")
        .header("x-opencode-session", session)
        .body(body.to_string())
        .send()
        .map_err(|e| format!("request failed: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().unwrap_or_default();
        return Err(format!("HTTP {}: {}", status.as_u16(), text.chars().take(120).collect::<String>()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_percentages_and_prefers_the_shortest_window() {
        let v = serde_json::json!({
            "fiveHour": {"usagePercent": 18, "resetsAt": 4102444800i64},
            "weekly": {"usagePercent": 61, "resetsAt": 4102444800i64},
            "monthly": {"usagePercent": 44, "resetsAt": 4102444800i64}
        });
        let r = build(&v, "OpenCode Go", true);
        assert!(r.err.is_none());
        assert_eq!(r.criticality, 18, "the 5h window is the headline");
        let labels: Vec<&str> = r.meters.iter().map(|m| m.label.as_str()).collect();
        assert_eq!(labels, vec!["5h left", "Weekly left", "Monthly left"]);
        assert_eq!(r.meters[0].percent, 82);
        assert_eq!(r.meters[0].severity, Some(18));
    }

    #[test]
    fn reads_the_shape_the_gateway_actually_returns() {
        // Captured from /zen/go/v1/usage — note resetsAt is an ISO-8601 string.
        let v: serde_json::Value = serde_json::from_str(
            r#"{"usage":{"rolling":{"status":"ok","percent":12,"resetsAt":"2099-09-26T20:01:40.815Z"},
                 "weekly":{"status":"ok","percent":34,"resetsAt":"2099-09-28T00:00:00.000Z"},
                 "monthly":{"status":"ok","percent":56,"resetsAt":"2099-10-26T09:31:15.000Z"}}}"#,
        )
        .unwrap();
        let r = build(&v, "OpenCode Go", true);
        assert!(r.err.is_none());
        assert_eq!(r.criticality, 12, "rolling is the headline");
        assert_eq!(r.meters.len(), 3);
        assert_eq!(r.meters[0].percent, 88);
        assert_eq!(r.meters[2].severity, Some(56));
        assert!(r.meters[0].detail.starts_with("→ "), "the reset instant must render: {:?}", r.meters[0].detail);
    }

    #[test]
    fn derives_a_percentage_from_a_used_limit_pair() {
        // Go's allowance is a dollar amount, so used/limit is the likelier shape.
        let v = serde_json::json!({"usage": {"weekly": {"used": 3.0, "limit": 5.0}}});
        let r = build(&v, "OpenCode Go", false);
        assert_eq!(r.meters.len(), 1);
        assert_eq!(r.meters[0].label, "Weekly");
        assert_eq!(r.meters[0].percent, 60);
    }

    #[test]
    fn one_percent_used_is_one_percent_not_a_full_fraction() {
        // Captured the moment usage first ticked over: `percent: 1`. A 0-1
        // fraction guess turned this into 100% used / 0% left and a red badge.
        let v = serde_json::json!({"usage": {
            "rolling": {"status": "ok", "percent": 1, "resetsAt": "2099-09-26T20:01:40.815Z"},
            "weekly":  {"status": "ok", "percent": 0, "resetsAt": "2099-09-28T00:00:00.000Z"}
        }});
        let r = build(&v, "OpenCode Go", true);
        assert_eq!(r.criticality, 1);
        assert_eq!(r.meters[0].percent, 99, "1% used is 99% left");
        assert_eq!(r.meters[0].severity, Some(1));
    }

    #[test]
    fn nothing_recognisable_is_an_error_rather_than_an_empty_card() {
        assert!(build(&serde_json::json!({"hello": "world"}), "OpenCode Go", true).err.is_some());
    }
}
