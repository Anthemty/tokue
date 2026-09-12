# ocg — Usage Monitor (macOS Menu Bar)

Multi-provider usage monitor for the macOS menu bar. Currently supports **OpenCode Go**, **DeepSeek** (balance), **MiniMax** (token plan quota), **Codex** (ChatGPT subscription usage for every `~/.codex*` login) and **Command Code** (commandcode.ai credits and 5h/weekly windows). Providers can be switched on/off in the settings pane, and all configuration lives in the SQLite store. Click the menu bar icon to open a native popover: a left sidebar shows each provider's brand logo, the right pane shows that provider's usage as progress bars. Switch providers by clicking a sidebar icon; configure credentials inline from the ⚙ Settings view.

The menu bar icon is a monochrome template gauge — a ring that fills proportionally to the worst usage across all providers, tinted automatically to match light/dark mode — with a percentage badge next to it (amber from 50%, red from 80%).

## Quick Start

```bash
make app                          # build → OCGTool.app
cp -R OCGTool.app ~/OCGTool.app   # copy out of source tree
open ~/OCGTool.app                # launch
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

Click a sidebar icon to switch providers. Click **⚙** in the header to edit the active provider's credentials inline (auth cookie / API key) and **Save & Refresh**. The popover closes when you click outside it.

### OpenCode Go
- **Auth**: cookie-based (browser DevTools → Application → Cookies)
- **Data**: 3 time windows (rolling / weekly / monthly) with percentage used

### DeepSeek
- **Auth**: API key (Bearer token)
- **Data**: monetary balance (¥), granted vs topped-up

### MiniMax
- **Auth**: Token Plan API key
- **Data**: 5-hour window + weekly token quota usage

### Codex (ChatGPT accounts)
Shows subscription usage for **every ChatGPT login on the machine**, not an API-key bill.

- **Accounts**: each `~/.codex*` directory that holds a `codex login` session is discovered automatically (`.codex`, `.codex2`, `.codex3` …). Add a home on disk and it appears after the next refresh, or press **Rescan ~/.codex\*** in the settings pane. Every account renders as its own card — email · plan on the left, the `~/.codexN` home on the right — with one compact line plus a thin bar per window.
- **Data**: the same endpoint the Codex CLI itself uses, `GET https://chatgpt.com/backend-api/wham/usage`, with the account's OAuth access token. The pane groups rows per account (email · plan) and shows the 5-hour and weekly windows with reset times, plus optional credits / spend / reset-credit rows.
- **Credentials are read-only.** ocg never writes `auth.json`. When an access token is close to expiry it asks the `codex` CLI (`codex app-server` → `account/rateLimits/read`) instead of refreshing on its own, so the CLI stays the single owner of token rotation.
- **Transports**: HTTPS via reqwest, falling back to the system `curl` (native TLS) if the request is refused, then to the CLI. Failures are per account and isolated; the last good snapshot is cached in `~/.config/ocg/cache/codex.json` and shown as `cached`.
- **Two homes, one account**: if several homes hold the same ChatGPT account, each card still names its own home and the title carries `⧉` (they share one quota).
- **Colour system**: quota rows read status by colour — **green** while there is room, **amber** under 30% left, **red** under 10%; grey is reserved for structure (bar tracks, separators, reset times) and for rows that are not quota (credits). Each account sits on a raised, bordered card, and the popover grows to fit every account instead of scrolling.
- **Settings**: rename an account inline, enable/disable it, switch the meter reading between **Used** and **Remaining** (default: remaining, shown as `5h left 43%`), and toggle the optional **Show spend limit** and **Show today's usage** rows (both off by default).
- The menu bar badge keeps reporting the worst *used* percentage across all providers, so its colour and the gauge fill always mean "how close to the limit".

### Command Code
Reads `~/.commandcode/auth.json` (read-only) and calls the CLI's own endpoints on `api.commandcode.ai`: credits (monthly/purchased/free remaining) plus the 5h and weekly `windowLimits` with their reset times. Cloudflare fronts that API, so calls that get fingerprint-blocked fall back to the system curl.

### Config in SQLite
Configuration lives in the `config` table of `ocg.db` (same store as the usage history). An existing `config.json` is imported once and renamed to `config.json.migrated`. Provider enable switches also live there; a provider that is switched off stops being fetched and leaves the sidebar and panel until it is re-enabled.

