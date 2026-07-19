---
title: "Pong v0.0.1: a tray watchdog that keeps your Claude session warm before you sit down"
published: false
description: "Pong just shipped v0.0.1 — a lightweight menu-bar app (Tauri + Rust) that pre-warms your Claude session on a schedule, shows a live usage dashboard with a real-time reset countdown, and runs a synthetic health check instead of a ping. Here's the productivity case for it."
tags: productivity, rust, opensource, ai
cover_image: https://raw.githubusercontent.com/neochaotic/pong/main/docs/screenshots/dash-normal.png
series: "Building Pong"
---

> **TL;DR** — [Pong](https://github.com/neochaotic/pong) `v0.0.1` just shipped. It's a menu-bar-only
> Tauri app for people who live inside Claude.ai all day: a live usage dashboard with a real-time
> reset countdown, a scheduled session warm-up so the login screen is never what's waiting for you,
> and a health check that's an actual synthetic transaction, not a ping. Grab an installer from the
> [latest release](https://github.com/neochaotic/pong/releases/latest) — macOS, Windows, and Linux.

---

## Three small interruptions, once a day each

None of these will ever make an incident report. They're just friction, and friction compounds:

1. **"Am I about to get throttled?"** — you tab away from your editor to a usage page, squint at a
   number, tab back. Thirty seconds, minimum, every time the thought crosses your mind.
2. **The session window resets on its own clock, not yours.** Claude's usage limits run on a
   rolling ~5-hour window that starts at your *first* message — not at a fixed time. Burn through it
   fast in the morning and you can find yourself sitting out the back half of the window in the
   afternoon, exactly when you wanted to keep working.
3. **A session can go quietly dead.** Cookie expired, logged out somewhere else — and the first
   sign is a prompt that just doesn't respond right, mid-task.

Pong is a tray app that turns all three into "check the icon" instead of "stop and go find out."

<p align="center">
  <img src="https://raw.githubusercontent.com/neochaotic/pong/main/docs/screenshots/dash-normal.png" width="260" alt="Pong's tray popover showing comfortable session and weekly usage bars">
</p>

## Usage, always in view

Session and weekly consumption sit in the popover with the reset countdown ticking in real time.
It's a glance at the tray icon, not a context switch to a browser tab — which matters more than it
sounds like, because the *cost* of checking usage manually isn't the ten seconds it takes, it's the
train of thought you drop to go do it.

## Timing the warm-up (the part that actually saves you time)

This is the trick that makes Pong more than a dashboard. A scheduled warm-up sends a real message —
same as one you'd type — so it opens a usage window just like any other. Point it at a time you
aren't using Claude anyway, say 5am:

<p align="center">
  <img src="https://raw.githubusercontent.com/neochaotic/pong/main/docs/screenshots/monitor-tab.png" width="260" alt="Pong's monitor tab counting down to the next scheduled warm-up check">
</p>

- The 5am warm-up opens a window that runs until 10am.
- You sit down at 8am — already three hours into that window, two hours of headroom before it
  rolls over.
- At 10am it rolls into a **fresh** 5-hour window, mid-task, instead of one you have to stop and
  wait for.

The morning reads as one continuous stretch instead of two hours of work followed by three of
watching a countdown. You're not getting more usage — you're choosing *when* the clock starts, so
it never starts while you're mid-task.

## A health check that's actually a health check

Most uptime checks are `curl -I` and a `200`. That tells you a server answered — not that the page
renders, not that your session is still valid, not that submitting a form does anything. Pong drives
a real hidden webview through the same path a human takes: it types into the chat box (handling
rich editors like ProseMirror/Slate/Lexical, which ignore a plain `textContent` write and need the
real `beforeinput`/`insertText` pipeline instead), submits, and confirms the app actually responded.

When a check comes back unauthorized, Pong fires a native notification and opens the popover in
recovery mode with a **Reconnect** button — so you find out from a notification, not from a prompt
that silently goes nowhere.

## How it's built

- **Tauri v2 + Rust** backend, **Svelte 5** frontend — not Electron. The entire point of living
  quietly in the tray is not costing 200MB of RAM to do it.
- A `tokio-cron-scheduler` drives the hidden webview on your schedule; results come back over IPC
  to a tray-resident popover.
- No macOS accessibility or screen-recording permissions needed — the synthetic interaction happens
  inside the app's own webview via injected JavaScript, not by driving the OS.
- 93% web / 82% Rust test coverage as of this release.

## What v0.0.1 is (and isn't)

- First stable release, cross-platform — macOS, Windows, and Linux installers, all built and
  bundled by CI on every push.
- The builds are **not code-signed** yet — that needs a paid Apple Developer account and a Windows
  code-signing certificate. First launch needs one extra step (documented right in the
  [README](https://github.com/neochaotic/pong#install) and the release notes), after which it's a
  normal double-click.
- Development so far has mostly happened on macOS. If you try it on Windows or Linux, a report of
  how the popover looks and behaves is genuinely useful signal.

## Try it

- Installers: [latest release](https://github.com/neochaotic/pong/releases/latest)
- Source, issues, PRs: [github.com/neochaotic/pong](https://github.com/neochaotic/pong) — MIT
  licensed.

If you've ever lost the first five minutes of a Claude session to a dead login or watched a usage
window reset at the worst possible moment, this is the tool for that.
