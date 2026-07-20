//! TDD suite for configuration parsing and validation.

use pongllm_lib::config::{Config, ConfigError, Interaction};
use std::str::FromStr;

const FULL: &str = r##"{
  "target_url": "https://dash.internal/login",
  "cron": "0 */15 * * * *",
  "selectors": {
    "authenticated": "#dashboard-main",
    "login_indicator": "input[type=password]",
    "action_button": "#new-chat",
    "text_input": "textarea#prompt"
  },
  "payload": "ping",
  "settle_ms": 3000,
  "typing_delay_ms": 60,
  "notifications_enabled": true
}"##;

#[test]
fn parses_a_fully_specified_config() {
    let cfg = Config::from_json(FULL).expect("valid config should parse");

    assert_eq!(cfg.target_url, "https://dash.internal/login");
    assert_eq!(cfg.cron, "0 */15 * * * *");
    assert_eq!(cfg.selectors.authenticated, "#dashboard-main");
    assert_eq!(cfg.selectors.action_button.as_deref(), Some("#new-chat"));
    assert_eq!(cfg.selectors.text_input, "textarea#prompt");
    assert_eq!(cfg.payload, "ping");
    assert_eq!(cfg.settle_ms, 3000);
    assert!(cfg.notifications_enabled);
}

#[test]
fn cron_is_disabled_by_default() {
    // A fresh install (or a hand-edited config that dropped this field)
    // must not start driving the target on a schedule unasked.
    let cfg = Config::from_json("{}").unwrap();
    assert!(!cfg.cron_enabled);
}

#[test]
fn default_cron_is_weekday_mornings_and_parses() {
    let cfg = Config::from_json("{}").unwrap();
    assert_eq!(cfg.cron, "0 0 5 * * Mon-Fri");
    // The default must itself be schedulable, or a fresh install would
    // reject its own defaults the moment cron_enabled is switched on.
    assert!(cron::Schedule::from_str(&cfg.cron).is_ok());
}

#[test]
fn cron_enabled_can_be_configured() {
    let raw = r##"{"cron_enabled": true}"##;
    let cfg = Config::from_json(raw).unwrap();
    assert!(cfg.cron_enabled);
}

#[test]
fn autostart_is_enabled_by_default() {
    // A tray-resident monitor that doesn't come back after a reboot without
    // the user remembering to relaunch it mostly defeats the point.
    let cfg = Config::from_json("{}").unwrap();
    assert!(cfg.autostart_enabled);
}

#[test]
fn autostart_can_be_disabled() {
    let raw = r##"{"autostart_enabled": false}"##;
    let cfg = Config::from_json(raw).unwrap();
    assert!(!cfg.autostart_enabled);
}

#[test]
fn defaults_to_probe_only_so_a_fresh_install_never_types() {
    // The default target is a real account. Typing into it on a schedule could
    // post a comment or submit a form once per cron tick, forever.
    let cfg = Config::from_json("{}").unwrap();
    assert_eq!(cfg.interaction, Interaction::ProbeOnly);
    assert_eq!(cfg.target_url, "https://claude.ai/new");
}

