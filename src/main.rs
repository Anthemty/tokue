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
mod store;

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

    if target == "stats" {
        print_stats();
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

/// `ocg --once stats`: what the SQLite history holds and what each account has
/// burned since local midnight.
fn print_stats() {
    let since = fetch_codex::start_of_today();
    let mut accounts: Vec<(String, f64)> = store::today_by_account(since).into_iter().collect();
    accounts.sort_by(|a, b| a.0.cmp(&b.0));
    let payload = serde_json::json!({
        "db": store::location().map(|p| p.to_string_lossy().to_string()),
        "summary": store::summary(),
        "since_local_midnight": since,
        "today": accounts
            .into_iter()
            .map(|(home, consumed)| serde_json::json!({
                "home": home,
                "consumed_percent_of_5h_window": (consumed * 10.0).round() / 10.0,
            }))
            .collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()));
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

/// Badge text: quota left when the panel reads remaining, otherwise quota used.
/// Severity is always the used side, so the bad-case number is 0% left / 100% used.
fn badge_percent(max_severity: i32, show_remaining: bool) -> i32 {
    let severity = max_severity.clamp(0, 100);
    if show_remaining {
        100 - severity
    } else {
        severity
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

    let cfg = config::load();
    // The badge reads the same way as the panel: quota left when the meters show
    // remaining, otherwise quota used. Colour always means the same thing —
    // under 10% left is red, under 30% is amber.
    let show_remaining = cfg.codex.show_remaining;
    let badge = badge_percent(max_crit, show_remaining);
    let wording = if show_remaining { "least remaining" } else { "worst used" };

    let icon = icon::usage_icon_bytes(badge);
    ffi::set_status_icon(&icon);
    ffi::set_status_title(&format!("{}%", badge), max_crit);
    let tooltip = if summary.is_empty() {
        format!("Usage Monitor — {}: {}%", wording, badge)
    } else {
        format!("Usage Monitor — {}: {}%\n\n{}", wording, badge, summary)
    };
    ffi::set_status_tooltip(&tooltip);

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
            Some(settings) => {
                cfg.codex.accounts = settings.accounts;
                cfg.codex.show_spend = settings.show_spend;
                cfg.codex.show_remaining = settings.show_remaining;
                cfg.codex.show_today = settings.show_today;
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
struct CodexSettings {
    accounts: Vec<config::CodexAccount>,
    show_spend: bool,
    show_remaining: bool,
    show_today: bool,
}

fn parse_codex_settings(json: &str) -> Option<CodexSettings> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let flag = |map: &serde_json::Map<String, serde_json::Value>, key: &str, default: bool| {
        json_bool(map.get(key), default)
    };
    match &value {
        serde_json::Value::Array(_) => Some(CodexSettings {
            accounts: parse_accounts(&value)?,
            show_spend: false,
            show_remaining: true,
            show_today: false,
        }),
        serde_json::Value::Object(map) => Some(CodexSettings {
            accounts: parse_accounts(map.get("accounts")?)?,
            show_spend: flag(map, "show_spend", false),
            show_remaining: flag(map, "show_remaining", true),
            show_today: flag(map, "show_today", false),
        }),
        _ => None,
    }
}

/// JSON booleans or 0/1 numbers: Foundation's JSON writer emits numbers for
/// boxed BOOLs, so a strict `as_bool()` silently flipped disabled accounts back on.
fn json_bool(value: Option<&serde_json::Value>, default: bool) -> bool {
    match value {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::Number(n)) => n.as_i64().map(|i| i != 0).unwrap_or(default),
        Some(serde_json::Value::String(s)) => match s.as_str() {
            "true" | "1" => true,
            "false" | "0" => false,
            _ => default,
        },
        _ => default,
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
            enabled: json_bool(entry.get("enabled"), true),
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
    fn badge_follows_the_panel_direction() {
        // Nothing left: 0% in remaining mode, 100% in used mode.
        assert_eq!(badge_percent(100, true), 0);
        assert_eq!(badge_percent(100, false), 100);
        // 85% used == 15% left.
        assert_eq!(badge_percent(85, true), 15);
        assert_eq!(badge_percent(85, false), 85);
        // Out-of-range providers must not produce nonsense.
        assert_eq!(badge_percent(130, true), 0);
        assert_eq!(badge_percent(-5, false), 0);
    }

    #[test]
    fn parses_account_list_payload() {
        let settings = parse_codex_settings(
            r#"[{"home":"~/.codex","label":"main","enabled":true},
                {"home":"~/.codex2","label":"","enabled":false},
                {"home":"~/.codex","label":"dupe"},
                {"label":"no home"}]"#,
        )
        .unwrap();
        let accounts = settings.accounts;
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].home, "~/.codex");
        assert_eq!(accounts[0].label, "main");
        assert!(accounts[0].enabled);
        assert!(!accounts[1].enabled);
        assert!(!settings.show_spend);
        assert!(settings.show_remaining, "remaining is the default reading");
        assert!(!settings.show_today, "the today row is opt-in");
    }

    #[test]
    fn parses_codex_settings_object() {
        let settings = parse_codex_settings(
            r#"{"accounts":[{"home":"~/.codex","label":"","enabled":true}],
                "show_spend":true,"show_remaining":false,"show_today":true}"#,
        )
        .unwrap();
        assert_eq!(settings.accounts.len(), 1);
        assert!(settings.show_spend);
        assert!(!settings.show_remaining);
        assert!(settings.show_today);
    }

    #[test]
    fn accepts_foundation_style_numeric_booleans() {
        // NSJSONSerialization writes @(NO) as 0, which used to re-enable accounts.
        let settings = parse_codex_settings(
            r#"{"accounts":[{"home":"~/.codex","label":"","enabled":0},
                            {"home":"~/.codex2","label":"","enabled":1}],
                "show_spend":0,"show_remaining":1,"show_today":0}"#,
        )
        .unwrap();
        assert!(!settings.accounts[0].enabled);
        assert!(settings.accounts[1].enabled);
        assert!(!settings.show_spend);
        assert!(settings.show_remaining);
        assert!(!settings.show_today);
    }

    #[test]
    fn codex_settings_without_mode_key_defaults_to_remaining() {
        let settings = parse_codex_settings(r#"{"accounts":[],"show_spend":true}"#).unwrap();
        assert!(settings.show_remaining);
        assert!(!settings.show_today);
    }

    #[test]
    fn rejects_malformed_account_list() {
        assert!(parse_codex_settings("not json").is_none());
        assert!(parse_codex_settings("{}").is_none());
        assert!(parse_codex_settings(r#"{"accounts":[]}"#).unwrap().accounts.is_empty());
        assert!(parse_codex_settings("[]").unwrap().accounts.is_empty());
    }
}
