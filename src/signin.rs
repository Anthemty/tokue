// signin.rs — "Add account": each provider's own sign-in, run by OCG.
//
//   Codex        browser OAuth + PKCE, Codex CLI's client, callback on :1455
//   Claude       browser OAuth + PKCE, Claude Code's client, callback on a free port
//   Command Code Studio's CLI sign-in: the page POSTs a new API key to a local callback
//   OpenCode Go  device code, as `opencode console login` does — no callback at all
//
// One sign-in per provider at a time. Progress is kept per provider for the
// Preferences window; a device code is shown there too.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::accounts::{self, Saved};
use crate::codex_accounts::{self, b64url_encode, jwt_claims};

const TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Where a provider's sign-in stands: "waiting", "done", "failed" or
/// "canceled"; a line for the pane; and for a device code, the code to type
/// and the page to type it on.
#[derive(Clone, Default)]
pub struct Status {
    pub state: String,
    pub message: String,
    pub code: Option<String>,
    pub url: Option<String>,
}

static STATUS: LazyLock<Mutex<HashMap<String, Status>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static CANCELED: LazyLock<Mutex<HashMap<String, bool>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn statuses() -> HashMap<String, Status> {
    STATUS.lock().unwrap().clone()
}

pub fn cancel(provider: &str) {
    CANCELED.lock().unwrap().insert(provider.to_string(), true);
}

fn canceled(provider: &str) -> bool {
    CANCELED.lock().unwrap().get(provider).copied().unwrap_or(false)
}

/// Can `provider` add accounts by signing in here?
pub fn supported(provider: &str) -> bool {
    matches!(provider, "codex" | "claude" | "commandcode" | "opencode")
}

/// A running sign-in's handle on its own status.
struct Flow {
    provider: String,
    on_change: fn(),
}

impl Flow {
    fn progress(&self, message: &str, code: Option<String>, url: Option<String>) {
        STATUS.lock().unwrap().insert(
            self.provider.clone(),
            Status { state: "waiting".into(), message: message.into(), code, url },
        );
        (self.on_change)();
    }
    fn canceled(&self) -> bool {
        canceled(&self.provider)
    }
}

