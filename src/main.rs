// main.rs — entry point, FFI callbacks, and threading.
//
// The main thread enters the NSApplication run loop (blocks forever). A
// background thread fetches all providers on an interval (15 minutes by
// default, backed off after failed cycles) and pushes state to the UI via the
// FFI mutators (which dispatch_async to the main queue).
//
// `ocg --once [provider|all]` runs one fetch cycle headlessly and prints JSON,
// which is how the ChatGPT account data is verified without the UI.

mod codex_accounts;
mod config;
mod fetch_codex;
mod fetch_deepseek;
mod fetch_minimax;
mod fetch_opencode;
mod ffi;
mod icon;
mod panel_state;
mod providers;
mod state;

use std::ffi::CStr;
use std::os::raw::c_char;
use std::thread;
use std::time::Duration;

use chrono::Local;

use providers::{ProviderFetchResult, PROVIDERS};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--once") {
        run_once_cli(&args);
        return;
    }
    // runApp blocks on the NSApplication run loop for the lifetime of the app.
    // The main thread is implicitly pinned (Rust's main thread is the process
    // main thread, which AppKit requires).
    ffi::run_app();
}

/// Headless single cycle: `ocg --once codex` / `--once deepseek` / `--once all`.
fn run_once_cli(args: &[String]) {
    let cfg = config::load();
    let target = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "codex".to_string());

    if target == "codex" {
        println!("{}", fetch_codex::debug_json(&cfg));
        return;
    }

    if target == "all" {
        fetch_all_providers();
        println!("{}", panel_state::build_json(&cfg));
        return;
    }

    let result = match target.as_str() {
        "opencode" => fetch_opencode::fetch(&cfg),
        "deepseek" => fetch_deepseek::fetch(&cfg),
        "minimax" => fetch_minimax::fetch(&cfg),
        other => {
            eprintln!("unknown provider: {}", other);
            std::process::exit(2);
        }
    };
    let payload = serde_json::json!({
        "provider": target,
        "criticality": result.criticality,
        "error": result.err,
        "meters": result.meters.iter().map(|m| serde_json::json!({
            "group": m.group,
            "label": m.label,
            "percent": m.percent,
            "detail": m.detail,
        })).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&payload).unwrap_or_default());
}

// ---------- background refresh ----------

/// Fetch all providers in parallel, update the cache, then push UI state.
/// Called from a background thread; serialised by REFRESH_MU.
fn refresh_once() {
    let _guard = state::REFRESH_MU.lock().unwrap();
    fetch_all_providers();
    push_ui_state();
}

/// Spawn one thread per provider, collect results into the cache.
fn fetch_all_providers() {
    let cfg = config::load();
    let handles: Vec<_> = PROVIDERS
        .iter()
        .map(|&p| {
            let cfg = cfg.clone();
            thread::spawn(move || {
                let r = fetch_provider(p, &cfg);
                (p.to_string(), r)
            })
        })
        .collect();

    let mut results: Vec<(String, ProviderFetchResult)> = Vec::with_capacity(handles.len());
    for h in handles {
        match h.join() {
            Ok(r) => results.push(r),
            Err(_) => continue,
        }
    }

    state::record_cycle(
        results.iter().filter(|(_, r)| r.err.is_some()).count(),
        results.len(),
    );

    {
        let mut cache = state::PROVIDER_CACHE.write().unwrap();
        for (name, r) in results {
            cache.insert(name, r);
        }
    }
    {
        let mut lu = state::LAST_UPDATED.write().unwrap();
        *lu = Some(Local::now());
    }
}

fn fetch_provider(name: &str, cfg: &config::Config) -> ProviderFetchResult {
    match name {
        "opencode" => fetch_opencode::fetch(cfg),
        "deepseek" => fetch_deepseek::fetch(cfg),
        "minimax" => fetch_minimax::fetch(cfg),
        "codex" => fetch_codex::fetch(cfg),
        other => ProviderFetchResult::err(format!("unknown provider: {}", other)),
    }
}

/// Minutes until the next cycle: configured interval, doubled while cycles fail.
fn next_interval_minutes() -> u32 {
    let cfg = config::load();
    let base = cfg
        .codex
        .refresh_minutes
        .or(cfg.refresh_minutes)
        .unwrap_or(15)
        .clamp(1, 240);
    match state::consecutive_failed_cycles() {
        0 => base,
        1 => (base * 2).min(60),
        _ => (base * 4).min(120),
    }
}