## Setup

Open the popover, click **⚙** in the header, edit the fields for the active provider, and click **Save & Refresh**. For Codex the settings pane lists the discovered ChatGPT logins: tick the ones to display, give them names, then save.

Config stored at `~/.config/ocg/config.json`. Old single-provider config files are automatically migrated. Codex section:

```json
{
  "codex": {
    "accounts": [
      { "home": "~/.codex", "label": "", "enabled": true },
      { "home": "~/.codex2", "label": "team", "enabled": true }
    ],
    "refresh_minutes": 15,
    "show_spend": false,
    "show_remaining": true
  },
  "refresh_minutes": 15
}
```

An empty `accounts` list means "auto-discover every `~/.codex*`". `refresh_minutes` (top level or per provider) overrides the default 15-minute cycle; cycles back off automatically while every provider fails.

## Storage (SQLite)

Usage history lives in `~/.config/ocg/ocg.db` (SQLite, WAL). Two tables:

- `samples` — one row per account/window per refresh, with `reset_at` and `window_secs` so consumption can be summed **per limit window** rather than per clock hour. Rows older than 90 days are pruned on write.
- `snapshots` — the newest payload per account, which is what paints the panel instantly after a restart and what a failed fetch falls back to (`cached`).

The Codex **Today** row is derived from `samples`: it walks the window in order, adds each rise, credits a reading in full when its window opened after midnight, and skips a reading whose window predates the range rather than over-reporting it.

An existing `~/.config/ocg/cache/codex.json` (pre-SQLite builds) is imported on first run and renamed to `codex.json.migrated`. Provider credentials and settings stay in `config.json` — they are meant to be hand-editable.

```bash
./target/release/ocg --once stats   # db path, row counts, per-account consumption since local midnight
```

## Debugging

`--once` fetches without the UI and prints JSON — the fastest way to check the Codex accounts:

```bash
./target/release/ocg --once codex   # per-account snapshot, transport, errors (read-only, records nothing)
./target/release/ocg --once stats   # SQLite contents + quota burned since local midnight
./target/release/ocg --once all     # full refresh cycle (writes history) + panel state
```

Environment switches (all optional):

| Variable | Effect |
| --- | --- |
| `OCG_CODEX_BIN` | Path to the `codex` binary used by the CLI fallback |
| `OCG_CODEX_FORCE_RPC` | Skip HTTPS and always use the CLI path |
| `OCG_SNAPSHOT=path.png` | Render the panel off-screen to a PNG at first refresh |
| `OCG_SNAPSHOT_SETTINGS=1` | Capture the settings pane instead of the usage pane |
| `OCG_AUTOOPEN=1` | Open the popover shortly after launch |
| `OCG_STATUS_TITLE="42%"` | Force the menu bar badge text |
| `OCG_DEBUG_SIZE=1` | Log every popover height decision (content vs screen cap) |

## Build

```bash
make        # builds OCGTool.app (requires Rust + Xcode CLT)
make run    # builds and opens the app
make build  # plain binary at target/release/ocg
```

Requires Rust 1.80+ and Xcode Command Line Tools (for compiling the native AppKit shell and linking Cocoa). The Objective-C UI layer (`app_darwin.m`) is compiled via [`cc`](https://crates.io/crates/cc) in `build.rs`.

---

**Version 0.0.6** — Command Code provider (credits + 5h/weekly windows), per-provider enable switches in settings, and configuration moved into the SQLite store.

**Version 0.0.5** — usage history in SQLite (`ocg.db`): window samples + last-good snapshots, an opt-in per-account "Today" row, and `--once stats`.

**Version 0.0.4** — panel typography one notch smaller (sidebar marks 15pt in a 36pt rail with ~10pt of air, all right-pane text −2pt), and a Used/Remaining switch for the Codex meters (defaults to remaining).

**Version 0.0.3** — Codex provider now shows ChatGPT subscription usage for every `~/.codex*` login (grouped rows, read-only credentials, CLI-assisted refresh, per-account error isolation), plus a percentage badge in the menu bar and a scrollable panel.

**Version 0.0.2** — rewritten in Rust (replaces Go). Native NSPopover shell, monochrome template gauge icon, brand-logo sidebar, inline credential editing.
