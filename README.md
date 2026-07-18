<div align="center">

# 🏓 Pong

**A synthetic web health monitor that lives in your system tray.**

[![CI](https://github.com/neochaotic/pong/actions/workflows/ci.yml/badge.svg)](https://github.com/neochaotic/pong/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/neochaotic/pong?color=5e6ad2&include_prereleases&sort=semver)](https://github.com/neochaotic/pong/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Downloads](https://img.shields.io/github/downloads/neochaotic/pong/total?color=4cb782)](https://github.com/neochaotic/pong/releases)

[![Platforms](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)](https://github.com/neochaotic/pong/releases/latest)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-1.77+-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Svelte](https://img.shields.io/badge/Svelte-5-FF3E00?logo=svelte&logoColor=white)](https://svelte.dev)
[![Coverage](https://img.shields.io/badge/coverage-94%25%20web%20%7C%2083%25%20rust-4cb782)](#coverage)

</div>

---

Pong is a lightweight, cross-platform **synthetic web health monitor**.

It keeps a logged-in session to a web dashboard inside a hidden webview, and on a cron schedule it
drives that page the way a real user would — click, type, submit, wait — then reports whether the
dashboard actually responded. It is a synthetic transaction monitor, not a ping: it verifies the app
*works*, not merely that the host answers TCP.

## Install

Grab the installer for your platform from the [latest release](https://github.com/neochaotic/pong/releases/latest):

| Platform | File | Notes |
| --- | --- | --- |
| macOS (Apple Silicon / Intel) | `.dmg` | drag to Applications |
| Windows | `.msi` or `.exe` | `.msi` for managed installs |
| **Linux — any distro** | `.AppImage` | portable; `chmod +x` and run, no install |
| Debian / Ubuntu | `.deb` | `sudo apt install ./Pong_*.deb` |
| Fedora / RHEL / openSUSE | `.rpm` | `sudo dnf install ./Pong-*.rpm` |

> **The builds are not code-signed.** On macOS the first launch is blocked: right-click the app →
> *Open* → *Open*, or run `xattr -cr /Applications/Pong.app`. On Windows, SmartScreen shows a warning:
> *More info* → *Run anyway*. Signing needs a paid Apple Developer account and a Windows
> code-signing certificate.

---

## Why a hidden webview instead of an HTTP request?

An HTTP `GET` tells you the server is up. It cannot tell you that the dashboard renders, that your
session is still valid, or that submitting a form still produces a response. Pong drives a real
browser engine with real cookies, so a check exercises the same path a human would.

---

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│ Rust (Tauri v2)                                              │
│                                                              │
│  tokio-cron-scheduler ──tick──►  monitor::run_health_check   │
│                                        │                     │
│                    1. heartbeat        │  2. full check      │
│                          ▼             ▼                     │
│                    webview.eval(<injected JS>)               │
│                          │                                   │
│  AppState  ◄──IPC report_health──┐                           │
│    │                             │                           │
│    └──emit("monitor://update")──►│                           │
└──────────┬───────────────────────┼───────────────────────────┘
           │                       │
   ┌───────▼────────┐     ┌────────┴──────────────────────────┐
   │ Popover        │     │ Hidden webview  (label: "monitor")│
   │ Svelte 5 + TW  │     │ target dashboard + probe agent    │
   │ (label:popover)│     │ persistent cookies on disk        │
   └────────────────┘     └───────────────────────────────────┘
```

### Typing into rich editors

`full` interaction handles two very different targets:

- **Plain fields** (`<input>`, `<textarea>`) — React and Vue cache the value in their
  own state, so the agent writes through the prototype's native setter and fires
  `input`.
- **Rich editors** (ProseMirror, Slate, Lexical, TipTap) — these keep a private
  document model and treat the DOM as a render target; assigning `textContent` is
  ignored or reverted, and the editor's state never sees the text. The agent instead
  drives the input pipeline: a cancellable `beforeinput` carrying
  `inputType: "insertText"`, then `document.execCommand("insertText")`, which makes
  the browser mutate the selection and fire the events natively. A manual
  `beforeinput` + range insertion is the fallback when an editor cancels the command.

Enter is handled the same way: rich editors bind it in `keydown` and call
`preventDefault`. If the editor claims the key, the agent does not also call
`form.requestSubmit()`, which would submit twice.

Point `selectors.text_input` at the editable node — the default
`textarea, div[contenteditable="true"]` matches both kinds.

### The check pipeline

Every run is two phases, so a dead session is detected before anything is typed into it:

1. **Heartbeat** (read-only). Rust evals `__PONG__.heartbeat()`. The agent inspects the DOM:
   - `selectors.login_indicator` present → **401**, stop here and notify.
   - `selectors.authenticated` present → **200**, proceed.
   - neither → **503**.
2. **Synthetic interaction**. Rust evals `__PONG__.runCheck()`, which:
   - clicks `selectors.action_button` (if configured),
   - types `payload` into `selectors.text_input` **one character at a time**, firing real
     `keydown` / `input` / `keyup` events,
   - presses **Enter** (and calls `form.requestSubmit()` when inside a form),
   - waits `settle_ms` for the DOM to react,
   - re-probes and reports `200`, `401` or `503`.

Every eval carries a **nonce**. Reports whose nonce no longer matches an in-flight probe are
discarded, so a late reply from a previous run can never be mistaken for the current one.

### Status codes

| Code    | Verdict        | Meaning                                        |
| ------- | -------------- | ---------------------------------------------- |
| 200–299 | `healthy`      | Authenticated and the dashboard responded      |
| 401/403 | `unauthorized` | Login screen detected — session expired        |
| 408     | `degraded`     | The probe never reported back in time          |
| 500–599 | `degraded`     | Reached the page, but markers/DOM were wrong   |
| other   | `unreachable`  | Navigation or injection failed outright        |

---

## Signing in

The default target is `https://github.com/login`, so a fresh install has something
real to authenticate against.

1. Click the tray icon → **Show login** (or the tray menu → *Show/Hide Login Window*).
2. The dashboard window appears. Sign in by hand — Pong never handles credentials.
3. Click **Hide login**. Monitoring continues against the now-authenticated session.
4. Quit and relaunch: the session should still be valid, because the webview's
   cookie jar lives on disk (see below).

## Session persistence

The hidden webview is built with `.data_directory(app_data_dir/webview-session)`, so cookies and
local storage survive restarts and you stay logged in.

**Platform caveat, worth knowing:** `data_directory` is honoured by WebView2 (Windows) and
WebKitGTK (Linux). On **macOS**, `WKWebView` ignores it and persists into the app's own container
instead (`~/Library/WebKit/com.pongllm.monitor/` and the app's cookie store). Persistence still
works on all three platforms — only the *location* differs on macOS.

When a check comes back `401`, Pong:

1. fires a native notification — *"Dashboard session expired — reconnect from the menu bar."*
2. opens the popover in recovery mode with a **Reconnect dashboard** button, which un-hides the
   webview so you can log in by hand;
3. once you confirm, hides it again and resumes monitoring.

> **Why the notification is not clickable.** The official `tauri-plugin-notification` exposes no
> click/action handler on desktop ([plugins-workspace#2150](https://github.com/tauri-apps/plugins-workspace/issues/2150)
> is still open). Rather than promise a click that does nothing, Pong opens the popover itself so
> the Reconnect button is already in front of you.

The notification fires only on the *transition* into the unauthorized state, so a dashboard left
logged out does not nag you every cron tick.

---

## Configuration

`config.json` is created on first launch, in the OS config directory:

| Platform | Path                                                            |
| -------- | --------------------------------------------------------------- |
| macOS    | `~/Library/Application Support/com.pongllm.monitor/config.json` |
| Linux    | `~/.config/com.pongllm.monitor/config.json`                     |
| Windows  | `%APPDATA%\com.pongllm.monitor\config.json`                     |

```json
{
  "target_url": "https://example.com/login",
  "cron": "0 */15 * * * *",
  "selectors": {
    "authenticated": "#dashboard-main",
    "login_indicator": "input[type=password]",
    "action_button": "#new-chat",
    "text_input": "textarea"
  },
  "payload": "ping",
  "settle_ms": 3000,
  "typing_delay_ms": 60,
  "notifications_enabled": true,
  "interaction": "probe_only"
}
```

> **`interaction` defaults to `probe_only`, on purpose.** A full check types into
> whatever `text_input` matches and presses Enter — on a real dashboard that can post
> a comment or submit a form, once per cron tick, forever. Start in `probe_only`,
> confirm your selectors are pointing at a scratch surface, then switch to `full`.

### Fields

| Field                     | Meaning                                                                     |
| ------------------------- | --------------------------------------------------------------------------- |
| `target_url`              | Dashboard entry point. Must be `http`/`https`.                              |
| `cron`                    | **Six fields, including seconds**: `sec min hour day-of-month month day-of-week`. |
| `selectors.authenticated` | Present **only** when logged in. This is the health signal.                  |
| `selectors.login_indicator` | Present **only** when logged out. Drives the 401 path.                    |
| `selectors.action_button` | Optional. Clicked before typing. Use `null` to skip.                        |
| `selectors.text_input`    | `<input>`, `<textarea>` or a `contenteditable` element.                     |
| `payload`                 | The string typed during a check.                                            |
| `settle_ms`               | How long to wait for the DOM after Enter. Max `60000`.                      |
| `typing_delay_ms`         | Per-keystroke delay. Max `2000`.                                            |
| `notifications_enabled`   | Native OS notification on session expiry.                                   |
| `interaction`             | `probe_only` (default) inspects the DOM only. `full` clicks, types and submits. |

Invalid values are rejected with a specific error rather than silently defaulted — a bad cron
string or a `file://` URL will refuse to load.

You can edit these fields either by hand or through the **⚙ settings panel** in the popover, which
validates locally before saving and surfaces the backend's error verbatim if it still refuses.

### Cron examples

| Expression      | Meaning              |
| --------------- | -------------------- |
| `0 */5 * * * *` | every 5 minutes      |
| `0 0 * * * *`   | hourly, on the hour  |
| `30 0 9 * * *`  | daily at 09:00:30    |
| `0 0 9 * * Mon` | Mondays at 09:00     |

### Picking selectors for your dashboard

1. Open your dashboard in a browser and log in.
2. DevTools → inspect an element that exists **only when logged in** (a sidebar, a main container).
   That is `authenticated`.
3. Log out, and find an element that exists **only on the login screen** (a password field is the
   most reliable). That is `login_indicator`.
4. Find the input you want to exercise → `text_input`; and the button that reveals it, if any →
   `action_button`.
5. Prefer stable hooks (`#id`, `[data-testid]`) over generated class names, which change on deploy.

Changes to `target_url` and `cron` are applied live — the webview re-navigates and the cron job is
reinstalled without a restart.

---

## How the webview talks back to Rust (and why it needs two files)

This is the least obvious part of the app. The hidden webview loads a **remote origin**, and in
Tauri v2 a remote origin reaches *no* IPC command by default — not even commands the app defines
itself. Getting a report back to Rust requires two pieces working together:

**1. `src-tauri/permissions/report-health.toml`** — an application permission that allows the
command. Without it the invoke is rejected at runtime with:

```
report_health not allowed. Plugin not found
```

(That message is misleading: nothing is wrong with plugins. It means the ACL found no permission
granting the command.)

```toml
[[permission]]
identifier = "allow-report-health"
commands.allow = ["report_health"]
```

**2. `src-tauri/capabilities/monitor.json`** — binds that permission to the `monitor` window and
lists the origins allowed to use it.

```json
{
  "windows": ["monitor"],
  "remote": { "urls": ["http://*:*/*", "https://*:*/*"] },
  "permissions": ["allow-report-health"]
}
```

> **Ports are a separate URLPattern component.** `https://*/*` does **not** match
> `http://localhost:8899`. Include `*:*` patterns if you monitor a non-standard port.

### Security note

`report_health` is deliberately the *only* command exposed to the dashboard. It accepts a status
code and a detail string and can neither read nor mutate anything else. The `remote.urls` list ships
permissive so any configured dashboard works out of the box — **narrow it to your own domain before
deploying:**

```json
"remote": { "urls": ["https://dashboard.your-company.com/*"] }
```

## Logs

Every verdict is logged, to stdout in dev and to the OS log directory in release
(`~/Library/Logs/com.pongllm.monitor/pongllm.log` on macOS):

```
[INFO] check finished: 200 Healthy (2165ms) — dashboard responded
[INFO] check finished: 401 Unauthorized (3ms) — login screen detected
[WARN] probe 7 timed out after 8s (page title: "…")
```

Navigation events are logged too, which is the quickest way to spot a dashboard silently redirecting
to an SSO provider.

---

## Development

```bash
pnpm install
pnpm tauri dev      # run the app
pnpm tauri build    # produce a bundle
```

> **Tip — keep the build cache out of the repo folder.** A Tauri `target/` directory grows to
> several GB. Exporting a shared cache keeps project folders small and lets Rust projects reuse
> each other's compiled dependencies:
>
> ```bash
> export CARGO_TARGET_DIR="$HOME/.cargo-target"   # in ~/.zshrc or ~/.bashrc
> ```
>
> Use the environment variable rather than `.cargo/config.toml`: the latter needs an absolute path,
> which would not resolve on a teammate's machine.

### Tests

```bash
pnpm verify              # versions + types + both suites + both coverage gates
```

Or individually:

```bash
pnpm test                # 71 Vitest tests
pnpm test:coverage       # frontend coverage, fails under 70%
pnpm test:rust           # 67 Rust tests
pnpm test:rust:coverage  # Rust coverage, fails under 70%
pnpm check               # svelte-check
cd src-tauri && cargo clippy --all-targets && cargo fmt --check
```

### Releasing

Versions live in three manifests (`package.json`, `src-tauri/tauri.conf.json`,
`src-tauri/Cargo.toml`) and `pnpm check:version` asserts they agree.

Tag to publish. The tag carries the pre-release identifier; the manifests stay numeric:

```bash
git tag v0.0.1-rc.1 && git push origin v0.0.1-rc.1   # flagged pre-release
git tag v0.0.1     && git push origin v0.0.1       # flagged stable
```

> **Why the manifest cannot say `0.0.1-rc1`.** Tauri's Windows bundlers reject
> pre-release identifiers ([#5286](https://github.com/tauri-apps/tauri/issues/5286),
> [#12470](https://github.com/tauri-apps/tauri/issues/12470)). Such a version builds
> fine on macOS and Linux, then fails only in the Windows release job.
> `pnpm check:version` blocks it locally so you find out in seconds, not after a
> 15-minute matrix build.

Any tag with a `-suffix` is marked as a pre-release on GitHub, so it never
becomes "Latest release". Releases are created as drafts — review the installers
before publishing.

### Coverage

Both suites enforce a **70% floor** and currently sit well above it:

| | Statements / Regions | Lines |
| --- | --- | --- |
| Frontend | 94.6% | 96.9% |
| Rust (logic) | 83.2% | 84.4% |

**What the Rust number excludes, and why.** `lib.rs`, `tray.rs` and `main.rs` are wiring:
window construction, IPC registration, tray callbacks. None of it exists until a real app,
tray and webview are running, so unit tests cannot reach it — the same reason `main.ts` is
excluded on the frontend. The gate measures the logic; the wiring is verified by running
the app.

**Read the number with that caveat in mind.** Every bug found in this project so far lived
in the wiring band: the missing ACL permission that silently blocked *every* IPC command,
a Promise rejection swallowed by a synchronous `try/catch`, and macOS suspending occluded
webviews. A perfectly green logic suite would have caught none of them. High coverage here
means the rules are right — not that the app runs.

The pipeline has also been exercised end to end against a local fake dashboard, covering the healthy
path (click → type → Enter → 200), the session-expiry path (DOM swapped for a login form → 401) and
the in-between state where neither marker is present (503).

The Rust suite covers config parsing/validation, cron arithmetic, verdict mapping, JS-injection
escaping, and the nonce-correlated probe state machine. Logic is deliberately kept in pure modules
(`config`, `scheduler`, `health`, `injection`, `state`) so it is testable without a display server;
`lib.rs`, `tray.rs` and the eval calls are the only parts that need a running app.

### Layout

```
src/                    Svelte 5 popover UI
  lib/format.ts         pure display helpers (unit tested)
  lib/StatusBadge.svelte
  lib/api.ts            typed IPC wrapper
src-tauri/src/
  config.rs             config.json parsing + validation
  scheduler.rs          cron arithmetic (pure)
  health.rs             verdicts, phases, reports
  injection.rs          builds the evaluated JS (escaping via serde_json)
  agent.js              the injected probe agent
  state.rs              AppState + nonce correlation
  monitor.rs            the check pipeline
  tray.rs               tray icon, menu, popover toggle
  lib.rs                wiring: windows, IPC commands, scheduler
src-tauri/permissions/  ACL permission for the report_health command
src-tauri/capabilities/ binds permissions to windows and remote origins
```

---

## Implementation notes

**Framework-aware typing.** React and Vue keep their own copy of an input's value and revert a plain
`el.value = x`. The agent writes through the prototype's native setter
(`Object.getOwnPropertyDescriptor(proto, "value").set`), which is what makes the framework observe
the change. `contenteditable` elements are handled separately.

**Injection safety.** Selectors and payload never reach JS via string concatenation. They cross as a
single `serde_json`-encoded object literal, so escaping is the serializer's job. Tests assert that
quotes, backslashes and newlines cannot break out.

**Tray-only.** No window is declared in `tauri.conf.json`; both webviews are created programmatically.
On macOS the activation policy is set to `Accessory`, so there is no dock icon and no app-switcher
entry. `ExitRequested` is intercepted so closing the popover does not quit the app.
