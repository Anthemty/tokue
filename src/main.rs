// main.rs — entry point, FFI callbacks, and threading.
//
// The main thread enters the NSApplication run loop (blocks forever). A
// background thread fetches all providers on an interval (15 minutes by
// default, backed off after failed cycles) and pushes state to the UI via the
// FFI mutators (which dispatch_async to the main queue).
//
// `ocg --once [provider|all]` runs one fetch cycle headlessly and prints JSON,
// which is how the ChatGPT account data is verified without the UI.

mod accounts;
mod codex_accounts;
mod config;
mod fetch_claude;
mod fetch_codex;
mod fetch_commandcode;
mod fetch_deepseek;
mod fetch_minimax;
mod fetch_opencode;
mod ffi;
mod i18n;
mod panel_state;
mod providers;
mod signin;
mod state;
mod store;

use std::ffi::CStr;
use std::os::raw::c_char;
use std::thread;
use std::time::Duration;

use chrono::Local;

use providers::{ProviderFetchResult, PROVIDERS};

fn main() {
    config::migrate_from_ocg();
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
        "commandcode" => fetch_commandcode::fetch(&cfg),
        "claude" => fetch_claude::fetch(&cfg),
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
            .map(|(account, consumed)| serde_json::json!({
                "account": account,
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
    {
        // Disabled providers leave the panel entirely: no cache, no badge, no rows.
        let mut cache = state::PROVIDER_CACHE.write().unwrap();
        for &p in PROVIDERS {
            if !cfg.provider_enabled(p) {
                cache.remove(p);
            }
        }
    }
    let handles: Vec<_> = PROVIDERS
        .iter()
        .filter(|&&p| cfg.provider_enabled(p))
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
        "commandcode" => fetch_commandcode::fetch(cfg),
        "claude" => fetch_claude::fetch(cfg),
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
/// The value set in Preferences (`refresh_minutes`) wins over the older
/// config-only Codex override, so what the UI shows is what runs.
fn next_interval_minutes() -> u32 {
    let cfg = config::load();
    let base = cfg.effective_refresh_minutes();
    match state::consecutive_failed_cycles() {
        0 => base,
        1 => (base * 2).min(60),
        _ => (base * 4).min(120),
    }
}

/// Recompute icon/title/tooltip/state and forward them to the UI.
/// Among the meters that belong to one card (`meter.key == key`, e.g. one
/// Codex account), the same window-priority rule as everywhere else: prefer
/// the 5h/rolling reading over weekly over monthly, rather than whichever is
/// worst. `None` when nothing in `meters` carries that key.
fn severity_for_key(meters: &[providers::UsageMeter], key: &str) -> Option<i32> {
    let matching: Vec<&providers::UsageMeter> =
        meters.iter().filter(|m| m.key.as_deref() == Some(key)).collect();
    if matching.is_empty() {
        return None;
    }
    matching
        .iter()
        .find(|m| m.label.starts_with("5h") || m.label.starts_with("Rolling"))
        .or_else(|| matching.iter().find(|m| m.label.starts_with("Weekly")))
        .or_else(|| matching.first())
        .and_then(|m| m.severity)
}

fn push_ui_state() {
    let cfg = config::load();
    i18n::apply(&cfg.language);

    // The badge reflects whichever provider is currently selected in the
    // popover, not the worst across all of them — switching tabs changes
    // what the menu bar shows. Same disabled-provider fallback as
    // panel_state::build_json, so the two stay in sync.
    let mut active_id = cfg.active_provider.clone();
    if !cfg.provider_enabled(&active_id) {
        if let Some(first) = PROVIDERS.iter().copied().find(|id| cfg.provider_enabled(id)) {
            active_id = first.to_string();
        }
    }

    let (active_crit, active_ok, summary) = {
        let cache = state::PROVIDER_CACHE.read().unwrap();
        let mut summaries: Vec<String> = Vec::new();
        for &id in PROVIDERS {
            if let Some(cached) = cache.get(id) {
                if let Some(text) = &cached.summary {
                    if !text.is_empty() {
                        summaries.push(format!("{}:\n{}", providers::label(id), text));
                    }
                }
            }
        }
        match cache.get(active_id.as_str()) {
            Some(cached) => {
                // A pinned card that is there but has no numbers (signed out,
                // unreadable) shows as "no data" — falling back to the
                // provider's worst would show another account's figure under
                // the pinned one's name.
                let mut pinned_has_data = true;
                let crit = match cfg.selected_key(&active_id) {
                    Some(key) => match severity_for_key(&cached.meters, key) {
                        Some(sev) => sev,
                        None if cached.meters.iter().any(|m| m.key.as_deref() == Some(key)) => {
                            pinned_has_data = false;
                            0
                        }
                        None => cached.criticality,
                    },
                    None => cached.criticality,
                };
                (crit, cached.err.is_none() && pinned_has_data, summaries.join("\n\n"))
            }
            None => (0, false, summaries.join("\n\n")),
        }
    };

    let active_label = providers::label(&active_id);
    if !active_ok {
        // Not configured, errored, or no successful fetch yet for the
        // selected provider — an empty ring, not a falsely-healthy "100%
        // left" green one.
        ffi::set_status_gauge(0, 0, false);
        ffi::set_status_tooltip(&tooltip(active_label, "no data", &summary));
    } else {
        // The badge reads the same way as the panel: quota left when the
        // meters show remaining, otherwise quota used. Colour always means
        // the same thing — under 10% left is red, under 30% is amber.
        let show_remaining = cfg.show_remaining();
        let badge = badge_percent(active_crit, show_remaining);
        let wording = if show_remaining { "left" } else { "used" };

        ffi::set_status_gauge(badge, active_crit, true);
        ffi::set_status_tooltip(&tooltip(active_label, &format!("{}% {}", badge, wording), &summary));
    }

    let state_json = panel_state::build_json(&cfg);
    ffi::update_panel_state(&state_json);
}

/// "tokue — Codex: 42% left", then every provider's breakdown.
fn tooltip(provider: &str, reading: &str, summary: &str) -> String {
    let head = format!("tokue — {}: {}", provider, i18n::tr(reading));
    if summary.is_empty() {
        head
    } else {
        format!("{}\n\n{}", head, i18n::tr_lines(summary))
    }
}

/// Push just a neutral icon + loading tooltip (before first fetch).
fn push_icon_only() {
    i18n::apply(&config::load().language);
    ffi::set_status_gauge(0, 0, false);
    ffi::set_status_tooltip(&format!("tokue — {}", i18n::tr("loading")));
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
        migrate_codex_homes_once();
        refresh_once();
        // Sleep in short ticks and re-read the interval each time, so a change
        // made in Preferences takes effect within half a minute instead of
        // after whatever the old interval was.
        let started = std::time::Instant::now();
        loop {
            thread::sleep(Duration::from_secs(30));
            if started.elapsed() >= Duration::from_secs(next_interval_minutes() as u64 * 60) {
                break;
            }
        }
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
    let v = unsafe { cstr_to_string(value) };
    thread::spawn(move || {
        let mut cfg = config::load();
        let matched = match (p.as_str(), f.as_str()) {
            ("opencode", "api_key") => {
                cfg.opencode.api_key = v;
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

/// Save the per-provider enable switches ({"opencode":true,…}) and refetch.
#[no_mangle]
pub extern "C" fn goSaveProviderEnabled(json: *const c_char) {
    let payload = unsafe { cstr_to_string(json) };
    thread::spawn(move || {
        let mut cfg = config::load();
        let value: serde_json::Value = match serde_json::from_str(&payload) {
            Ok(v) => v,
            Err(_) => return,
        };
        if let Some(map) = value.as_object() {
            for (id, on) in map {
                let enabled = on.as_bool().unwrap_or(true);
                cfg.set_provider_enabled(id, enabled);
                if !enabled && cfg.active_provider == *id {
                    cfg.active_provider = cfg
                        .first_enabled_provider()
                        .unwrap_or(crate::providers::OPENCODE)
                        .to_string();
                }
            }
            let _ = config::save(&mut cfg);
            refresh_once();
        }
    });
}

/// Save the Codex settings ({"accounts":[{key,label,enabled}],"show_spend":bool}).
#[no_mangle]
pub extern "C" fn goSaveCodexAccounts(json: *const c_char) {
    let payload = unsafe { cstr_to_string(json) };
    thread::spawn(move || {
        let mut cfg = config::load();
        match parse_codex_settings(&payload) {
            Some(settings) => {
                if let Some(accounts) = settings.accounts {
                    cfg.codex.accounts = accounts;
                }
                cfg.codex.show_spend = settings.show_spend;
                cfg.codex.show_remaining = settings.show_remaining;
                cfg.codex.show_today = settings.show_today;
                cfg.codex.show_reset_credits = settings.show_reset_credits;
                let _ = config::save(&mut cfg);
                refresh_once();
            }
            None => eprintln!("tokue: ignoring malformed codex settings payload"),
        }
    });
}

/// Once, on the first launch with the account store: move ~/.codex2… into it.
fn migrate_codex_homes_once() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let mut cfg = config::load();
        if codex_accounts::migrate_homes(&mut cfg) {
            let _ = config::save(&mut cfg);
        }
    });
}

/// "Add account": run the provider's own sign-in from here. Progress shows in
/// Preferences; a kept account is fetched straight away.
#[no_mangle]
pub extern "C" fn goSignIn(provider: *const c_char) {
    let provider = unsafe { cstr_to_string(provider) };
    if let Err(e) = signin::start(&provider, push_ui_state, || {
        thread::spawn(refresh_once);
    }) {
        eprintln!("tokue: {}", e);
    }
}

#[no_mangle]
pub extern "C" fn goCancelSignIn(provider: *const c_char) {
    signin::cancel(&unsafe { cstr_to_string(provider) });
}

/// An account's name and whether it is shown in the panel.
#[no_mangle]
pub extern "C" fn goSetAccount(provider: *const c_char, key: *const c_char, label: *const c_char, shown: bool) {
    let (provider, key, label) = unsafe { (cstr_to_string(provider), cstr_to_string(key), cstr_to_string(label)) };
    thread::spawn(move || {
        let mut cfg = config::load();
        cfg.set_account(&provider, &key, label.trim(), shown);
        let _ = config::save(&mut cfg);
        refresh_once();
    });
}

/// "Use in Codex": sign Codex in to a saved account; its previous account is
/// saved in its place.
#[no_mangle]
pub extern "C" fn goCodexUseAccount(key: *const c_char) {
    let key = unsafe { cstr_to_string(key) };
    thread::spawn(move || {
        match codex_accounts::use_in_codex(&key) {
            Ok(()) => {
                // Running Codex sessions keep the account they started with,
                // which is easy to mistake for the switch not having worked.
                state::set_card_notice(
                    &key,
                    Some("Codex now uses this account — restart running Codex sessions to switch them"),
                );
                refresh_once();
                thread::sleep(Duration::from_secs(10));
                state::set_card_notice(&key, None);
                push_ui_state();
            }
            Err(e) => {
                state::set_card_notice(&key, Some(&format!("Could not switch: {}", e)));
                refresh_once();
            }
        }
    });
}

/// Forget an account OCG keeps, with its display settings and any pin on it.
/// A CLI's own login is left alone: that is signed out with the CLI.
#[no_mangle]
pub extern "C" fn goRemoveAccount(provider: *const c_char, key: *const c_char) {
    let (provider, key) = unsafe { (cstr_to_string(provider), cstr_to_string(key)) };
    thread::spawn(move || {
        let removed = if provider == "codex" {
            codex_accounts::remove(&key).is_ok()
        } else {
            accounts::remove(&provider, &key)
        };
        if removed {
            let mut cfg = config::load();
            cfg.forget_account(&provider, &key);
            let _ = config::save(&mut cfg);
        }
        refresh_once();
    });
}

/// Pin (or, passed an empty key, un-pin) which card drives `provider`'s menu
/// bar badge — clicking an account card in a multi-account provider. Nothing
/// needs re-fetching: the cache already has every account's data, so this
/// just recomputes and re-pushes the already-cached numbers under the new
/// selection.
#[no_mangle]
pub extern "C" fn goSetSelectedAccount(provider: *const c_char, key: *const c_char) {
    let p = unsafe { cstr_to_string(provider) };
    let k = unsafe { cstr_to_string(key) };
    thread::spawn(move || {
        let mut cfg = config::load();
        cfg.set_selected_key(&p, if k.is_empty() { None } else { Some(&k) });
        let _ = config::save(&mut cfg);
        push_ui_state();
    });
}

/// Start a provider's 5h window now — the "start window" button on a card.
/// `key` is the card's meter key (a CODEX_HOME for Codex; the fixed card key
/// for the single-login providers). Feedback rides on the card notice:
/// "starting…" while the request runs, the failure reason if it fails, and
/// nothing on success — the refresh that follows shows the fresh countdown,
/// which is the real confirmation.
#[no_mangle]
pub extern "C" fn goStartWindow(provider: *const c_char, key: *const c_char) {
    let provider = unsafe { cstr_to_string(provider) };
    let key = unsafe { cstr_to_string(key) };
    if key.is_empty() {
        return;
    }
    thread::spawn(move || {
        state::set_card_notice(&key, Some("Starting the 5h window…"));
        push_ui_state();
        let result = match provider.as_str() {
            "codex" => fetch_codex::start_window(&key),
            "minimax" => fetch_minimax::start_window(&config::load()),
            "commandcode" => fetch_commandcode::start_window(),
            "claude" => fetch_claude::start_window(),
            "opencode" => fetch_opencode::start_window(&config::load(), &key),
            other => Err(format!("no start action for {}", other)),
        };
        match result {
            Ok(()) => state::set_card_notice(&key, None),
            Err(e) => state::set_card_notice(&key, Some(&format!("Could not start window: {}", e))),
        }
        refresh_once();
    });
}

/// Refresh interval from Preferences, in minutes (clamped to 1–240).
#[no_mangle]
pub extern "C" fn goSaveRefreshMinutes(minutes: u32) {
    thread::spawn(move || {
        let mut cfg = config::load();
        cfg.refresh_minutes = Some(minutes.clamp(1, 240));
        let _ = config::save(&mut cfg);
        push_ui_state();
    });
}

/// The system's first preferred language, from AppKit before goOnReady.
#[no_mangle]
pub extern "C" fn goSetSystemLanguage(tag: *const c_char) {
    i18n::set_system_language(&unsafe { cstr_to_string(tag) });
}

/// Language from Preferences: "" (follow the system), "en" or "zh-Hans".
/// Everything is translated on the way out, so no refetch is needed.
#[no_mangle]
pub extern "C" fn goSaveLanguage(setting: *const c_char) {
    let setting = unsafe { cstr_to_string(setting) };
    thread::spawn(move || {
        let mut cfg = config::load();
        cfg.language = match setting.as_str() {
            i18n::ENGLISH | i18n::CHINESE => setting,
            _ => String::new(),
        };
        let _ = config::save(&mut cfg);
        push_ui_state();
    });
}

#[no_mangle]
pub extern "C" fn goQuitRequested() {
    // Termination is handled AppKit-side; placeholder for future teardown.
}

/// Parse the Codex settings JSON sent by the settings view. Accepts the current
/// object form ({"accounts": …, "show_spend": …}) and the older bare array.
struct CodexSettings {
    /// Absent when only the display switches were sent.
    accounts: Option<Vec<config::CodexAccount>>,
    show_spend: bool,
    show_remaining: bool,
    show_today: bool,
    show_reset_credits: bool,
}

fn parse_codex_settings(json: &str) -> Option<CodexSettings> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let flag = |map: &serde_json::Map<String, serde_json::Value>, key: &str, default: bool| {
        json_bool(map.get(key), default)
    };
    match &value {
        serde_json::Value::Array(_) => Some(CodexSettings {
            accounts: Some(parse_accounts(&value)?),
            show_spend: false,
            show_remaining: true,
            show_today: false,
            show_reset_credits: false,
        }),
        serde_json::Value::Object(map) => Some(CodexSettings {
            accounts: match map.get("accounts") {
                Some(a) => Some(parse_accounts(a)?),
                None => None,
            },
            show_spend: flag(map, "show_spend", false),
            show_remaining: flag(map, "show_remaining", true),
            show_today: flag(map, "show_today", false),
            show_reset_credits: flag(map, "show_reset_credits", false),
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
        let key = entry.get("key").and_then(|h| h.as_str()).unwrap_or_default();
        if key.is_empty() {
            continue;
        }
        // Guard against duplicates in the payload.
        if out.iter().any(|a| a.key == key) {
            continue;
        }
        out.push(config::CodexAccount {
            key: key.to_string(),
            label: entry.get("label").and_then(|l| l.as_str()).unwrap_or_default().to_string(),
            enabled: json_bool(entry.get("enabled"), true),
        });
    }
    Some(out)
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
            r#"[{"key":"codex:a","label":"main","enabled":true},
                {"key":"codex:b","label":"","enabled":false},
                {"key":"codex:a","label":"dupe"},
                {"label":"no key"}]"#,
        )
        .unwrap();
        let accounts = settings.accounts.unwrap();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].key, "codex:a");
        assert_eq!(accounts[0].label, "main");
        assert!(accounts[0].enabled);
        assert!(!accounts[1].enabled);
        assert!(!settings.show_spend);
        assert!(settings.show_remaining, "remaining is the default reading");
        assert!(!settings.show_today, "the today row is opt-in");
        assert!(!settings.show_reset_credits, "reset credits are opt-in");
    }

    #[test]
    fn parses_codex_settings_object() {
        let settings = parse_codex_settings(
            r#"{"accounts":[{"key":"codex:a","label":"","enabled":true}],
                "show_spend":true,"show_remaining":false,"show_today":true}"#,
        )
        .unwrap();
        assert_eq!(settings.accounts.unwrap().len(), 1);
        assert!(settings.show_spend);
        assert!(!settings.show_remaining);
        assert!(settings.show_today);
    }

    #[test]
    fn accepts_foundation_style_numeric_booleans() {
        // NSJSONSerialization writes @(NO) as 0, which used to re-enable accounts.
        let settings = parse_codex_settings(
            r#"{"accounts":[{"key":"codex:a","label":"","enabled":0},
                            {"key":"codex:b","label":"","enabled":1}],
                "show_spend":0,"show_remaining":1,"show_today":0,
                "show_reset_credits":1}"#,
        )
        .unwrap();
        let accounts = settings.accounts.clone().unwrap();
        assert!(!accounts[0].enabled);
        assert!(accounts[1].enabled);
        assert!(!settings.show_spend);
        assert!(settings.show_remaining);
        assert!(!settings.show_today);
        assert!(settings.show_reset_credits, "numeric 1 means on");
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
        assert!(parse_codex_settings(r#"{"accounts":"nope"}"#).is_none());
        assert!(parse_codex_settings(r#"{"accounts":[]}"#).unwrap().accounts.unwrap().is_empty());
        assert!(parse_codex_settings("[]").unwrap().accounts.unwrap().is_empty());
        assert!(parse_codex_settings(r#"{"show_spend":true}"#).unwrap().accounts.is_none(), "display switches alone keep the accounts");
    }
}
