//! Regression guard for a real bug: `agent.js` used to compute a shrinking
//! "time remaining" budget for the response wait and cleanup, instead of
//! giving each stage its own full `element_timeout_ms` window — which is
//! what `monitor::check_budget` (the Rust-side watchdog) already assumes: it
//! reserves one independent `element_timeout_ms` pass per stage (text input,
//! response, and each configured cleanup step).
//!
//! With the shrinking budget, a slow AI reply could eat most or all of the
//! shared timeout before cleanup even started, leaving it a near-zero or
//! outright negative deadline. `setTimeout` silently clamps a negative delay
//! to zero rather than erroring, so cleanup would fail almost instantly —
//! observed live as "confirm button never appeared (dialogs=0 buttons=0)"
//! because the confirmation dialog never got a chance to mount.
//!
//! `agent.js` runs only inside a real webview and has no JS test harness in
//! this project, so this pins the fix structurally: each stage must be
//! called with the stage's own full `timeout`, not a computed remainder.

#[test]
fn response_wait_and_cleanup_each_get_their_own_full_timeout_budget() {
    let source = include_str!("../src/agent.js");

    assert!(
        source.contains("waitForStableText(p.selectors.response, timeout)"),
        "the response wait must use the full `timeout`, not a shrinking remainder \
         (see check_budget in monitor.rs, which reserves one independent \
         element_timeout_ms pass per stage)"
    );
    assert!(
        source.contains("runCleanup(p.cleanup, timeout)"),
        "cleanup must get the full `timeout`, not `timeout - elapsed` — a slow AI \
         reply can otherwise starve cleanup down to a near-zero or negative budget"
    );
    assert!(
        !source.contains("remainingForCleanup") && !source.contains("timeout - (performance.now()"),
        "a shrinking per-stage budget for the response wait / cleanup was removed \
         as buggy; don't reintroduce it"
    );
}
