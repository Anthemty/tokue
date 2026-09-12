// panel_state.rs — build the JSON state string pushed to the Obj-C UI.
//
// Faithful port of app_darwin.go's buildPanelStateJSON. The wire format is
// consumed by app_darwin.m; field names must match the Go json tags exactly.

use serde::Serialize;

use crate::codex_accounts;
use crate::config::Config;
use crate::providers::{label, PROVIDERS};
use crate::state::{updated_at_string, PROVIDER_CACHE};

#[derive(Serialize)]
struct PanelState {
    active: String,
    updated_at: String,
    worst: i32,
    providers: Vec<PanelProvider>,
    results: std::collections::BTreeMap<String, PanelResult>,
    credentials: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    codex_accounts: Vec<PanelCodexAccount>,
    codex_show_spend: bool,
    codex_show_remaining: bool,
}

#[derive(Serialize)]
struct PanelProvider {
    id: String,
    label: String,
}

/// One row in the Codex account settings list.
#[derive(Serialize)]
struct PanelCodexAccount {
    home: String,
    home_display: String,
    label: String,
    email: String,
    plan: String,
    enabled: bool,
    /// Set when another home holds the same ChatGPT account.
    #[serde(skip_serializing_if = "Option::is_none")]
    duplicate_of: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct PanelResult {
    criticality: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    meters: Option<Vec<PanelMeter>>,
}

#[derive(Serialize)]
struct PanelMeter {
    label: String,
    percent: i32,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<String>,
}

pub fn build_json(cfg: &Config) -> String {
    let mut state = PanelState {
        active: cfg.active_provider.clone(),
        updated_at: updated_at_string(),
        worst: 0,
        providers: Vec::with_capacity(PROVIDERS.len()),
        results: std::collections::BTreeMap::new(),
        credentials: std::collections::BTreeMap::new(),
        codex_accounts: Vec::new(),
        codex_show_spend: cfg.codex.show_spend,
        codex_show_remaining: cfg.codex.show_remaining,
    };

    for &id in PROVIDERS {
        state.providers.push(PanelProvider {
            id: id.to_string(),
            label: label(id).to_string(),
        });
    }

    // Snapshot cache under read lock.
    let cache = PROVIDER_CACHE.read().unwrap();
    let mut max_crit: i32 = 0;

    for &id in PROVIDERS {
        let result = match cache.get(id) {
            None => PanelResult {
                criticality: 0,
                error: Some("not fetched".to_string()),
                meters: None,
            },
            Some(r) => {
                if r.criticality > max_crit && r.err.is_none() {
                    max_crit = r.criticality;
                }
                if let Some(err) = &r.err {
                    PanelResult {
                        criticality: r.criticality,
                        error: Some(err.clone()),
                        meters: None,
                    }
                } else {
                    let meters: Vec<PanelMeter> = r
                        .meters
                        .iter()
                        .map(|m| PanelMeter {
                            label: m.label.clone(),
                            percent: m.percent,
                            detail: m.detail.clone(),
                            group: m.group.clone(),
                        })
                        .collect();
                    PanelResult {
                        criticality: r.criticality,
                        error: None,
                        meters: Some(meters),
                    }
                }
            }
        };
        state.results.insert(id.to_string(), result);
    }
    drop(cache);

    state.worst = max_crit;

    // Credentials snapshot for the settings form (prefill).
    let mut oc = std::collections::BTreeMap::new();
    oc.insert("workspace_id".to_string(), cfg.opencode.workspace_id.clone());
    oc.insert("auth_cookie".to_string(), cfg.opencode.auth_cookie.clone());
    state.credentials.insert("opencode".to_string(), oc);

    let mut ds = std::collections::BTreeMap::new();
    ds.insert("api_key".to_string(), cfg.deepseek.api_key.clone());
    state.credentials.insert("deepseek".to_string(), ds);

    let mut mx = std::collections::BTreeMap::new();
    mx.insert("api_key".to_string(), cfg.minimax.api_key.clone());
    state.credentials.insert("minimax".to_string(), mx);

    // Codex settings list: every configured (or discovered) home with the
    // identity we can read offline, so the panel can label rows before any
    // network call.
    let specs = codex_accounts::effective_accounts(cfg);
    let mut identities = codex_accounts::load_enabled(cfg);
    // load_enabled drops disabled rows; reload them so they can be re-enabled.
    let enabled_homes: Vec<String> = identities.iter().map(|a| a.home.clone()).collect();
    for spec in specs.iter().filter(|s| !s.enabled) {
        let expanded = codex_accounts::expand_home(&spec.home);
        if !enabled_homes.contains(&expanded) {
            identities.push(codex_accounts::load(&spec.home, &spec.label));
        }
    }

    let mut seen: Vec<(String, String)> = Vec::new();
    for acct in &identities {
        let mut duplicate_of = acct.duplicate_of.clone();
        if duplicate_of.is_none() && !acct.account_id.is_empty() {
            if let Some((_, first)) = seen.iter().find(|(id, _)| *id == acct.account_id) {
                duplicate_of = Some(first.clone());
            } else {
                seen.push((acct.account_id.clone(), acct.home_display.clone()));
            }
        }
        let enabled = specs
            .iter()
            .find(|s| codex_accounts::expand_home(&s.home) == acct.home)
            .map(|s| s.enabled)
            .unwrap_or(true);
        state.codex_accounts.push(PanelCodexAccount {
            home: acct.home.clone(),
            home_display: acct.home_display.clone(),
            label: acct.label.clone(),
            email: if acct.email.is_empty() { String::new() } else { acct.email.clone() },
            plan: if acct.plan.is_empty() {
                String::new()
            } else {
                codex_accounts::plan_display(&acct.plan)
            },
            enabled,
            duplicate_of,
            error: acct.problem.clone(),
        });
    }

    serde_json::to_string(&state).unwrap_or_else(|_| {
        r#"{"active":"","providers":[],"results":{},"credentials":{}}"#.to_string()
    })
}