#[test]
fn interaction_can_be_set_to_full() {
    let cfg = Config::from_json(r##"{"interaction":"full"}"##).unwrap();
    assert_eq!(cfg.interaction, Interaction::Full);
}

#[test]
fn rejects_an_unknown_interaction_mode() {
    assert!(Config::from_json(r##"{"interaction":"sometimes"}"##).is_err());
}

#[test]
fn default_text_input_matches_both_plain_and_rich_editors() {
    let cfg = Config::from_json("{}").unwrap();
    assert!(cfg.selectors.text_input.contains("textarea"));
    assert!(cfg.selectors.text_input.contains("contenteditable"));
}

#[test]
fn applies_defaults_for_omitted_fields() {
    // Every field has a sane default so a bare `{}` still boots the app.
    let cfg = Config::from_json("{}").expect("empty object should fall back to defaults");

    assert_eq!(cfg.target_url, "https://claude.ai/new");
    assert_eq!(cfg.payload, "ping");
    assert_eq!(cfg.settle_ms, 3000);
    assert!(!cfg.cron.is_empty());
    assert!(!cfg.selectors.authenticated.is_empty());
}

#[test]
fn action_button_is_optional() {
    let raw = r##"{ "selectors": { "authenticated": "#main", "login_indicator": "#login",
                    "action_button": null, "text_input": "textarea" } }"##;
    let cfg = Config::from_json(raw).expect("null action_button is allowed");
    assert_eq!(cfg.selectors.action_button, None);
}

#[test]
fn rejects_malformed_cron_expression() {
    let raw = r##"{ "cron": "not a cron" }"##;
    let err = Config::from_json(raw).expect_err("malformed cron must be rejected");
    assert!(matches!(err, ConfigError::Cron { .. }), "got {err:?}");
}

#[test]
fn rejects_cron_with_too_few_fields() {
    // tokio-cron-scheduler expects 6 (or 7) fields, including seconds.
    let raw = r##"{ "cron": "*/5 * * *" }"##;
    let err = Config::from_json(raw).expect_err("4-field cron must be rejected");
    assert!(matches!(err, ConfigError::Cron { .. }), "got {err:?}");
}

#[test]
fn rejects_non_http_target_url() {
    let raw = r##"{ "target_url": "file:///etc/passwd" }"##;
    let err = Config::from_json(raw).expect_err("non-http scheme must be rejected");
    assert!(matches!(err, ConfigError::Url { .. }), "got {err:?}");
}

#[test]
fn rejects_unparseable_target_url() {
    let raw = r##"{ "target_url": "not-a-url" }"##;
    let err = Config::from_json(raw).expect_err("garbage URL must be rejected");
    assert!(matches!(err, ConfigError::Url { .. }), "got {err:?}");
}

#[test]
fn rejects_empty_required_selector() {
    let raw = r##"{ "selectors": { "authenticated": "", "login_indicator": "#login",
                    "text_input": "textarea" } }"##;
    let err = Config::from_json(raw).expect_err("empty selector must be rejected");
    assert!(
        matches!(err, ConfigError::EmptySelector { field } if field == "authenticated"),
        "got {err:?}"
    );
}

#[test]
fn submit_button_is_optional_and_defaults_to_none() {
    let cfg = Config::from_json("{}").unwrap();
    assert_eq!(cfg.selectors.submit_button, None);
}

#[test]
fn submit_button_can_be_configured() {
    let raw = r##"{"selectors":{"submit_button":"button[type=submit]"}}"##;
    let cfg = Config::from_json(raw).unwrap();
    assert_eq!(
        cfg.selectors.submit_button.as_deref(),
        Some("button[type=submit]")
    );
}

#[test]
fn rejects_an_empty_submit_button_selector() {
    let raw = r##"{"selectors":{"submit_button":"  "}}"##;
    let err = Config::from_json(raw).expect_err("blank selector must be rejected");
    assert!(
        matches!(err, ConfigError::EmptySelector { field } if field == "submit_button"),
        "got {err:?}"
    );
}

#[test]
fn response_is_optional_and_defaults_to_none() {
    let cfg = Config::from_json("{}").unwrap();
    assert_eq!(cfg.selectors.response, None);
}