/// Begin a sign-in in the background. `on_change` runs whenever its status
/// moves; `on_added` once an account has been kept.
pub fn start(provider: &str, on_change: fn(), on_added: fn()) -> Result<(), String> {
    if !supported(provider) {
        return Err(format!("{} has no sign-in; it uses an API key", provider));
    }
    {
        let mut st = STATUS.lock().unwrap();
        if st.get(provider).map(|s| s.state == "waiting").unwrap_or(false) {
            return Err("a sign-in is already waiting".to_string());
        }
        st.insert(
            provider.to_string(),
            Status { state: "waiting".into(), message: "Opening the browser…".into(), ..Default::default() },
        );
    }
    CANCELED.lock().unwrap().insert(provider.to_string(), false);
    on_change();
    let flow = Flow { provider: provider.to_string(), on_change };
    std::thread::spawn(move || {
        let result = match flow.provider.as_str() {
            "codex" => codex(&flow),
            "claude" => claude(&flow),
            "commandcode" => commandcode(&flow),
            _ => opencode(&flow),
        };
        let (state, message) = match result {
            Ok(Some(m)) => ("done", m),
            Ok(None) => ("canceled", "Sign-in canceled".to_string()),
            Err(e) => ("failed", e),
        };
        STATUS.lock().unwrap().insert(
            flow.provider.clone(),
            Status { state: state.into(), message, ..Default::default() },
        );
        on_change();
        if state == "done" {
            on_added();
        }
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

const CODEX_AUTHORIZE: &str = "https://auth.openai.com/oauth/authorize";
const CODEX_REDIRECT: &str = "http://localhost:1455/auth/callback";
const CODEX_SCOPES: &str = "openid profile email offline_access api.connectors.read api.connectors.invoke";

fn codex_authorize_url(challenge: &str, state: &str) -> String {
    with_query(
        CODEX_AUTHORIZE,
        &[
            ("response_type", "code"),
            ("client_id", codex_accounts::CLIENT_ID),
            ("redirect_uri", CODEX_REDIRECT),
            ("scope", CODEX_SCOPES),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
            ("id_token_add_organizations", "true"),
            ("codex_cli_simplified_flow", "true"),
            ("state", state),
            ("originator", "codex_cli_rs"),
        ],
    )
}

/// OpenAI only sends Codex's client back to port 1455. Another sign-in
/// holding it — an earlier one, or a `codex login` left open — is asked to
/// stop first, as the CLI itself does.
fn codex(flow: &Flow) -> Result<Option<String>, String> {
    let listener = listen_on("127.0.0.1:1455", true)
        .map_err(|_| "port 1455, where ChatGPT sends the sign-in back, is busy — close any other Codex sign-in and try again".to_string())?;
    let (verifier, challenge) = pkce()?;
    let state = random_token(24)?;
    open(&codex_authorize_url(&challenge, &state))?;
    flow.progress("Waiting for the browser…", None, None);
    serve(&listener, flow, |req| match oauth_callback(&req, &["/auth/callback"], &state) {
        Callback::Code(code) => {
            let auth = codex_exchange(&code, &verifier)?;
            let (email, in_codex) = codex_accounts::add_login(auth)?;
            let note = if in_codex { " — Codex now uses it" } else { "" };
            Ok(Step::Done(format!("Added {}{}", email, note), done_page(&email)))
        }
        other => other.into_step(),
    })
}

fn codex_exchange(code: &str, verifier: &str) -> Result<serde_json::Value, String> {
    let tok = post_form(
        codex_accounts::TOKEN_URL,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", CODEX_REDIRECT),
            ("client_id", codex_accounts::CLIENT_ID),
            ("code_verifier", verifier),
        ],
    )?;
    codex_auth_blob(&tok)
}

/// auth.json exactly as `codex login` writes it.
fn codex_auth_blob(tok: &serde_json::Value) -> Result<serde_json::Value, String> {
    let field = |k: &str| tok.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let (id_token, access_token, refresh_token) = (field("id_token"), field("access_token"), field("refresh_token"));
    if id_token.is_empty() || access_token.is_empty() || refresh_token.is_empty() {
        return Err("ChatGPT sent back no tokens".to_string());
    }
    let account_id = jwt_claims(&id_token)
        .and_then(|c| {
            c.get("https://api.openai.com/auth")
                .and_then(|a| a.get("chatgpt_account_id"))
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .unwrap_or_default();
    Ok(serde_json::json!({
        "OPENAI_API_KEY": null,
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": id_token,
            "access_token": access_token,
            "refresh_token": refresh_token,
            "account_id": account_id,
        },
        "last_refresh": chrono::Utc::now().to_rfc3339(),
    }))
}

// ---------------------------------------------------------------------------
// Claude
// ---------------------------------------------------------------------------

pub const CLAUDE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const CLAUDE_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CLAUDE_AUTHORIZE: &str = "https://claude.com/cai/oauth/authorize";
/// What Claude Code asks for at /login.
const CLAUDE_SCOPES: &str =
    "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload user:plugins";

fn claude_authorize_url(redirect: &str, challenge: &str, state: &str) -> String {
    with_query(
        CLAUDE_AUTHORIZE,
        &[
            ("code", "true"),
            ("client_id", CLAUDE_CLIENT_ID),
            ("response_type", "code"),
            ("redirect_uri", redirect),
            ("scope", CLAUDE_SCOPES),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
            ("state", state),
        ],
    )
}

fn claude(flow: &Flow) -> Result<Option<String>, String> {
    let listener = listen_on("127.0.0.1:0", false)?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://localhost:{}/callback", port);
    let (verifier, challenge) = pkce()?;
    let state = random_token(24)?;
    open(&claude_authorize_url(&redirect, &challenge, &state))?;
    flow.progress("Waiting for the browser…", None, None);
    serve(&listener, flow, |req| match oauth_callback(&req, &["/callback"], &state) {
        Callback::Code(code) => {
            let saved = claude_exchange(&code, &verifier, &redirect, &state)?;
            let name = saved.name.clone();
            accounts::upsert("claude", saved)?;
            Ok(Step::Done(format!("Added {}", name), done_page(&name)))
        }
        other => other.into_step(),
    })
}

fn claude_exchange(code: &str, verifier: &str, redirect: &str, state: &str) -> Result<Saved, String> {
    let tok = post_json(
        CLAUDE_TOKEN_URL,
        &serde_json::json!({
            "grant_type": "authorization_code",
            "code": code,
            "redirect_uri": redirect,
            "client_id": CLAUDE_CLIENT_ID,
            "code_verifier": verifier,
            "state": state,
        }),
    )?;
    let access = str_at(&tok, "/access_token");
    let refresh = str_at(&tok, "/refresh_token");
    if access.is_empty() || refresh.is_empty() {
        return Err("Claude sent back no tokens".to_string());
    }
    let expires_in = tok.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
    let profile = crate::fetch_claude::profile(&access).ok();
    let email = profile
        .as_ref()
        .map(|p| str_at(p, "/account/email"))
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| str_at(&tok, "/account/email_address"));
    let uuid = profile
        .as_ref()
        .map(|p| str_at(p, "/account/uuid"))
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| str_at(&tok, "/account/uuid"));
    if email.is_empty() && uuid.is_empty() {
        return Err("Claude didn't say which account signed in".to_string());
    }
    let plan = profile
        .as_ref()
        .map(|p| crate::fetch_claude::plan_of_org_type(&str_at(p, "/organization/organization_type")))
        .unwrap_or_default();
    Ok(Saved {
        key: format!("claude:{}", if uuid.is_empty() { &email } else { &uuid }),
        name: email.clone(),
        plan,
        auth: serde_json::json!({
            "accessToken": access,
            "refreshToken": refresh,
            "expiresAt": chrono::Utc::now().timestamp_millis() + expires_in * 1000,
            "email": email,
        }),
        lapsed: None,
    })
}

// ---------------------------------------------------------------------------
// Command Code
// ---------------------------------------------------------------------------

const CMD_STUDIO: &str = "https://commandcode.ai";

/// Studio asks the user to approve a key for this machine, then posts it — a
/// form (the browser lands on our reply), or JSON from an older Studio — to
/// the callback, exactly as it does for `commandcode login`.
fn commandcode(flow: &Flow) -> Result<Option<String>, String> {
    let listener = listen_on("127.0.0.1:0", false)?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let state = random_token(24)?;
    let callback = format!("http://127.0.0.1:{}/callback", port);
    open(&with_query(
        &format!("{}/studio/auth/cli", CMD_STUDIO),
        &[("callback", callback.as_str()), ("state", state.as_str()), ("mode", "redirect")],
    ))?;
    flow.progress("Approve the key in the browser…", None, None);
    serve(&listener, flow, |req| {
        if req.method == "OPTIONS" {
            return Ok(Step::Reply(204, cors(&req), String::new()));
        }
        if req.path != "/callback" || req.method != "POST" {
            return Ok(Step::Reply(404, vec![], String::new()));
        }
        let json = req.header("content-type").to_ascii_lowercase().starts_with("application/json");
        let got: HashMap<String, String> = if json {
            serde_json::from_slice::<HashMap<String, serde_json::Value>>(&req.body)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
                .collect()
        } else {
            parse_query(&String::from_utf8_lossy(&req.body))
        };
        let field = |k: &str| got.get(k).cloned().unwrap_or_default();
        let answer = |ok: bool, msg: &str| -> Step {
            if json {
                let mut headers = cors(&req);
                headers.push(("Content-Type".into(), "application/json".into()));
                Step::Reply(if ok { 200 } else { 400 }, headers, serde_json::json!({"success": ok, "error": msg}).to_string())
            } else if ok {
                Step::Reply(200, vec![], page(true, "You're signed in", msg))
            } else {
                Step::Reply(200, vec![], page(false, "Sign-in didn't finish", msg))
            }
        };
        if field("state") != state {
            return Ok(Step::Reply(403, vec![], page(false, "This link isn't from this sign-in", "Start it again from tokue.")));
        }
        if !field("error").is_empty() {
            let msg = if field("error") == "access_denied" {
                "the sign-in was denied".to_string()
            } else {
                non_empty(field("error_description"), field("error"))
            };
            return Ok(Step::FailWith(msg.clone(), Box::new(answer(false, &msg))));
        }
        let key = field("apiKey");
        if key.is_empty() {
            return Ok(Step::Reply(400, vec![], "Command Code sent back no key".into()));
        }
        // Only Command Code turning the key down fails the sign-in: whoami has
        // answered a key made a moment ago with a 500, so any other error
        // falls back to the name Studio sent with it.
        let who = match crate::fetch_commandcode::whoami_name(&key) {
            Ok(w) if !w.is_empty() => w,
            Err(e) if e.starts_with("HTTP 401") || e.starts_with("HTTP 403") => {
                let msg = "Command Code didn't accept the new key".to_string();
                return Ok(Step::FailWith(msg.clone(), Box::new(answer(false, &msg))));
            }
            _ => non_empty(field("userName"), field("userId")),
        };
        if who.is_empty() {
            let msg = "Command Code didn't say which account signed in".to_string();
            return Ok(Step::FailWith(msg.clone(), Box::new(answer(false, &msg))));
        }
        let id = non_empty(field("userId"), who.clone());
        accounts::upsert(
            "commandcode",
            Saved {
                key: format!("commandcode:{}", id),
                name: who.clone(),
                plan: String::new(),
                auth: serde_json::json!({"apiKey": key, "userId": field("userId"), "userName": who}),
                lapsed: None,
            },
        )?;
        Ok(Step::FinishWith(
            format!("Added {}", who),
            Box::new(answer(true, &format!("{} is added to tokue. You can close this tab.", who))),
        ))
    })
}

fn cors(req: &Request) -> Vec<(String, String)> {
    let origin = match req.header("origin") {
        o if o == "https://commandcode.ai" || o == "https://staging.commandcode.ai" => o,
        _ => CMD_STUDIO.to_string(),
    };
    let mut h = vec![
        ("Access-Control-Allow-Origin".into(), origin),
        ("Access-Control-Allow-Methods".into(), "GET, POST, OPTIONS".into()),
        ("Access-Control-Allow-Headers".into(), "Content-Type".into()),
    ];
    if req.header("access-control-request-private-network") == "true" {
        h.push(("Access-Control-Allow-Private-Network".into(), "true".into()));
    }
    h
}

// ---------------------------------------------------------------------------
// OpenCode Go — device code
// ---------------------------------------------------------------------------

pub const OPENCODE_CONSOLE: &str = "https://opencode.ai/console";
pub const OPENCODE_CLIENT_ID: &str = "opencode-cli";

fn opencode(flow: &Flow) -> Result<Option<String>, String> {
    let start = post_json(
        &format!("{}/auth/device/code", OPENCODE_CONSOLE),
        &serde_json::json!({"client_id": OPENCODE_CLIENT_ID}),
    )?;
    let device_code = str_at(&start, "/device_code");
    let user_code = str_at(&start, "/user_code");
    let page_path = str_at(&start, "/verification_uri_complete");
    if device_code.is_empty() || user_code.is_empty() {
        return Err("OpenCode sent back no device code".to_string());
    }
    let page_url = if page_path.starts_with("http") {
        page_path
    } else {
        format!("https://opencode.ai{}", page_path)
    };
    let mut interval = start.get("interval").and_then(|v| v.as_u64()).unwrap_or(5).max(1);
    let expires = start.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(600);
    let deadline = Instant::now() + Duration::from_secs(expires);
    open(&page_url)?;
    flow.progress("Confirm this code in the browser", Some(user_code), Some(page_url));

    let tok = loop {
        let wait_until = Instant::now() + Duration::from_secs(interval);
        while Instant::now() < wait_until {
            if flow.canceled() {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if Instant::now() > deadline {
            return Err("the code expired before it was confirmed".to_string());
        }
        let r = post_json_any(
            &format!("{}/auth/device/token", OPENCODE_CONSOLE),
            &serde_json::json!({
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "device_code": device_code,
                "client_id": OPENCODE_CLIENT_ID,
            }),
        )?;
        if !str_at(&r, "/access_token").is_empty() {
            break r;
        }
        match str_at(&r, "/error").as_str() {
            "authorization_pending" => {}
            "slow_down" => interval += 5,
            "" => return Err("OpenCode sent back neither a token nor a reason".to_string()),
            other => return Err(format!("OpenCode refused the sign-in: {}", other)),
        }
    };
    let access = str_at(&tok, "/access_token");
    let refresh = str_at(&tok, "/refresh_token");
    let expires_in = tok.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
    let user = get_json(&format!("{}/api/user", OPENCODE_CONSOLE), &access, None)?;
    let orgs = get_json(&format!("{}/api/orgs", OPENCODE_CONSOLE), &access, None).unwrap_or_default();
    let mut orgs: Vec<&serde_json::Value> = orgs.as_array().map(|a| a.iter().collect()).unwrap_or_default();
    orgs.sort_by_key(|o| (str_at(o, "/name"), str_at(o, "/id")));
    let (org_id, org_name) = orgs.first().map(|o| (str_at(o, "/id"), str_at(o, "/name"))).unwrap_or_default();
    let (id, email) = (str_at(&user, "/id"), str_at(&user, "/email"));
    if id.is_empty() {
        return Err("OpenCode didn't say which account signed in".to_string());
    }
    accounts::upsert(
        "opencode",
        Saved {
            key: format!("opencode:{}", id),
            name: email.clone(),
            plan: String::new(),
            auth: serde_json::json!({
                "access": access,
                "refresh": refresh,
                "expires": chrono::Utc::now().timestamp_millis() + expires_in * 1000,
                "server": OPENCODE_CONSOLE,
                "orgID": org_id,
                "orgName": org_name,
            }),
            lapsed: None,
        },
    )?;
    Ok(Some(format!("Added {}", non_empty(email, id))))
}

// ---------------------------------------------------------------------------
// Local callback server
// ---------------------------------------------------------------------------

struct Request {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> String {
        self.headers.get(name).cloned().unwrap_or_default()
    }
}

/// What a callback does with one request.
enum Step {
    /// Answer and keep waiting for the real callback.
    Reply(u16, Vec<(String, String)>, String),
    /// The sign-in succeeded; show this page.
    Done(String, String),
    /// The sign-in succeeded with this message; answer with this reply.
    FinishWith(String, Box<Step>),
    /// The sign-in failed with this reason; answer with this reply.
    FailWith(String, Box<Step>),
}

fn listen_on(addr: &str, ask_holder_to_stop: bool) -> Result<TcpListener, String> {
    for attempt in 0..10 {
        if let Ok(l) = TcpListener::bind(addr) {
            l.set_nonblocking(true).map_err(|e| e.to_string())?;
            return Ok(l);
        }
        if !ask_holder_to_stop {
            break;
        }
        if attempt == 0 {
            if let Ok(mut s) = TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_secs(2)) {
                let _ = s.write_all(b"GET /cancel HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(format!("cannot listen on {}", addr))
}

/// Accept requests until the callback settles, the user cancels, or ten
/// minutes pass. Ok(None) when canceled.
fn serve(
    listener: &TcpListener,
    flow: &Flow,
    mut handle: impl FnMut(Request) -> Result<Step, String>,
) -> Result<Option<String>, String> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if flow.canceled() {
            return Ok(None);
        }
        if Instant::now() > deadline {
            return Err("no answer from the browser within 10 minutes".to_string());
        }
        let mut stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            Err(e) => return Err(e.to_string()),
        };
        let Some(req) = read_request(&mut stream) else { continue };
        if req.path == "/cancel" {
            respond(&mut stream, 204, &[], "");
            return Ok(None);
        }
        match handle(req) {
            Ok(step) => {
                if let Some(result) = finish(&mut stream, step) {
                    return result;
                }
            }
            Err(e) => {
                respond(&mut stream, 200, &[], &page(false, "Sign-in didn't finish", &e));
                return Err(e);
            }
        }
    }
}

/// Send a step's reply; Some when the sign-in is over.
fn finish(stream: &mut TcpStream, step: Step) -> Option<Result<Option<String>, String>> {
    match step {
        Step::Reply(code, headers, body) => {
            respond(stream, code, &headers, &body);
            None
        }
        Step::Done(message, html) => {
            respond(stream, 200, &[], &html);
            Some(Ok(Some(message)))
        }
        Step::FinishWith(message, reply) => {
            finish(stream, *reply);
            Some(Ok(Some(message)))
        }
        Step::FailWith(why, reply) => {
            finish(stream, *reply);
            Some(Err(why))
        }
    }
}

enum Callback {
    Code(String),
    Error(String),
    NotOurs,
    Elsewhere,
}

impl Callback {
    fn into_step(self) -> Result<Step, String> {
        match self {
            Callback::Code(_) => unreachable!("handled by the caller"),
            Callback::Error(why) => {
                let html = page(false, "Sign-in didn't finish", &why);
                Ok(Step::FailWith(why, Box::new(Step::Reply(200, vec![], html))))
            }
            // Someone else's link, or a stale tab: keep waiting for ours.
            Callback::NotOurs => Ok(Step::Reply(200, vec![], page(false, "This link isn't from this sign-in", "Start it again from tokue."))),
            Callback::Elsewhere => Ok(Step::Reply(404, vec![], String::new())),
        }
    }
}

/// Read an OAuth redirect: the code when it is ours.
fn oauth_callback(req: &Request, paths: &[&str], state: &str) -> Callback {
    if !paths.contains(&req.path.as_str()) {
        return Callback::Elsewhere;
    }
    if let Some(err) = req.query.get("error") {
        return Callback::Error(req.query.get("error_description").unwrap_or(err).clone());
    }
    let code = req.query.get("code").cloned().unwrap_or_default();
    if req.query.get("state").map(|s| s.as_str()) != Some(state) || code.is_empty() {
        return Callback::NotOurs;
    }
    Callback::Code(code)
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if buf.len() > 64 * 1024 {
            return None;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.lines();
    let first = lines.next()?;
    let mut parts = first.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let headers: HashMap<String, String> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let length: usize = headers.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0).min(64 * 1024);
    let mut body = buf[head_end..].to_vec();
    while body.len() < length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    body.truncate(length);
    let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
    Some(Request { method, path: path.to_string(), query: parse_query(query), headers, body })
}

fn respond(stream: &mut TcpStream, code: u16, headers: &[(String, String)], body: &str) {
    let reason = match code {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        _ => "Not Found",
    };
    let mut head = format!("HTTP/1.1 {} {}\r\nConnection: close\r\nContent-Length: {}\r\n", code, reason, body.len());
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
        head.push_str("Content-Type: text/html; charset=utf-8\r\n");
    }
    for (k, v) in headers {
        head.push_str(&format!("{}: {}\r\n", k, v));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

fn done_page(who: &str) -> String {
    page(true, "You're signed in", &format!("{} is added to tokue. You can close this tab.", who))
}

/// What the browser shows at the end — OCG's own page, not the vendor's
/// "return to the CLI".
fn page(ok: bool, title: &str, detail: &str) -> String {
    let (title, detail) = (crate::i18n::tr(title), crate::i18n::tr(detail));
    format!(
        "<!doctype html><meta charset=utf-8><title>tokue</title>\
         <body style=\"margin:0;height:100vh;display:grid;place-items:center;background:#161816;\
         color:#e6ebe6;font:15px ui-monospace,SFMono-Regular,Menlo,monospace\">\
         <div style=\"text-align:center\"><div style=\"font-size:28px;color:{}\">{}</div>\
         <h1 style=\"font-size:18px;margin:14px 0 6px\">{}</h1>\
         <p style=\"color:#8f9a8f;margin:0\">{}</p></div>",
        if ok { "#66e08f" } else { "#e8a33a" },
        if ok { "✓" } else { "!" },
        html_escape(&title),
        html_escape(&detail)
    )
}

// ---------------------------------------------------------------------------
// HTTP out, and small helpers
// ---------------------------------------------------------------------------

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("tokue/1.0")
        .build()
        .map_err(|e| e.to_string())
}

fn parse_reply(resp: reqwest::blocking::Response, require_ok: bool) -> Result<serde_json::Value, String> {
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if require_ok && !status.is_success() {
        return Err(format!("HTTP {} {}", status.as_u16(), text.chars().take(120).collect::<String>()));
    }
    serde_json::from_str(&text).map_err(|e| format!("unreadable reply: {}", e))
}

fn post_form(url: &str, form: &[(&str, &str)]) -> Result<serde_json::Value, String> {
    let body: Vec<String> = form.iter().map(|(k, v)| format!("{}={}", k, pct_encode(v))).collect();
    let resp = client()?
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body.join("&"))
        .send()
        .map_err(|e| format!("token exchange: {}", e))?;
    parse_reply(resp, true)
}

pub fn post_json(url: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let resp = client()?
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body.to_string())
        .send()
        .map_err(|e| e.to_string())?;
    parse_reply(resp, true)
}

/// A JSON reply whatever the status — device-code polling answers "still
/// pending" with a 400.
fn post_json_any(url: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let resp = client()?
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body.to_string())
        .send()
        .map_err(|e| e.to_string())?;
    parse_reply(resp, false)
}

