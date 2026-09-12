# ocg — Usage Monitor (macOS Menu Bar)

Multi-provider usage monitor for the macOS menu bar. Currently supports **OpenCode Go**, **DeepSeek** (balance), **MiniMax** (token plan quota), and **Codex** (ChatGPT subscription usage for every `~/.codex*` login). Click the menu bar icon to open a native popover: a left sidebar shows each provider's brand logo, the right pane shows that provider's usage as progress bars. Switch providers by clicking a sidebar icon; configure credentials inline from the ⚙ Settings view.

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

- **Accounts**: each `~/.codex*` directory that holds a `codex login` session is discovered automatically (`.codex`, `.codex2`, `.codex3` …). Add a home on disk and it appears after the next refresh, or press **Rescan ~/.codex\*** in the settings pane.
- **Data**: the same endpoint the Codex CLI itself uses, `GET https://chatgpt.com/backend-api/wham/usage`, with the account's OAuth access token. The pane groups rows per account (email · plan) and shows the 5-hour and weekly windows with reset times, plus optional credits / spend / reset-credit rows.
- **Credentials are read-only.** ocg never writes `auth.json`. When an access token is close to expiry it asks the `codex` CLI (`codex app-server` → `account/rateLimits/read`) instead of refreshing on its own, so the CLI stays the single owner of token rotation.
- **Transports**: HTTPS via reqwest, falling back to the system `curl` (native TLS) if the request is refused, then to the CLI. Failures are per account and isolated; the last good snapshot is cached in `~/.config/ocg/cache/codex.json` and shown as `cached`.
- **Two homes, one account**: if several homes hold the same ChatGPT account the row leads with the home path and is marked `⧉` (they share one quota).
- **Settings**: rename an account inline, enable/disable it, and toggle the optional **Show spend limit** row (off by default — most plans cap it at 0 credits).

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
    "show_spend": false
  },
  "refresh_minutes": 15
}
```

An empty `accounts` list means "auto-discover every `~/.codex*`". `refresh_minutes` (top level or per provider) overrides the default 15-minute cycle; cycles back off automatically while every provider fails.

## Debugging

`--once` fetches without the UI and prints JSON — the fastest way to check the Codex accounts:

```bash
./target/release/ocg --once codex   # per-account snapshots, transport, errors
./target/release/ocg --once all     # full panel state
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

## Build

```bash
make        # builds OCGTool.app (requires Rust + Xcode CLT)
make run    # builds and opens the app
make build  # plain binary at target/release/ocg
```

Requires Rust 1.80+ and Xcode Command Line Tools (for compiling the native AppKit shell and linking Cocoa). The Objective-C UI layer (`app_darwin.m`) is compiled via [`cc`](https://crates.io/crates/cc) in `build.rs`.

---

**Version 0.0.3** — Codex provider now shows ChatGPT subscription usage for every `~/.codex*` login (grouped rows, read-only credentials, CLI-assisted refresh, per-account error isolation), plus a percentage badge in the menu bar and a scrollable panel.

**Version 0.0.2** — rewritten in Rust (replaces Go). Native NSPopover shell, monochrome template gauge icon, brand-logo sidebar, inline credential editing.
