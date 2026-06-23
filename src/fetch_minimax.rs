// fetch_minimax.rs — fetch token-plan quota from MiniMax.
//
// Faithful port of minimax.go: tries two hosts, parses via flexible map
// helpers (snake_case + camelCase, base_resp recursion), computes used %
// from remaining %.

use std::time::Duration;

use chrono::{DateTime, Local, TimeZone, Utc};

use crate::config::Config;
use crate::providers::{ProviderFetchResult, UsageMeter};

pub fn fetch(cfg: &Config) -> ProviderFetchResult {
    let key = &cfg.minimax.api_key;
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

    // Try multiple hosts; first valid 200 + parseable JSON wins.
    let mut raw: Option<serde_json::Value> = None;
    let mut errors: Vec<String> = Vec::new();

    for host in &["https://www.minimax.io", "https://api.minimax.io"] {
        let url = format!("{}/v1/token_plan/remains", host);
        let resp = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", key))
            .header("Content-Type", "application/json")
            .send();

        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("{}: {}", url, e));
                continue;
            }
        };

        let status = resp.status();
        let body = match resp.text() {
            Ok(b) => b,
            Err(e) => {
                errors.push(format!("{}: read {}", url, e));
                continue;
            }
        };

        if !status.is_success() {
            errors.push(format!("{}: HTTP {} {}", url, status.as_u16(), body.trim()));
            continue;
        }

        match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(v) => {
                raw = Some(v);
                errors.clear();
                break;
            }
            Err(e) => {
                errors.push(format!("{}: parse {}", url, e));
                continue;
            }
        }
    }

    let raw = match raw {
        Some(v) => v,
        None => {
            let mut msg = errors.join("; ");
            if msg.len() > 80 {
                msg.truncate(80);
                msg.push('…');
            }
            return ProviderFetchResult::err(format!("request failed: {}", msg));
        }
    };

    // API status check: getInt recurses into base_resp.
    let status_code = get_int(&raw, &["status_code"]);
    if status_code != 0 {
        // getString does NOT recurse, so status_msg under base_resp is missed
        // (matches the Go quirk — error message stays empty).
        let msg = get_string(&raw, &["status_msg"]);
        return ProviderFetchResult::err(format!("api error: {}", msg));
    }

    // Extract model_remains (root level, or under "data").
    let mut remains = get_slice(&raw, &["model_remains", "modelRemains"]).cloned();
    if remains.is_none() {
        if let Some(data_obj) = get_map(&raw, &["data"]) {
            remains = get_slice_map(data_obj, &["model_remains", "modelRemains"]).cloned();
        }
    }
    let remains = match remains {
        Some(r) => r,
        None => return ProviderFetchResult::err("no model_remains"),
    };

    // Find the "general" model entry (skip video/image/etc).
    let mut entry: Option<&serde_json::Value> = None;
    for item in &remains {
        if let Some(m) = item.as_object() {
            let name = get_string_obj(m, &["model_name", "modelName"]);
            if name == "general" || name == "chat" || name == "text" {
                entry = Some(item);
                break;
            }
        }
    }
    if entry.is_none() {
        if let Some(first) = remains.first() {
            if first.is_object() {
                entry = Some(first);
            }
        }
    }
    let entry = match entry {
        Some(e) => e,
        None => return ProviderFetchResult::err("no model data"),
    };

    let interval_remaining = get_float(entry, &["current_interval_remaining_percent", "currentIntervalRemainingPercent"]);
    let weekly_remaining = get_float(entry, &["current_weekly_remaining_percent", "currentWeeklyRemainingPercent"]);

    let interval_used = (100.0 - interval_remaining) as i32;
    let weekly_used = (100.0 - weekly_remaining) as i32;

    // Reset times from ms-epoch, rounded up (+1).
    let reset_5h = {
        let end_ms = get_float(entry, &["end_time", "endTime"]);
        if end_ms > 0.0 {
            if let Some(t) = Utc.timestamp_millis_opt(end_ms as i64).single() {
                let local: DateTime<Local> = DateTime::from(t);
                let now = Local::now();
                if local > now {
                    let h = (local - now).num_seconds() / 3600;
                    format!("{}h", h + 1)
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    };

    let reset_week = {
        let week_end_ms = get_float(entry, &["weekly_end_time", "weeklyEndTime"]);
        if week_end_ms > 0.0 {
            if let Some(t) = Utc.timestamp_millis_opt(week_end_ms as i64).single() {
                let local: DateTime<Local> = DateTime::from(t);
                let now = Local::now();
                if local > now {
                    let h = (local - now).num_seconds() / 3600;
                    format!("{}d", h / 24 + 1)
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    };

    let criticality = interval_used.max(weekly_used);

    let meters = vec![
        UsageMeter { label: "5h".into(), percent: interval_used, detail: reset_5h },
        UsageMeter { label: "Weekly".into(), percent: weekly_used, detail: reset_week },
    ];

    ProviderFetchResult::ok(criticality, meters)
}

// ---------- flexible JSON helpers (port of minimax.go) ----------

/// Get an int, recursing into base_resp/baseResp if not found at top level.
fn get_int(v: &serde_json::Value, keys: &[&str]) -> i32 {
    if let Some(obj) = v.as_object() {
        for k in keys {
            if let Some(val) = obj.get(*k) {
                if let Some(n) = val.as_i64() {
                    return n as i32;
                }
                if let Some(n) = val.as_f64() {
                    return n as i32;
                }
            }
        }
        // Recurse into base_resp / baseResp.
        for base_key in &["base_resp", "baseResp"] {
            if let Some(sub) = obj.get(*base_key) {
                let r = get_int(sub, keys);
                if r != 0 {
                    return r;
                }
            }
        }
    }
    0
}

/// Get a string (no recursion).
fn get_string(v: &serde_json::Value, keys: &[&str]) -> String {
    if let Some(obj) = v.as_object() {
        for k in keys {
            if let Some(val) = obj.get(*k) {
                if let Some(s) = val.as_str() {
                    return s.to_string();
                }
            }
        }
    }
    String::new()
}

fn get_string_obj(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> String {
    for k in keys {
        if let Some(val) = obj.get(*k) {
            if let Some(s) = val.as_str() {
                return s.to_string();
            }
        }
    }
    String::new()
}

/// Get a float from number or numeric string (no recursion).
fn get_float(v: &serde_json::Value, keys: &[&str]) -> f64 {
    if let Some(obj) = v.as_object() {
        for k in keys {
            if let Some(val) = obj.get(*k) {
                if let Some(n) = val.as_f64() {
                    return n;
                }
                if let Some(s) = val.as_str() {
                    if let Ok(f) = serde_json::from_str::<f64>(s) {
                        return f;
                    }
                }
            }
        }
    }
    0.0
}

/// Get a nested map (no recursion).
fn get_map<'a>(v: &'a serde_json::Value, keys: &[&str]) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    if let Some(obj) = v.as_object() {
        for k in keys {
            if let Some(val) = obj.get(*k) {
                if let Some(m) = val.as_object() {
                    return Some(m);
                }
            }
        }
    }
    None
}

/// Get an array (no recursion).
fn get_slice<'a>(v: &'a serde_json::Value, keys: &[&str]) -> Option<&'a Vec<serde_json::Value>> {
    if let Some(obj) = v.as_object() {
        for k in keys {
            if let Some(val) = obj.get(*k) {
                if let Some(a) = val.as_array() {
                    return Some(a);
                }
            }
        }
    }
    None
}

/// Get an array from a Map directly (for the data-wrapper fallback).
fn get_slice_map<'a>(
    obj: &'a serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<&'a Vec<serde_json::Value>> {
    for k in keys {
        if let Some(val) = obj.get(*k) {
            if let Some(a) = val.as_array() {
                return Some(a);
            }
        }
    }
    None
}