pub fn get_json(url: &str, bearer: &str, org: Option<&str>) -> Result<serde_json::Value, String> {
    let mut req = client()?.get(url).header("Authorization", format!("Bearer {}", bearer)).header("Accept", "application/json");
    if let Some(o) = org.filter(|o| !o.is_empty()) {
        req = req.header("x-org-id", o);
    }
    parse_reply(req.send().map_err(|e| e.to_string())?, true)
}

fn open(url: &str) -> Result<(), String> {
    // Debug aid: exercise a sign-in without a browser; the page it would
    // have opened is logged instead.
    if std::env::var("TOKUE_SIGNIN_NO_BROWSER").is_ok() {
        eprintln!("[signin] would open {}", url);
        return Ok(());
    }
    std::process::Command::new("/usr/bin/open")
        .arg(url)
        .status()
        .map(|_| ())
        .map_err(|e| format!("cannot open the browser: {}", e))
}

fn pkce() -> Result<(String, String), String> {
    let verifier = random_token(32)?;
    let challenge = b64url_encode(ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes()).as_ref());
    Ok((verifier, challenge))
}

fn random_token(bytes: usize) -> Result<String, String> {
    use ring::rand::SecureRandom;
    let mut buf = vec![0u8; bytes];
    ring::rand::SystemRandom::new().fill(&mut buf).map_err(|_| "no secure randomness".to_string())?;
    Ok(b64url_encode(&buf))
}

