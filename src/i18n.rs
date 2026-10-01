// i18n.rs — English or Simplified Chinese for everything the UI is sent.
//
// Every string is built in English, as the fetchers have always built them,
// and translated once on the way out: the panel state, the menu bar tooltip,
// the sign-in pages. Nothing inside compares translated text, so the rest of
// the code (and its tests, and the English snapshots kept in SQLite) never
// has to know which language is showing — and switching it needs no refetch.
//
// A rule is an English template and its Chinese one. In the English template
// `{}` captures text that is itself translated, `{=}` captures text kept as is
// (names, emails, raw server messages) and `{#}` captures a number; the Chinese
// template places them back as `{0}`, `{1}`… Composite strings that match no
// rule are split on " · ", " — " and "; " and their parts translated alone.

use std::sync::atomic::{AtomicBool, Ordering};

static ZH: AtomicBool = AtomicBool::new(false);
static SYSTEM_ZH: AtomicBool = AtomicBool::new(false);

/// The setting values Preferences offers; "" follows the system.
pub const ENGLISH: &str = "en";
pub const CHINESE: &str = "zh-Hans";

/// Record the system's first preferred language (from AppKit at launch).
pub fn set_system_language(tag: &str) {
    SYSTEM_ZH.store(is_simplified_chinese(tag), Ordering::Relaxed);
}

fn is_simplified_chinese(tag: &str) -> bool {
    let t = tag.to_ascii_lowercase();
    // zh-Hans, zh-Hans-CN, and the bare region forms of the mainland and Singapore.
    t.starts_with("zh-hans") || t == "zh" || t == "zh-cn" || t == "zh-sg"
}

/// Follow a language setting: "en", "zh-Hans", or anything else for the
/// system's. `TOKUE_LANG` overrides it, for snapshots and `--once`.
pub fn apply(setting: &str) {
    let forced = std::env::var("TOKUE_LANG").unwrap_or_default();
    let setting = if forced.is_empty() { setting } else { forced.as_str() };
    let zh = match setting {
        ENGLISH => false,
        CHINESE => true,
        _ => SYSTEM_ZH.load(Ordering::Relaxed),
    };
    ZH.store(zh, Ordering::Relaxed);
}

pub fn is_chinese() -> bool {
    ZH.load(Ordering::Relaxed)
}

/// The language in effect, as the UI is told it.
pub fn current() -> &'static str {
    if is_chinese() { CHINESE } else { ENGLISH }
}

/// Translate one UI string into the current language.
pub fn tr(s: &str) -> String {
    if is_chinese() { to_chinese(s) } else { s.to_string() }
}

/// The tooltip: line by line, and its account lines column by column.
pub fn tr_lines(s: &str) -> String {
    if !is_chinese() {
        return s.to_string();
    }
    s.lines()
        .map(|line| line.split("  ").map(to_chinese).collect::<Vec<_>>().join("  "))
        .collect::<Vec<_>>()
        .join("\n")
}

const SPLITS: &[(&str, &str)] = &[(" · ", " · "), (" — ", "——"), ("; ", "；")];

fn to_chinese(s: &str) -> String {
    if s.trim().is_empty() {
        return s.to_string();
    }
    for (en, zh) in EXACT {
        if *en == s {
            return zh.to_string();
        }
    }
    for (en, zh) in PATTERNS {
        if let Some(caps) = capture(en, s) {
            return fill(zh, &caps);
        }
    }
    for (sep, joined) in SPLITS {
        if s.contains(sep) {
            return s.split(sep).map(to_chinese).collect::<Vec<_>>().join(joined);
        }
    }
    s.to_string()
}

#[derive(Clone, Copy, PartialEq)]
enum Hole {
    Translate,
    Keep,
    Number,
}

