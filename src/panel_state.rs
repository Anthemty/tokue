// panel_state.rs — build the JSON state string pushed to the Obj-C UI.
//
// Faithful port of app_darwin.go's buildPanelStateJSON. The wire format is
// consumed by app_darwin.m; field names must match the Go json tags exactly.

use serde::Serialize;

use crate::codex_accounts;
use crate::config::Config;
use crate::i18n::tr;
use crate::providers::{label, PROVIDERS};
use crate::state::{updated_at_string, CARD_NOTICES, PROVIDER_CACHE};

#[derive(Serialize)]
struct PanelState {
    active: String,
    /// The language in effect ("en" / "zh-Hans"); every string below is in it.
    language: String,
    /// The Preferences setting behind it: "" follows the system.
    language_setting: String,
    updated_at: String,
    worst: i32,
    providers: Vec<PanelProvider>,
    results: std::collections::BTreeMap<String, PanelResult>,
    credentials: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    /// Per provider: every account, for Preferences.
    accounts: std::collections::BTreeMap<String, Vec<PanelAccount>>,
    /// Per provider: where an "Add account" sign-in stands.
    signins: std::collections::BTreeMap<String, PanelSignIn>,
    /// Providers that can add accounts by signing in (the rest take a key).
    signin_providers: Vec<String>,
    codex_show_spend: bool,
    codex_show_remaining: bool,
    codex_show_today: bool,
    codex_show_reset_credits: bool,
    /// Background refresh interval, minutes — prefills the Preferences field.
    refresh_minutes: u32,
    /// The card (a meter's `key`) currently pinned to drive the active
    /// provider's menu bar badge, if any — so the popover can highlight it.
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_key: Option<String>,
    /// Transient per-card feedback (meter key -> text) for actions started
    /// from a card, e.g. "Starting 5h window…".
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    notices: std::collections::BTreeMap<String, String>,
}

#[derive(Serialize)]
struct PanelProvider {
    id: String,
    label: String,
    enabled: bool,
}

/// One account row in Preferences.
#[derive(Serialize)]
struct PanelAccount {
    key: String,
    /// The user's own name for it, or "".
    label: String,
    /// Email or user name.
    name: String,
    plan: String,
    enabled: bool,
    /// Where it lives: "cli" (the provider's CLI's own login, read only),
    /// "saved" (signed in from tokue), or "key" (an API key from Preferences).
    source: String,
    /// Codex only: the account the Codex CLI is signed in to.
    active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Where an "Add account" sign-in stands.
#[derive(Serialize)]
struct PanelSignIn {
    state: String,
    message: String,
    /// A device code to type in the browser (OpenCode).
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    badge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    can_start: bool,
    #[serde(skip_serializing_if = "is_false")]
    informational: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}

pub fn build_json(cfg: &Config) -> String {
    crate::i18n::apply(&cfg.language);
    let mut state = PanelState {
        active: cfg.active_provider.clone(),
        language: crate::i18n::current().to_string(),
        language_setting: cfg.language.clone(),
        updated_at: updated_at_string(),
        worst: 0,
        providers: Vec::with_capacity(PROVIDERS.len()),
        results: std::collections::BTreeMap::new(),
        credentials: std::collections::BTreeMap::new(),
        accounts: std::collections::BTreeMap::new(),
        signins: crate::signin::statuses()
            .into_iter()
            .map(|(p, st)| (p, PanelSignIn { state: st.state, message: tr(&st.message), code: st.code, url: st.url }))
            .collect(),
        signin_providers: PROVIDERS.iter().filter(|p| crate::signin::supported(p)).map(|p| p.to_string()).collect(),
        codex_show_spend: cfg.codex.show_spend,
        codex_show_remaining: cfg.codex.show_remaining,
        codex_show_today: cfg.codex.show_today,
        codex_show_reset_credits: cfg.codex.show_reset_credits,
        refresh_minutes: cfg.effective_refresh_minutes(),
        selected_key: None,
        notices: CARD_NOTICES.read().unwrap().iter().map(|(k, v)| (k.clone(), tr(v))).collect(),
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
    state.selected_key = cfg.selected_key(&state.active).map(|s| s.to_string());

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
                        error: Some(tr(err)),
                        meters: None,
                    }
                } else {
                    let meters: Vec<PanelMeter> = r
                        .meters
                        .iter()
                        .map(|m| PanelMeter {
                            label: tr(&m.label),
                            percent: m.percent,
                            detail: tr(&m.detail),
                            group: m.group.as_deref().map(tr),
                            key: m.key.clone(),
                            severity: m.severity,
                            badge: m.badge.clone(),
                            tag: m.tag.as_deref().map(tr),
                            can_start: m.can_start,
                            informational: m.informational,
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
    oc.insert("api_key".to_string(), cfg.opencode.api_key.clone());
    state.credentials.insert("opencode".to_string(), oc);

    let mut ds = std::collections::BTreeMap::new();
    ds.insert("api_key".to_string(), cfg.deepseek.api_key.clone());
    state.credentials.insert("deepseek".to_string(), ds);

    let mut mx = std::collections::BTreeMap::new();
    mx.insert("api_key".to_string(), cfg.minimax.api_key.clone());
    state.credentials.insert("minimax".to_string(), mx);

    // Account lists. Codex's own account first, then the saved ones in the
    // order they were added; likewise a CLI's own login before OCG's.
    let codex: Vec<PanelAccount> = codex_accounts::list(cfg)
        .into_iter()
        .map(|(acct, enabled)| PanelAccount {
            key: acct.key.clone(),
            label: acct.label.clone(),
            name: acct.email.clone(),
            plan: codex_accounts::plan_display(&acct.plan),
            enabled,
            source: if acct.active { "cli" } else { "saved" }.to_string(),
            active: acct.active,
            error: acct.problem.as_deref().map(tr),
        })
        .collect();
    state.accounts.insert("codex".into(), codex);
    let others: [(&str, Vec<crate::accounts::Listed>); 3] = [
        ("claude", crate::fetch_claude::listed()),
        ("commandcode", crate::fetch_commandcode::listed()),
        ("opencode", crate::fetch_opencode::listed(cfg)),
    ];
    for (provider, list) in others {
        let rows = list
            .into_iter()
            .map(|a| PanelAccount {
                label: cfg.account_label(provider, &a.key),
                enabled: cfg.account_shown(provider, &a.key),
                source: if a.own {
                    "cli"
                } else if a.key == crate::fetch_opencode::KEY_ACCOUNT {
                    "key"
                } else {
                    "saved"
                }
                .to_string(),
                active: false,
                error: a.problem.as_deref().map(tr),
                key: a.key,
                name: a.name,
                plan: a.plan,
            })
            .collect();
        state.accounts.insert(provider.to_string(), rows);
    }

    serde_json::to_string(&state).unwrap_or_else(|_| {
        r#"{"active":"","providers":[],"results":{},"credentials":{}}"#.to_string()
    })
}