/// Recompute icon/title/tooltip/state and forward them to the UI.
fn push_ui_state() {
    let (max_crit, summary) = {
        let cache = state::PROVIDER_CACHE.read().unwrap();
        let mut m = 0;
        let mut summaries: Vec<String> = Vec::new();
        for &id in PROVIDERS {
            if let Some(cached) = cache.get(id) {
                if cached.err.is_none() && cached.criticality > m {
                    m = cached.criticality;
                }
                if let Some(text) = &cached.summary {
                    if !text.is_empty() {
                        summaries.push(format!("{}:\n{}", providers::label(id), text));
                    }
                }
            }
        }
        (m, summaries.join("\n\n"))
    };

    let icon = icon::usage_icon_bytes(max_crit);
    ffi::set_status_icon(&icon);
    ffi::set_status_title(&format!("{}%", max_crit));
    let tooltip = if summary.is_empty() {
        format!("Usage Monitor — worst: {}%", max_crit)
    } else {
        format!("Usage Monitor — worst: {}%\n\n{}", max_crit, summary)
    };
    ffi::set_status_tooltip(&tooltip);

    let cfg = config::load();
    let state_json = panel_state::build_json(&cfg);
    ffi::update_panel_state(&state_json);
}

/// Push just a neutral icon + loading tooltip (before first fetch).
fn push_icon_only() {
    let icon = icon::neutral_icon_bytes();
    ffi::set_status_icon(&icon);
    ffi::set_status_tooltip("Usage Monitor — loading");
}

/// Change the active provider and re-push state (no fetch).
fn switch_provider(name: &str) {
    let mut cfg = config::load();
    if cfg.active_provider == name {
        return;
    }
    cfg.active_provider = name.to_string();
    let _ = config::save(&mut cfg);
    push_ui_state();
}

// ---------- FFI callbacks (invoked from Obj-C on the main queue) ----------

#[no_mangle]
pub extern "C" fn goOnReady() {
    push_icon_only();
    thread::spawn(|| loop {
        refresh_once();
        thread::sleep(Duration::from_secs(next_interval_minutes() as u64 * 60));
    });
}

#[no_mangle]
pub extern "C" fn goProviderSelected(provider_id: *const c_char) {
    let id = unsafe { cstr_to_string(provider_id) };
    switch_provider(&id);
}

#[no_mangle]
pub extern "C" fn goRefreshRequested() {
    thread::spawn(|| refresh_once());
}

#[no_mangle]
pub extern "C" fn goSaveCredentials(
    provider: *const c_char,
    field: *const c_char,
    value: *const c_char,
) {
    let p = unsafe { cstr_to_string(provider) };
    let f = unsafe { cstr_to_string(field) };
    let mut v = unsafe { cstr_to_string(value) };
    thread::spawn(move || {
        let mut cfg = config::load();
        let matched = match (p.as_str(), f.as_str()) {
            ("opencode", "workspace_id") => {
                cfg.opencode.workspace_id = v;
                true
            }
            ("opencode", "auth_cookie") => {
                if !v.is_empty() && !v.starts_with("auth=") {
                    v = format!("auth={}", v);
                }
                cfg.opencode.auth_cookie = v;
                true
            }
            ("deepseek", "api_key") => {
                cfg.deepseek.api_key = v;
                true
            }
            ("minimax", "api_key") => {
                cfg.minimax.api_key = v;
                true
            }
            _ => false,
        };
        if matched {
            let _ = config::save(&mut cfg);
            refresh_once();
        }
    });
}

/// Save the Codex settings ({"accounts":[{home,label,enabled}],"show_spend":bool}).
#[no_mangle]
pub extern "C" fn goSaveCodexAccounts(json: *const c_char) {
    let payload = unsafe { cstr_to_string(json) };
    thread::spawn(move || {
        let mut cfg = config::load();
        match parse_codex_settings(&payload) {
            Some((accounts, show_spend, show_remaining)) => {
                cfg.codex.accounts = accounts;
                cfg.codex.show_spend = show_spend;
                cfg.codex.show_remaining = show_remaining;
                let _ = config::save(&mut cfg);
                refresh_once();
            }
            None => eprintln!("ocg: ignoring malformed codex settings payload"),
        }
    });
}