/// Match `s` against an English template; the captures, already rendered.
fn capture(template: &str, s: &str) -> Option<Vec<String>> {
    // Split the template into literal pieces and the holes between them.
    let mut literals: Vec<&str> = Vec::new();
    let mut holes: Vec<Hole> = Vec::new();
    let mut rest = template;
    loop {
        let next = ["{}", "{=}", "{#}"]
            .iter()
            .filter_map(|h| rest.find(h).map(|i| (i, *h)))
            .min_by_key(|(i, _)| *i);
        match next {
            Some((i, h)) => {
                literals.push(&rest[..i]);
                holes.push(match h {
                    "{}" => Hole::Translate,
                    "{=}" => Hole::Keep,
                    _ => Hole::Number,
                });
                rest = &rest[i + h.len()..];
            }
            None => {
                literals.push(rest);
                break;
            }
        }
    }
    if holes.is_empty() {
        return None;
    }
    let first = literals[0];
    let last = *literals.last().unwrap();
    if !s.starts_with(first) || !s.ends_with(last) || s.len() < first.len() + last.len() {
        return None;
    }
    let mut body = &s[first.len()..s.len() - last.len()];
    let mut raw: Vec<&str> = Vec::new();
    for (n, literal) in literals[1..literals.len() - 1].iter().enumerate() {
        // The earliest occurrence — holes are as short as they can be. A
        // number hole skips occurrences that would leave it non-numeric.
        let mut from = 0;
        let i = loop {
            let i = from + body[from..].find(literal)?;
            if holes[n] != Hole::Number || is_number(&body[..i]) {
                break i;
            }
            from = i + literal.len().max(1);
            if from > body.len() {
                return None;
            }
        };
        raw.push(&body[..i]);
        body = &body[i + literal.len()..];
    }
    raw.push(body);
    let mut out = Vec::with_capacity(raw.len());
    for (text, hole) in raw.iter().zip(&holes) {
        out.push(match hole {
            Hole::Number if !is_number(text) => return None,
            Hole::Translate => to_chinese(text),
            _ => text.to_string(),
        });
    }
    Some(out)
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-')
}

fn fill(template: &str, caps: &[String]) -> String {
    let mut out = template.to_string();
    for (i, cap) in caps.iter().enumerate() {
        out = out.replace(&format!("{{{}}}", i), cap);
    }
    out
}

