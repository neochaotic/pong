//! TDD suite for configuration parsing and validation.

use pongllm_lib::config::{Config, ConfigError};

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
fn applies_defaults_for_omitted_fields() {
    // Every field has a sane default so a bare `{}` still boots the app.
    let cfg = Config::from_json("{}").expect("empty object should fall back to defaults");

    assert_eq!(cfg.target_url, "https://example.com/login");
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
