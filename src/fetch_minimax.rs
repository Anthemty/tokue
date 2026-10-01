// fetch_minimax.rs — fetch token-plan quota from MiniMax.
//
// Faithful port of minimax.go: tries two hosts, parses via flexible map
// helpers (snake_case + camelCase, base_resp recursion), computes used %
// from remaining %.

use std::time::Duration;

use chrono::Utc;

use crate::config::Config;
use crate::providers::{format_duration, window_meter, ProviderFetchResult};

/// Meter key for the single MiniMax card (one API key, one plan).
pub const CARD_KEY: &str = "minimax";
/// Card title: the API calls the subscription a token plan.
const CARD_TITLE: &str = "Token plan";

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

    let reset_5h = reset_in(entry, &["end_time", "endTime"]);
    let reset_week = reset_in(entry, &["weekly_end_time", "weeklyEndTime"]);

    // Prefer the 5h window as the headline number over weekly — the shorter
    // window is the more immediately actionable one, not whichever is worse.
    let criticality = interval_used;

    let left = cfg.show_remaining();
    let mut meters = vec![
        window_meter("5h", interval_used, reset_5h, left),
        window_meter("Weekly", weekly_used, reset_week, left),
    ];
    // One card, like the other subscription providers, so it has somewhere to
    // carry the start-window button and its feedback.
    for meter in meters.iter_mut() {
        meter.group = Some(CARD_TITLE.to_string());
        meter.key = Some(CARD_KEY.to_string());
        meter.can_start = true;
    }

    ProviderFetchResult::ok(criticality, meters)
}

/// Send the cheapest real request the plan will count — one token from the
/// current model — so the window registers activity now. (MiniMax's windows
/// appear to run on a fixed schedule rather than starting with the first
/// request, so unlike Codex this does not move the reset time; it is offered
/// for parity and for the case where a plan does behave that way.)
pub fn start_window(cfg: &Config) -> Result<(), String> {
    let key = &cfg.minimax.api_key;
    if key.is_empty() {
        return Err("not configured".to_string());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let body = serde_json::json!({
        "model": "MiniMax-M2",
        "messages": [{"role": "user", "content": "Reply with the single word OK."}],
        "max_tokens": 1,
    });
    let resp = client
        .post("https://api.minimax.io/v1/text/chatcompletion_v2")
        .header("Authorization", format!("Bearer {}", key))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .map_err(|e| format!("request failed: {}", e))?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() {
        return Err(format!("HTTP {}: {}", status.as_u16(), text.chars().take(120).collect::<String>()));
    }
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("parse: {}", e))?;
    let code = get_int(&v, &["status_code"]);
    if code != 0 {
        let msg = v
            .get("base_resp")
            .and_then(|b| b.get("status_msg"))
            .and_then(|m| m.as_str())
            .unwrap_or("");
        return Err(format!("api error {}: {}", code, msg));
    }
    Ok(())
}

/// Time left until a window's ms-epoch end, in the same wording every other
/// provider uses ("2h 21m", "1d 4h"). The old bespoke arithmetic forced whole
/// hours on the 5h window and whole DAYS on the weekly one, so a weekly window
/// six hours from resetting was reported as "1d".
fn reset_in(entry: &serde_json::Value, keys: &[&str]) -> String {
    let end_ms = get_float(entry, keys);
    if end_ms <= 0.0 {
        return String::new();
    }
    let secs = end_ms as i64 / 1000 - Utc::now().timestamp();
    if secs <= 0 {
        return String::new();
    }
    format_duration(secs)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_ending_in(secs_5h: i64, secs_week: i64) -> serde_json::Value {
        let now_ms = Utc::now().timestamp() * 1000;
        serde_json::json!({
            "model_name": "general",
            "end_time": now_ms + secs_5h * 1000,
            "weekly_end_time": now_ms + secs_week * 1000,
        })
    }

    #[test]
    fn weekly_window_hours_away_reports_hours_not_a_day() {
        // The weekly window resets in six hours; it used to render as "1d"
        // because the arithmetic divided by 86400 and added one. The extra 30s
        // keeps the truncation to whole minutes off a boundary.
        let entry = entry_ending_in(2 * 3600 + 21 * 60 + 30, 6 * 3600 + 21 * 60 + 30);
        assert_eq!(reset_in(&entry, &["end_time"]), "2h 21m");
        assert_eq!(reset_in(&entry, &["weekly_end_time"]), "6h 21m");
    }

    #[test]
    fn a_window_more_than_a_day_out_still_reads_in_days() {
        let entry = entry_ending_in(90, 3 * 86400 + 4 * 3600 + 30);
        assert_eq!(reset_in(&entry, &["weekly_end_time"]), "3d 4h");
    }

    #[test]
    fn a_passed_or_missing_window_has_no_countdown() {
        let entry = entry_ending_in(-3600, -60);
        assert_eq!(reset_in(&entry, &["end_time"]), "");
        assert_eq!(reset_in(&serde_json::json!({}), &["end_time"]), "");
    }
}
