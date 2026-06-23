// panel_state.rs — build the JSON state string pushed to the Obj-C UI.
//
// Faithful port of app_darwin.go's buildPanelStateJSON. The wire format is
// consumed by app_darwin.m; field names must match the Go json tags exactly.

use serde::Serialize;

use crate::config::Config;
use crate::providers::{PROVIDERS, label};
use crate::state::{updated_at_string, PROVIDER_CACHE};

#[derive(Serialize)]
struct PanelState {
    active: String,
    updated_at: String,
    worst: i32,
    providers: Vec<PanelProvider>,
    results: std::collections::BTreeMap<String, PanelResult>,
    credentials: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

#[derive(Serialize)]
struct PanelProvider {
    id: String,
    label: String,
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
}

pub fn build_json(cfg: &Config) -> String {
    let mut state = PanelState {
        active: cfg.active_provider.clone(),
        updated_at: updated_at_string(),
        worst: 0,
        providers: Vec::with_capacity(PROVIDERS.len()),
        results: std::collections::BTreeMap::new(),
        credentials: std::collections::BTreeMap::new(),
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

    serde_json::to_string(&state).unwrap_or_else(|_| {
        r#"{"active":"","providers":[],"results":{},"credentials":{}}"#.to_string()
    })
}