/// Whole strings.
const EXACT: &[(&str, &str)] = &[
    // Meter labels
    ("5h", "5小时"),
    ("7d", "7天"),
    ("Weekly", "每周"),
    ("Weekly Opus", "每周 Opus"),
    ("Weekly Sonnet", "每周 Sonnet"),
    ("Monthly", "每月"),
    ("Primary", "主窗口"),
    ("Secondary", "次窗口"),
    ("Today", "今日"),
    ("Spend", "支出"),
    ("Credits", "积分"),
    ("Reset credits", "重置次数"),
    ("Usage", "用量"),
    ("Login", "登录"),
    ("Granted", "赠送"),
    ("Topped", "充值"),
    ("Balance", "余额"),
    // Details and tags
    ("available", "可用"),
    ("unlimited", "无限"),
    ("cached", "缓存"),
    ("in Codex", "Codex 使用中"),
    ("in Claude Code", "Claude Code 使用中"),
    ("in CLI", "CLI 使用中"),
    ("API key", "API key"),
    ("rate limit reached", "已达速率上限"),
    ("workspace out of credits", "工作区积分已用完"),
    ("out of credits", "积分已用完"),
    ("workspace usage limit", "工作区用量已达上限"),
    ("usage limit reached", "用量已达上限"),
    ("(limit reached)", "（已达上限）"),
    ("[stale]", "[过期]"),
    // Card notices
    ("Starting the 5h window…", "正在开启 5 小时窗口…"),
    (
        "Codex now uses this account — restart running Codex sessions to switch them",
        "Codex 现在使用此账户——正在运行的 Codex 会话需重启才会切换",
    ),
    // Errors and account problems
    ("not configured", "未配置"),
    ("not logged in", "未登录"),
    ("signed out", "已退出登录"),
    ("rate limited", "已被限流"),
    ("no usage data returned", "没有返回用量数据"),
    ("no usage windows in response", "响应里没有用量窗口"),
    ("no rate limit data for this account", "此账户没有额度数据"),
    ("no rate limits in CLI response", "CLI 响应里没有额度数据"),
    ("account is not available", "账户不可用"),
    ("no balance info", "没有余额信息"),
    ("no model_remains", "没有模型额度数据"),
    ("no model data", "没有模型额度数据"),
    ("no token in the reply", "响应里没有令牌"),
    ("timed out waiting for Codex CLI", "等待 Codex CLI 超时"),
    ("this account is no longer saved", "此账户已不在 tokue 中"),
    ("only saved accounts can be removed", "只能移除在 tokue 中登录的账户"),
    ("no ChatGPT login (empty access token)", "没有 ChatGPT 登录（访问令牌为空）"),
    ("login names no account", "登录信息里没有账户"),
    ("this account's sign-in is not accepted for Go usage", "此账户的登录不能用于读取 Go 用量"),
    ("login expired — open Claude Code once to refresh it", "登录已过期——打开一次 Claude Code 即可刷新"),
    ("access token expired — run codex once to refresh it", "访问令牌已过期——运行一次 codex 即可刷新"),
    ("signed out — add this account again to sign back in", "已退出登录——重新添加此账户即可登录"),
    ("key revoked — add this account again", "密钥已被吊销——请重新添加此账户"),
    ("login rejected — sign in to this account again", "登录被拒绝——请重新登录此账户"),
    ("no refresh token — sign in to this account again", "没有刷新令牌——请重新登录此账户"),
    ("API key rejected — copy it again from the Zen console", "API key 被拒绝——请从 Zen 控制台重新复制"),
    (
        "not logged in — sign in to Claude Code, or add an account in Preferences",
        "未登录——请登录 Claude Code，或在偏好设置中添加账户",
    ),
    (
        "not logged in — run `commandcode login`, or add an account in Preferences",
        "未登录——运行 `commandcode login`，或在偏好设置中添加账户",
    ),
    (
        "no ChatGPT account — sign in to Codex, or add one in Preferences",
        "没有 ChatGPT 账户——请登录 Codex，或在偏好设置中添加",
    ),
    // Sign-in
    ("Opening the browser…", "正在打开浏览器…"),
    ("Waiting for the browser…", "等待在浏览器中完成登录…"),
    ("Approve the key in the browser…", "请在浏览器中批准密钥…"),
    ("Confirm this code in the browser", "请在浏览器中确认此代码"),
    ("Sign-in canceled", "已取消登录"),
    ("a sign-in is already waiting", "已有一个登录在进行中"),
    ("the sign-in was denied", "登录被拒绝"),
    ("the code expired before it was confirmed", "代码在确认前已过期"),
    ("no answer from the browser within 10 minutes", "10 分钟内浏览器没有回应"),
    ("ChatGPT sent back no tokens", "ChatGPT 没有返回令牌"),
    ("Claude sent back no tokens", "Claude 没有返回令牌"),
    ("Claude didn't say which account signed in", "Claude 没有说明登录的是哪个账户"),
    ("OpenCode sent back no device code", "OpenCode 没有返回设备码"),
    ("OpenCode sent back neither a token nor a reason", "OpenCode 既没有返回令牌也没有说明原因"),
    ("OpenCode didn't say which account signed in", "OpenCode 没有说明登录的是哪个账户"),
    ("Command Code sent back no key", "Command Code 没有返回密钥"),
    ("Command Code didn't accept the new key", "Command Code 不接受这个新密钥"),
    ("Command Code didn't say which account signed in", "Command Code 没有说明登录的是哪个账户"),
    ("Codex now uses it", "Codex 已切换到该账户"),
    // Sign-in pages in the browser
    ("You're signed in", "登录成功"),
    ("Sign-in didn't finish", "登录未完成"),
    ("This link isn't from this sign-in", "此链接不属于本次登录"),
    ("Start it again from tokue.", "请在 tokue 中重新开始。"),
    // Menu bar tooltip
    ("no data", "无数据"),
    ("loading", "加载中"),
];