#[test]
fn response_can_be_configured() {
    let raw = r##"{"selectors":{"response":"[data-testid=\"assistant-message\"]"}}"##;
    let cfg = Config::from_json(raw).unwrap();
    assert_eq!(
        cfg.selectors.response.as_deref(),
        Some(r##"[data-testid="assistant-message"]"##)
    );
}

#[test]
fn rejects_an_empty_response_selector() {
    let raw = r##"{"selectors":{"response":"  "}}"##;
    let err = Config::from_json(raw).expect_err("blank selector must be rejected");
    assert!(
        matches!(err, ConfigError::EmptySelector { field } if field == "response"),
        "got {err:?}"
    );
}

#[test]
fn cleanup_is_optional_and_defaults_to_unconfigured() {
    let cfg = Config::from_json("{}").unwrap();
    assert_eq!(cfg.cleanup.menu_button, None);
    assert_eq!(cfg.cleanup.delete_option, None);
    assert_eq!(cfg.cleanup.confirm_button, None);
    assert!(!cfg.cleanup.is_configured());
}

#[test]
fn cleanup_can_be_partially_configured() {
    let raw = r##"{"cleanup":{"delete_option":"[data-testid=\"delete-chat-trigger\"]"}}"##;
    let cfg = Config::from_json(raw).unwrap();
    assert_eq!(cfg.cleanup.menu_button, None);
    assert_eq!(
        cfg.cleanup.delete_option.as_deref(),
        Some(r##"[data-testid="delete-chat-trigger"]"##)
    );
    assert!(cfg.cleanup.is_configured());
}

#[test]
fn rejects_an_empty_cleanup_selector() {
    let raw = r##"{"cleanup":{"menu_button":"  "}}"##;
    let err = Config::from_json(raw).expect_err("blank selector must be rejected");
    assert!(
        matches!(err, ConfigError::EmptySelector { field } if field == "cleanup.menu_button"),
        "got {err:?}"
    );
}

#[test]
fn usage_url_defaults_to_claude_ai_usage_page() {
    // DASH (the usage dashboard) is the popover's primary view — leaving it
    // dark by default defeated the point for the Claude.ai audience Pong is
    // built for. Explicit `null` still opts out.
    let cfg = Config::from_json("{}").unwrap();
    assert_eq!(
        cfg.usage_url.as_deref(),
        Some("https://claude.ai/settings/usage")
    );
}

#[test]
fn usage_url_can_be_explicitly_disabled() {
    let cfg = Config::from_json(r##"{"usage_url": null}"##).unwrap();
    assert_eq!(cfg.usage_url, None);
}

#[test]
fn usage_url_can_be_configured() {
    let raw = r##"{"usage_url":"https://claude.ai/settings/usage"}"##;
    let cfg = Config::from_json(raw).unwrap();
    assert_eq!(
        cfg.usage_url.as_deref(),
        Some("https://claude.ai/settings/usage")
    );
}

#[test]
fn rejects_a_non_http_usage_url() {
    let raw = r##"{"usage_url":"file:///etc/passwd"}"##;
    let err = Config::from_json(raw).expect_err("non-http usage_url must be rejected");
    assert!(matches!(err, ConfigError::Url { .. }), "got {err:?}");
}

#[test]
fn element_timeout_defaults_to_ten_seconds() {
    // A single-page app mounts asynchronously; one shot at querySelector would
    // report a healthy dashboard as broken.
    assert_eq!(Config::from_json("{}").unwrap().element_timeout_ms, 10_000);
}

#[test]
fn rejects_an_absurd_element_timeout() {
    let raw = r##"{"element_timeout_ms": 900000}"##;
    let err = Config::from_json(raw).expect_err("above the ceiling");
    assert!(matches!(err, ConfigError::OutOfRange { field, .. } if field == "element_timeout_ms"));
}

#[test]
fn rejects_absurd_settle_window() {
    let raw = r##"{ "settle_ms": 600000 }"##;
    let err = Config::from_json(raw).expect_err("settle_ms above the ceiling must be rejected");
    assert!(matches!(err, ConfigError::OutOfRange { field, .. } if field == "settle_ms"));
}

#[test]
fn rejects_invalid_json() {
    let err = Config::from_json("{ definitely not json").expect_err("bad JSON must be rejected");
    assert!(matches!(err, ConfigError::Json(_)), "got {err:?}");
}

#[test]
fn survives_a_serialize_parse_roundtrip() {
    let original = Config::from_json(FULL).unwrap();
    let encoded = serde_json::to_string_pretty(&original).unwrap();
    let decoded = Config::from_json(&encoded).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn load_or_create_writes_defaults_when_file_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");

    let cfg = Config::load_or_create(&path).expect("missing file should seed defaults");

    assert!(
        path.exists(),
        "config.json should have been created on disk"
    );
    let reloaded = Config::load_or_create(&path).expect("second load reads the written file");
    assert_eq!(cfg, reloaded, "seeding then reloading must be stable");
}

#[test]
fn load_or_create_reads_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(&path, FULL).unwrap();

    let cfg = Config::load_or_create(&path).unwrap();
    assert_eq!(cfg.target_url, "https://dash.internal/login");
}

#[test]
fn load_or_create_migrates_a_stale_v0_0_1_github_default_to_claude_ai() {
    // v0.0.1 shipped target_url/authenticated defaulted to GitHub's login
    // page. A version bump alone never touches an existing config.json, so
    // anyone who installed before the fix is stuck on it forever without
    // this migration.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(
        &path,
        r##"{
          "target_url": "https://github.com/login",
          "selectors": { "authenticated": "meta[name=\"user-login\"]" }
        }"##,
    )
    .unwrap();

    let cfg = Config::load_or_create(&path).expect("stale config should still load");

    assert_eq!(cfg.target_url, "https://claude.ai/new");
    assert_eq!(
        cfg.selectors.authenticated,
        "[data-testid=\"user-menu-button\"]"
    );

    // The fix must be persisted, not just applied in memory — reloading from
    // a fresh Config::load_or_create call reads the file straight off disk.
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(
        raw.contains("claude.ai/new"),
        "migration was not saved: {raw}"
    );
}

