//! Guards the wiring bug this project has hit more than once: a command
//! registered in `tauri::generate_handler!` but missing from the popover's
//! ACL permission. Tauri denies it at runtime with "not allowed. Command not
//! found" — a message that gives no hint the fix is a one-line TOML edit,
//! and previous instances of this bug shipped before anyone noticed the
//! button just silently did nothing.
//!
//! This is a plain-text structural check, not a Rust/TOML parser: both files
//! use a simple one-item-per-line list, so splitting on commas is enough.

const LIB_RS: &str = include_str!("../src/lib.rs");
const APP_COMMANDS_TOML: &str = include_str!("../permissions/app-commands.toml");

/// Pulls the comma-separated identifiers out of `start...end`, trimming
/// whitespace and any surrounding quotes so `generate_handler!` idents and
/// TOML string literals compare equal.
fn extract_list(haystack: &str, start: &str, end: char) -> Vec<String> {
    let after_start = haystack
        .find(start)
        .unwrap_or_else(|| panic!("marker {start:?} not found"))
        + start.len();
    let rest = &haystack[after_start..];
    let body_len = rest
        .find(end)
        .unwrap_or_else(|| panic!("no closing {end:?} after {start:?}"));

    rest[..body_len]
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[test]
fn every_popover_command_is_allowed_by_the_acl() {
    let registered = extract_list(LIB_RS, "tauri::generate_handler![", ']');
    assert!(
        registered.len() > 5,
        "sanity check: parsed suspiciously few commands out of lib.rs — did the \
         generate_handler! formatting change? got {registered:?}"
    );

    let allowed = extract_list(APP_COMMANDS_TOML, "commands.allow = [", ']');

    // Reached from the *monitored dashboard's* origin (a remote page), not
    // the popover — these live in their own permission files instead, one
    // per command, granted only to the "monitor" window's capability.
    let remote_only = ["report_health", "report_usage"];

    let missing: Vec<&String> = registered
        .iter()
        .filter(|cmd| !remote_only.contains(&cmd.as_str()))
        .filter(|cmd| !allowed.contains(cmd))
        .collect();

    assert!(
        missing.is_empty(),
        "commands registered in generate_handler! but missing from \
         permissions/app-commands.toml's commands.allow — the popover's buttons for \
         these will silently fail at runtime: {missing:?}"
    );
}