/// Re-scan ~/.codex* and merge any newly discovered home into the saved list.
#[no_mangle]
pub extern "C" fn goRescanCodexAccounts() {
    thread::spawn(|| {
        let mut cfg = config::load();
        merge_discovered_accounts(&mut cfg);
        let _ = config::save(&mut cfg);
        refresh_once();
    });
}

#[no_mangle]
pub extern "C" fn goQuitRequested() {
    // Termination is handled AppKit-side; placeholder for future teardown.
}

/// Parse the Codex settings JSON sent by the settings view. Accepts the current
/// object form ({"accounts": …, "show_spend": …}) and the older bare array.
fn parse_codex_settings(json: &str) -> Option<(Vec<config::CodexAccount>, bool, bool)> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    match &value {
        serde_json::Value::Array(_) => Some((parse_accounts(&value)?, false, true)),
        serde_json::Value::Object(map) => {
            let accounts = parse_accounts(map.get("accounts")?)?;
            let show_spend = map.get("show_spend").and_then(|v| v.as_bool()).unwrap_or(false);
            let show_remaining = map
                .get("show_remaining")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            Some((accounts, show_spend, show_remaining))
        }
        _ => None,
    }
}

fn parse_accounts(value: &serde_json::Value) -> Option<Vec<config::CodexAccount>> {
    let array = value.as_array()?;
    let mut out: Vec<config::CodexAccount> = Vec::with_capacity(array.len());
    for entry in array {
        let home = entry.get("home").and_then(|h| h.as_str()).unwrap_or_default();
        if home.is_empty() {
            continue;
        }
        // Guard against duplicates in the payload.
        if out.iter().any(|a| a.home == home) {
            continue;
        }
        out.push(config::CodexAccount {
            home: home.to_string(),
            label: entry.get("label").and_then(|l| l.as_str()).unwrap_or_default().to_string(),
            enabled: entry.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true),
        });
    }
    Some(out)
}

/// Keep saved accounts (and their labels/enabled flags), appending any home
/// discovered on disk that the config does not know about yet.
fn merge_discovered_accounts(cfg: &mut config::Config) {
    let known: Vec<String> = cfg
        .codex
        .accounts
        .iter()
        .map(|a| codex_accounts::expand_home(&a.home))
        .collect();
    for path in codex_accounts::discover_homes() {
        let home = path.to_string_lossy().to_string();
        if known.contains(&home) {
            continue;
        }
        cfg.codex.accounts.push(config::CodexAccount {
            home,
            label: String::new(),
            enabled: true,
        });
    }
}

/// Convert a borrowed C string to a Rust String. Returns "" on null.
unsafe fn cstr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_account_list_payload() {
        let (accounts, show_spend, show_remaining) = parse_codex_settings(
            r#"[{"home":"~/.codex","label":"main","enabled":true},
                {"home":"~/.codex2","label":"","enabled":false},
                {"home":"~/.codex","label":"dupe"},
                {"label":"no home"}]"#,
        )
        .unwrap();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].home, "~/.codex");
        assert_eq!(accounts[0].label, "main");
        assert!(accounts[0].enabled);
        assert!(!accounts[1].enabled);
        assert!(!show_spend);
        assert!(show_remaining, "remaining is the default reading");
    }

    #[test]
    fn parses_codex_settings_object() {
        let (accounts, show_spend, show_remaining) = parse_codex_settings(
            r#"{"accounts":[{"home":"~/.codex","label":"","enabled":true}],
                "show_spend":true,"show_remaining":false}"#,
        )
        .unwrap();
        assert_eq!(accounts.len(), 1);
        assert!(show_spend);
        assert!(!show_remaining);
    }

    #[test]
    fn codex_settings_without_mode_key_defaults_to_remaining() {
        let (_, _, show_remaining) =
            parse_codex_settings(r#"{"accounts":[],"show_spend":true}"#).unwrap();
        assert!(show_remaining);
    }

    #[test]
    fn rejects_malformed_account_list() {
        assert!(parse_codex_settings("not json").is_none());
        assert!(parse_codex_settings("{}").is_none());
        assert!(parse_codex_settings(r#"{"accounts":[]}"#).unwrap().0.is_empty());
        assert!(parse_codex_settings("[]").unwrap().0.is_empty());
    }
}