#[test]
fn load_or_create_leaves_a_genuinely_customized_target_url_alone() {
    // The migration must not clobber a real customization — a target_url
    // that was never the GitHub default is left exactly as the user set it.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(
        &path,
        r##"{
          "target_url": "https://dash.internal/login",
          "selectors": { "authenticated": "meta[name=\"user-login\"]" }
        }"##,
    )
    .unwrap();

    let cfg = Config::load_or_create(&path).unwrap();

    assert_eq!(cfg.target_url, "https://dash.internal/login");
}

#[test]
fn load_or_create_migrates_authenticated_even_when_target_url_was_already_hand_fixed() {
    // A user who followed an earlier, incomplete workaround (pointing
    // target_url at claude.ai by hand, before the selector fix shipped)
    // must not be permanently excluded from the selector migration just
    // because target_url no longer equals the old default. Each field
    // migrates on its own.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(
        &path,
        r##"{
          "target_url": "https://claude.ai/new",
          "selectors": { "authenticated": "meta[name=\"user-login\"]" }
        }"##,
    )
    .unwrap();

    let cfg = Config::load_or_create(&path).unwrap();

    assert_eq!(cfg.target_url, "https://claude.ai/new");
    assert_eq!(
        cfg.selectors.authenticated,
        "[data-testid=\"user-menu-button\"]"
    );
}

#[test]
fn load_or_create_enables_the_usage_dashboard_once_for_an_existing_install() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(&path, r##"{"usage_url": null}"##).unwrap();

    let cfg = Config::load_or_create(&path).expect("existing config should still load");

    assert_eq!(
        cfg.usage_url.as_deref(),
        Some("https://claude.ai/settings/usage"),
        "an existing install with the dashboard never configured should get it enabled once"
    );
}

#[test]
fn load_or_create_never_re_enables_a_deliberately_disabled_usage_dashboard() {
    // The one-time migration must not become a standing rule that fights a
    // user who explicitly turns the dashboard back off later.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(&path, r##"{"usage_url": null}"##).unwrap();

    // First load: the one-time migration enables it and writes the marker.
    let first = Config::load_or_create(&path).unwrap();
    assert!(first.usage_url.is_some());

    // The user turns it back off by hand and saves.
    let mut turned_off = first;
    turned_off.usage_url = None;
    turned_off.save(&path).unwrap();

    // A later load must respect that choice, not re-enable it.
    let second = Config::load_or_create(&path).unwrap();
    assert_eq!(second.usage_url, None);
}
