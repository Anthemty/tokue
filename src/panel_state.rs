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
    codex_show_today: bool,
    codex_show_reset_credits: bool,
}

#[derive(Serialize)]
struct PanelProvider {
    id: String,
    label: String,
    enabled: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    severity: Option<i32>,
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
        codex_show_today: cfg.codex.show_today,
        codex_show_reset_credits: cfg.codex.show_reset_credits,
    };

    for &id in PROVIDERS {
        state.providers.push(PanelProvider {
            id: id.to_string(),
            label: label(id).to_string(),
            enabled: cfg.provider_enabled(id),
        });
    }
    // A disabled provider must not stay selected.
    if !cfg.provider_enabled(&state.active) {
        if let Some(first) = PROVIDERS.iter().copied().find(|id| cfg.provider_enabled(id)) {
            state.active = first.to_string();
        }
    }

    // Snapshot cache under read lock.
    let cache = PROVIDER_CACHE.read().unwrap();
    let mut max_crit: i32 = 0;

    for &id in PROVIDERS {
        let result = match cache.get(id) {
            None => PanelResult {
                criticality: 0,
                error: (!cfg.provider_enabled(id)).then(|| ()).and(None),
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
                            key: m.key.clone(),
                            severity: m.severity,
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

    // Codex settings list: every configured (or discovered) home, in config
    // order — disabled rows stay where they are instead of sinking to the end
    // (which used to reshuffle the saved list on the next save).
    let specs = codex_accounts::effective_accounts(cfg);
    let identities: Vec<codex_accounts::AccountIdentity> = specs
        .iter()
        .map(|spec| codex_accounts::load(&spec.home, &spec.label))
        .collect();

    let mut seen: Vec<(String, String)> = Vec::new();
    for (acct, spec) in identities.iter().zip(specs.iter()) {
        let mut duplicate_of = acct.duplicate_of.clone();
        if duplicate_of.is_none() && !acct.account_id.is_empty() {
            if let Some((_, first)) = seen.iter().find(|(id, _)| *id == acct.account_id) {
                duplicate_of = Some(first.clone());
            } else {
                seen.push((acct.account_id.clone(), acct.home_display.clone()));
            }
        }
        let enabled = spec.enabled;
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
