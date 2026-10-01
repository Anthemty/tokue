# tokue — AI quota in the macOS menu bar

Multi-provider usage monitor for the macOS menu bar. Currently supports **OpenCode Go**, **DeepSeek** (balance), **MiniMax** (token plan quota), **Codex** (ChatGPT subscription usage, several accounts), **Command Code** (commandcode.ai credits and 5h/weekly windows) and **Claude** (claude.ai subscription windows). Click the menu bar icon to open a popover: a left sidebar switches between providers, the right pane shows that provider's usage as progress bars on a dark, fixed "terminal" surface (it does not follow system light/dark mode — that's deliberate). Settings live in their own standalone **Preferences** window (open it from the ⚙ in the popover header), not inside the popover itself. Accounts are added there by signing in — in the browser or with a device code — wherever the provider allows it, and by API key where it doesn't.

The menu bar icon is a monochrome template ring gauge that depletes clockwise from the top as quota runs out — a healthy account draws most of the ring, an almost-exhausted one only a sliver — with a coloured percentage badge next to it (green with 30%+ left, amber under 30%, red under 10%). The badge reads the same way the panel does (remaining by default, used if you switch the panel's Meters setting), and the ring always matches whatever number the badge shows.

## Quick Start

```bash
make app                                # build → tokue.app
ditto tokue.app /Applications/tokue.app  # install
open /Applications/tokue.app             # launch
```

## Popover

```
              ┌──┬──────────────────────┐
              │  │  OpenCode Go      ⚙  │
              │</>│  Updated 14:32:05    │
              │  │  Rolling    2%        │
[◔ gauge] ──▶│  │  ▓░░░░░░░░░░  2h 1m  │
              │🐳│  Weekly   24%        │
              │  │  ▓▓▓░░░░░░░░  18h    │
              │〰 │  Monthly  63%        │
              │  │  ▓▓▓▓▓▓░░░░  6d 11h  │
              │  │                       │
              │  │  Refresh       Quit   │
              └──┴───────────────────────┘
```

Click a sidebar icon to switch providers. Click **⚙** in the header to open the standalone **Preferences** window (see [Setup](#setup)) — the popover closes and the window opens in its own space, so it stays out of your way while you're just glancing at usage. The popover itself closes when you click outside it.

### OpenCode Go
- **Auth**: **Add account** signs in to your OpenCode console account with a device code (Preferences shows the code; confirm it on the page that opens). An API key from the [Zen console](https://opencode.ai/auth) — the same key the TUI's `/connect` takes — works too and shows as its own account.
- **Data**: 3 time windows — 5-hour, weekly, monthly. Go's allowance is a monthly
  dollar amount; the shorter windows are fractions of it (5h = 20%, weekly = 50%).

### DeepSeek
- **Auth**: API key (Bearer token)
- **Data**: monetary balance (¥), granted vs topped-up

### MiniMax
- **Auth**: Token Plan API key
- **Data**: 5-hour window + weekly token quota usage

### Codex (ChatGPT accounts)
Shows ChatGPT subscription usage for several accounts, not an API-key bill.

- **Accounts**: one is Codex's own login (`~/.codex/auth.json`). Every other account is added with **Add account** in Preferences — the same browser sign-in `codex login` uses (callback on `localhost:1455`) — and kept in `~/.config/tokue/accounts/codex.json` (0600) in the exact `auth.json` shape. Accounts are told apart by user × workspace, so two people in one ChatGPT Team stay separate. Every account renders as its own card.
- **Switching**: **Use in Codex** (Preferences, or the account's card in the popover, with a confirm step) makes a saved account Codex's login and moves the previous one into tokue's store. A refresh token is single-use, so each login has exactly one holder: the CLI refreshes its own, tokue refreshes the saved ones.
- **Data**: the same endpoint the Codex CLI itself uses, `GET https://chatgpt.com/backend-api/wham/usage`, with the account's OAuth access token. The pane groups rows per account (email · plan) and shows the 5-hour and weekly windows with reset times, plus optional credits / spend / reset-credit rows.
- **Codex's own login is never refreshed by tokue.** When its access token is close to expiry tokue asks the `codex` CLI (`codex app-server` → `account/rateLimits/read`), so the CLI stays its single owner. `auth.json` is only written by **Use in Codex**.
- **Transports**: HTTPS via reqwest, falling back to the system `curl` (native TLS) if the request is refused, then to the CLI. Failures are per account and isolated; the last good snapshot is cached in `~/.config/tokue/cache/codex.json` and shown as `cached`.
- **Colour system**: quota rows read status by colour — **green** while there is room, **amber** under 30% left, **red** under 10%; grey is reserved for structure (bar tracks, separators, reset times) and for rows that are not quota (credits). Each account sits on a raised, bordered card, and the popover grows to fit every account instead of scrolling.
- **Settings**: the Codex page in Preferences renames, hides, switches and removes accounts and toggles the optional **Show spend limit** and **Show today's usage** rows (both off by default); the General page switches every meter between **Used** and **Remaining** (default: remaining, shown as `5h left 43%`). Every control applies immediately — there is no Save button.
- The menu bar badge reports the worst quota across all providers, read the same way the panel is (remaining by default), so its colour and the ring always mean "how close to the limit".

### Command Code
Reads the CLI's own login, `~/.commandcode/auth.json` (read-only), and any account added with **Add account**: that opens commandcode.ai's CLI sign-in page, which hands a new API key back to a one-shot callback on `127.0.0.1`; tokue checks the key with `whoami` before keeping it. Usage comes from the CLI's own endpoints on `api.commandcode.ai`: credits (monthly/purchased/free remaining) plus the 5h and weekly `windowLimits` with their reset times. Cloudflare fronts that API, so calls that get fingerprint-blocked fall back to the system curl.

### Claude
Reads Claude Code's own login (macOS keychain, or `~/.claude/.credentials.json`; read-only — Claude Code refreshes it) plus any account added with **Add account**, which signs in to claude.ai in the browser (OAuth with PKCE, the same client Claude Code uses) and keeps the tokens in `~/.config/tokue/accounts/claude.json`. Shows the 5-hour and weekly windows from `api.anthropic.com/api/oauth/usage`. A saved login the server stops accepting is marked **signed out**; signing in again replaces it.

### Saved logins
Every login tokue signs in to itself lives in `~/.config/tokue/accounts/<provider>.json` — directory 0700, files 0600, written atomically, one lock per account around its refresh token. Removing an account in Preferences deletes it from that file; a CLI's own login is never touched (it isn't removable there).

### Config in SQLite
Configuration lives in the `config` table of `tokue.db` (same store as the usage history). An existing `config.json` is imported once and renamed to `config.json.migrated`. Provider enable switches also live there; a provider that is switched off stops being fetched and leaves the sidebar and panel until it is re-enabled.

## Setup

Open the popover and click **⚙** in the header — this opens the standalone **Preferences** window on the provider you were looking at. Its sidebar has **General** and one page per provider (the dot shows whether it's enabled):

- **General** — the interface language (**System** / **English** / **简体中文**; System follows macOS's first preferred language), the Used/Remaining meter reading and the refresh interval.
- **A provider's page** — **Show in the popover** (off stops fetching it), its **Accounts** (rename by editing the title, hide/show with the switch, **Remove** for saved ones, **Use in Codex** for Codex) and **Add account**, plus an **API key** field for the providers that take one:

| Provider | Add account | API key |
| --- | --- | --- |
| Codex | browser sign-in | — |
| Claude | browser sign-in | — |
| Command Code | browser sign-in (hands back a CLI key) | — |
| OpenCode Go | device code | optional |
| DeepSeek | — | required |
| MiniMax | — | required (Token Plan key) |

DeepSeek and MiniMax offer no sign-in flow for third-party apps, so they stay key-only. A sign-in in progress can be canceled from the same page; it gives up on its own after a few minutes.

Every control applies immediately — there is no Save button, and closing the window doesn't discard anything.

Config lives in `tokue.db` (see [Config in SQLite](#config-in-sqlite)); an old `config.json` is imported once. The Codex section:

```json
{
  "codex": {
    "accounts": [
      { "key": "codex:user-abc__acct-123", "label": "", "enabled": true },
      { "key": "codex:user-def__acct-456", "label": "team", "enabled": false }
    ],
    "refresh_minutes": 15,
    "show_spend": false,
    "show_remaining": true
  },
  "refresh_minutes": 15
}
```

An account missing from `accounts` is shown with its own name; other providers keep the same per-account `label`/`enabled` under `account_settings.<provider>`. `refresh_minutes` (top level or per provider) overrides the default 15-minute cycle; cycles back off automatically while every provider fails.

## Storage (SQLite)

Before it was named tokue the app was OCG and kept everything in `~/.config/ocg/ocg.db`. The first launch of tokue moves that directory to `~/.config/tokue` (renamed, not copied, so each saved login's refresh token still has one copy) after checkpointing the database; until that has happened the old location keeps being used. Quit OCG before starting tokue.

Usage history lives in `~/.config/tokue/tokue.db` (SQLite, WAL). Two tables:

- `samples` — one row per account/window per refresh, with `reset_at` and `window_secs` so consumption can be summed **per limit window** rather than per clock hour. Rows older than 90 days are pruned on write.
- `snapshots` — the newest payload per account, which is what paints the panel instantly after a restart and what a failed fetch falls back to (`cached`).

The Codex **Today** row is derived from `samples`: it walks the window in order, adds each rise, credits a reading in full when its window opened after midnight, and skips a reading whose window predates the range rather than over-reporting it.

An existing `~/.config/tokue/cache/codex.json` (pre-SQLite builds) is imported on first run and renamed to `codex.json.migrated`. Provider credentials and settings live in the `config` table; saved logins in `~/.config/tokue/accounts/`.

```bash
./target/release/tokue --once stats   # db path, row counts, per-account consumption since local midnight
```

## Debugging

`--once` fetches without the UI and prints JSON — the fastest way to check the Codex accounts:

```bash
./target/release/tokue --once codex   # per-account snapshot, transport, errors (read-only, records nothing)
./target/release/tokue --once stats   # SQLite contents + quota burned since local midnight
./target/release/tokue --once all     # full refresh cycle (writes history) + panel state
```

Environment switches (all optional):

| Variable | Effect |
| --- | --- |
| `TOKUE_CODEX_BIN` | Path to the `codex` binary used by the CLI fallback |
| `TOKUE_CODEX_FORCE_RPC` | Skip HTTPS and always use the CLI path |
| `TOKUE_SNAPSHOT=path.png` | Render the panel off-screen to a PNG at first refresh |
| `TOKUE_SNAPSHOT_SETTINGS=<page>` | Capture that Preferences page (`general` or a provider id; `1` = General) instead of the popover |
| `TOKUE_AUTOOPEN=1` | Open the popover shortly after launch |
| `TOKUE_STATUS_BADGE=42` | Force the number inside the menu bar ring |
| `TOKUE_LANG=zh-Hans` / `en` | Force the interface language (overrides Preferences), e.g. for snapshots or `--once` |
| `TOKUE_SIGNIN_NO_BROWSER=1` | "Add account" logs the sign-in page instead of opening the browser |
| `TOKUE_DEBUG_OPENCODE=1` | Log the raw OpenCode Go usage response (shape is undocumented) |
| `TOKUE_DEBUG_SIZE=1` | Log every popover height decision (content vs screen cap) |

## Build

```bash
make        # builds tokue.app (requires Rust + Xcode CLT)
make run    # builds and opens the app
make build  # plain binary at target/release/tokue
make dmg    # tokue-<version>.dmg: the app + an Applications link, signed ad hoc
```

The disk image is built for Apple silicon only (arm64) and signed ad hoc, not with a Developer ID, so it is not notarized. On another Mac, Gatekeeper blocks the first launch of a downloaded copy: open it once, then allow it under **System Settings → Privacy & Security → Open Anyway** (or remove the quarantine flag: `xattr -dr com.apple.quarantine /Applications/tokue.app`).

Requires Rust 1.80+ and Xcode Command Line Tools (for compiling the native AppKit shell and linking Cocoa). The Objective-C UI layer (`app_darwin.m`) is compiled via [`cc`](https://crates.io/crates/cc) in `build.rs`.

---

**Version 0.0.6** — Command Code provider (credits + 5h/weekly windows), per-provider enable switches in settings, and configuration moved into the SQLite store.

**Version 0.0.5** — usage history in SQLite (`ocg.db`): window samples + last-good snapshots, an opt-in per-account "Today" row, and `--once stats`.

**Version 0.0.4** — panel typography one notch smaller (sidebar marks 15pt in a 36pt rail with ~10pt of air, all right-pane text −2pt), and a Used/Remaining switch for the Codex meters (defaults to remaining).

**Version 0.0.3** — Codex provider now shows ChatGPT subscription usage for every `~/.codex*` login (grouped rows, read-only credentials, CLI-assisted refresh, per-account error isolation), plus a percentage badge in the menu bar and a scrollable panel.

**Version 0.0.2** — rewritten in Rust (replaces Go). Native NSPopover shell, monochrome template gauge icon, brand-logo sidebar, inline credential editing.