/// Templates, most specific first.
const PATTERNS: &[(&str, &str)] = &[
    // Durations: "2d 4h", "3h 19m", "12m", "1d", "45s"
    ("{#}d {#}h", "{0}天{1}小时"),
    ("{#}h {#}m", "{0}小时{1}分"),
    ("{#}d", "{0}天"),
    ("{#}h", "{0}小时"),
    ("{#}m", "{0}分"),
    ("{#}s", "{0}秒"),
    // Meters
    ("{#}% left", "剩余 {0}%"),
    ("{#}% used", "已用 {0}%"),
    ("¥{=} left", "剩余 ¥{0}"),
    ("${=} left", "剩余 ${0}"),
    ("{} left", "{0}剩余"),
    ("→ {}", "→ {0}"),
    ("{} (stale)", "{0}（过期）"),
    ("{#}% used today", "今日已用 {0}%"),
    ("{#} available", "可用 {0} 次"),
    ("balance {=}", "余额 {0}"),
    ("{=} / {=} credits", "{0} / {1} 积分"),
    ("{=} cr {}", "{0} 积分 {1}"),
    ("{=} cr", "{0} 积分"),
    ("{#} monthly", "月度 {0}"),
    ("{#} purchased", "已购 {0}"),
    ("{#} free", "免费 {0}"),
    ("spend {=}", "支出 {0}"),
    // Notices and errors
    ("rate limited — retrying in {}", "已被限流，{0}后重试"),
    ("Could not switch: {}", "切换失败：{0}"),
    ("Could not start window: {}", "无法开启窗口：{0}"),
    ("{} — it would sign Codex out too", "{0}——这会让 Codex 也退出登录"),
    ("token refresh: {}", "令牌刷新失败：{0}"),
    ("request failed: {=}", "请求失败：{0}"),
    ("parse: {=}", "解析失败：{0}"),
    ("read body: {=}", "读取响应失败：{0}"),
    ("api error: {=}", "接口错误：{0}"),
    ("CLI usage read failed: {}", "通过 CLI 读取用量失败：{0}"),
    ("token refresh via CLI failed: {}", "通过 CLI 刷新令牌失败：{0}"),
    ("cannot start: {=}", "无法启动：{0}"),
    ("did not finish within {#}s", "{0} 秒内没有完成"),
    // Sign-in
    ("Added {=} — Codex now uses it", "已添加 {0}，Codex 已切换到该账户"),
    ("Added {=}", "已添加 {0}"),
    ("{=} is added to tokue. You can close this tab.", "{0} 已添加到 tokue，可以关闭此标签页。"),
    ("{=} has no sign-in; it uses an API key", "{0} 不支持登录，请使用 API key"),
    ("OpenCode refused the sign-in: {=}", "OpenCode 拒绝了登录：{0}"),
    ("cannot listen on {=}", "无法监听 {0}"),
    // Menu bar tooltip
    ("{} {#}%", "{0} {1}%"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meters_and_details_read_in_chinese() {
        let t = to_chinese;
        assert_eq!(t("5h left"), "5小时剩余");
        assert_eq!(t("Weekly left"), "每周剩余");
        assert_eq!(t("→ 10/2 00:18 · 3h 19m"), "→ 10/2 00:18 · 3小时19分");
        assert_eq!(t("→ 10/3 15:12 · 1d"), "→ 10/3 15:12 · 1天");
        assert_eq!(t("2 available"), "可用 2 次");
        assert_eq!(t("¥12.30 left"), "剩余 ¥12.30");
        assert_eq!(t("0.0/37 cr → 3d"), "0.0/37 积分 → 3天");
        assert_eq!(t("1.0/10 cr → 07:37 · 4h 40m"), "1.0/10 积分 → 07:37 · 4小时40分");
        assert_eq!(t("0.0 monthly · 3.0 purchased"), "月度 0.0 · 已购 3.0");
        assert_eq!(t("a@b.com · cached"), "a@b.com · 缓存");
    }

    #[test]
    fn names_and_raw_messages_are_kept() {
        let t = to_chinese;
        assert_eq!(t("Added a@b.com — Codex now uses it"), "已添加 a@b.com，Codex 已切换到该账户");
        assert_eq!(t("Added refresh"), "已添加 refresh");
        assert_eq!(t("token refresh: HTTP 400"), "令牌刷新失败：HTTP 400");
        assert_eq!(t("rate limited — retrying in 45s"), "已被限流，45秒后重试");
        // Ends in "h" but is no duration.
        assert_eq!(t("Could not switch: no refresh"), "切换失败：no refresh");
        assert_eq!(t("HTTP 500 Internal"), "HTTP 500 Internal");
    }

    #[test]
    fn system_setting_follows_the_preferred_language() {
        assert!(is_simplified_chinese("zh-Hans-CN"));
        assert!(is_simplified_chinese("zh-CN"));
        assert!(!is_simplified_chinese("zh-Hant-TW"));
        assert!(!is_simplified_chinese("en-US"));
    }
}
