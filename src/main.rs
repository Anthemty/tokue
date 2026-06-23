// main.rs — entry point, FFI callbacks, and threading.
//
// The main thread enters the NSApplication run loop (blocks forever). A
// background thread fetches all providers every 15 minutes and pushes state
// to the UI via the FFI mutators (which dispatch_async to the main queue).

mod config;
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
    // runApp blocks on the NSApplication run loop for the lifetime of the app.
    // The main thread is implicitly pinned (Rust's main thread is the process
    // main thread, which AppKit requires).
    ffi::run_app();
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
                let r = match p {
                    "opencode" => fetch_opencode::fetch(&cfg),
                    "deepseek" => fetch_deepseek::fetch(&cfg),
                    "minimax" => fetch_minimax::fetch(&cfg),
                    _ => ProviderFetchResult::err(format!("unknown provider: {}", p)),
                };
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

/// Recompute icon/tooltip/state and forward them to the UI.
fn push_ui_state() {
    let max_crit = {
        let cache = state::PROVIDER_CACHE.read().unwrap();
        let mut m = 0;
        for &id in PROVIDERS {
            if let Some(cached) = cache.get(id) {
                if cached.err.is_none() && cached.criticality > m {
                    m = cached.criticality;
                }
            }
        }
        m
    };

    let icon = icon::usage_icon_bytes(max_crit);
    ffi::set_status_icon(&icon);
    ffi::set_status_tooltip(&format!("Usage Monitor — worst: {}%", max_crit));

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
    thread::spawn(|| {
        refresh_once();
        loop {
            thread::sleep(Duration::from_secs(15 * 60));
            refresh_once();
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

#[no_mangle]
pub extern "C" fn goQuitRequested() {
    // Termination is handled AppKit-side; placeholder for future teardown.
}

/// Convert a borrowed C string to a Rust String. Returns "" on null.
unsafe fn cstr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
}