fn with_query(base: &str, params: &[(&str, &str)]) -> String {
    let q: Vec<String> = params.iter().map(|(k, v)| format!("{}={}", k, pct_encode(v))).collect();
    format!("{}?{}", base, q.join("&"))
}

fn str_at(v: &serde_json::Value, pointer: &str) -> String {
    v.pointer(pointer).and_then(|x| x.as_str()).unwrap_or_default().to_string()
}

fn non_empty(a: String, b: String) -> String {
    if a.is_empty() {
        b
    } else {
        a
    }
}

fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn pct_decode(s: &str) -> String {
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(hi), Some(lo)) => {
                    out.push(hi << 4 | lo);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(q: &str) -> HashMap<String, String> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (pct_decode(k), pct_decode(v))
        })
        .collect()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_matches_the_rfc_example() {
        // RFC 7636 appendix B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = b64url_encode(ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes()).as_ref());
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn the_authorize_urls_carry_what_the_clis_send() {
        let q = parse_query(codex_authorize_url("CHAL", "STATE").split_once('?').unwrap().1);
        assert_eq!(q["client_id"], codex_accounts::CLIENT_ID);
        assert_eq!(q["redirect_uri"], "http://localhost:1455/auth/callback");
        assert_eq!(q["code_challenge_method"], "S256");
        let url = claude_authorize_url("http://localhost:5000/callback", "CHAL", "STATE");
        assert!(url.starts_with("https://claude.com/cai/oauth/authorize?"));
        let q = parse_query(url.split_once('?').unwrap().1);
        assert_eq!(q["client_id"], CLAUDE_CLIENT_ID);
        assert_eq!(q["scope"], CLAUDE_SCOPES);
        assert_eq!(q["code"], "true");
    }

    #[test]
    fn query_values_are_decoded() {
        let q = parse_query("code=a%2Fb%3D&state=x+y&e=%E4%BD%A0%");
        assert_eq!(q["code"], "a/b=");
        assert_eq!(q["state"], "x y");
        assert_eq!(q["e"], "你%");
    }

    /// Send one raw request over a real socket and read it as the server does.
    fn over_socket(raw: &str) -> Request {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let raw = raw.to_string();
        let client = std::thread::spawn(move || {
            let mut c = TcpStream::connect(addr).unwrap();
            c.write_all(raw.as_bytes()).unwrap();
        });
        let (mut s, _) = listener.accept().unwrap();
        let req = read_request(&mut s).unwrap();
        client.join().unwrap();
        req
    }

    #[test]
    fn requests_are_read_with_their_bodies() {
        let body = "apiKey=cmd_x&state=S&userName=me";
        let req = over_socket(&format!(
            "POST /callback HTTP/1.1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nOrigin: https://commandcode.ai\r\n\r\n{}",
            body.len(),
            body
        ));
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/callback");
        assert_eq!(req.header("origin"), "https://commandcode.ai");
        assert_eq!(parse_query(&String::from_utf8_lossy(&req.body))["apiKey"], "cmd_x");
    }

    #[test]
    fn an_oauth_redirect_is_ours_only_with_our_state_and_a_code() {
        let req = |q: &str| over_socket(&format!("GET /callback?{} HTTP/1.1\r\n\r\n", q));
        assert!(matches!(oauth_callback(&req("code=c&state=S"), &["/callback"], "S"), Callback::Code(c) if c == "c"));
        assert!(matches!(oauth_callback(&req("code=c&state=OTHER"), &["/callback"], "S"), Callback::NotOurs));
        assert!(matches!(oauth_callback(&req("state=S"), &["/callback"], "S"), Callback::NotOurs));
        assert!(matches!(
            oauth_callback(&req("error=access_denied&error_description=user%20said%20no&state=S"), &["/callback"], "S"),
            Callback::Error(e) if e == "user said no"
        ));
        let other = over_socket("GET /favicon.ico HTTP/1.1\r\n\r\n");
        assert!(matches!(oauth_callback(&other, &["/callback"], "S"), Callback::Elsewhere));
    }

    #[test]
    fn a_codex_login_is_kept_in_auth_json_shape() {
        let id = b64url_encode(br#"{"https://api.openai.com/auth":{"chatgpt_account_id":"ws-1"}}"#);
        let tok = serde_json::json!({"id_token": format!("h.{}.s", id), "access_token": "a", "refresh_token": "r"});
        let auth = codex_auth_blob(&tok).unwrap();
        assert_eq!(auth["auth_mode"], "chatgpt");
        assert_eq!(auth["tokens"]["account_id"], "ws-1");
        assert!(codex_auth_blob(&serde_json::json!({"access_token": "a"})).is_err());
    }
}
