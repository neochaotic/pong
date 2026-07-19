//! Builds the JavaScript evaluated inside the hidden webview.
//!
//! All user-controlled values (selectors, payload) cross into JS as a single
//! `serde_json`-encoded object literal, so escaping is handled by the serializer
//! rather than by hand-rolled string concatenation.

use crate::config::{Cleanup, Config, Selectors};

/// The probe agent, installed once per navigation via `initialization_script`.
pub const AGENT_SCRIPT: &str = include_str!("agent.js");

/// Parameters handed to the agent for a single check.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct InjectionParams {
    /// Host of the configured target. The agent refuses to act anywhere else.
    pub expected_host: String,
    pub selectors: Selectors,
    /// Optional post-check teardown, run only after a successful interaction.
    pub cleanup: Cleanup,
    pub payload: String,
    pub settle_ms: u64,
    pub typing_delay_ms: u64,
    /// How long the agent waits for an element to appear and become usable.
    pub element_timeout_ms: u64,
    /// Correlates the eventual report with this run; stale reports are dropped.
    pub nonce: u64,
}

impl InjectionParams {
    pub fn from_config(cfg: &Config, nonce: u64) -> Self {
        Self {
            expected_host: url::Url::parse(&cfg.target_url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_owned))
                .unwrap_or_default(),
            selectors: cfg.selectors.clone(),
            cleanup: cfg.cleanup.clone(),
            payload: cfg.payload.clone(),
            settle_ms: cfg.settle_ms,
            typing_delay_ms: cfg.typing_delay_ms,
            element_timeout_ms: cfg.element_timeout_ms,
            nonce,
        }
    }
}

/// Evaluate a full synthetic check (click, type, submit, settle, re-probe).
pub fn build_check_call(params: &InjectionParams) -> String {
    build_call("runCheck", params)
}

/// Evaluate a read-only heartbeat (DOM inspection only, no interaction).
pub fn build_heartbeat_call(params: &InjectionParams) -> String {
    build_call("heartbeat", params)
}

/// Wrap a method call so a missing agent still reports back instead of throwing.
fn build_call(method: &str, params: &InjectionParams) -> String {
    // Serialization of a plain struct of strings/numbers cannot fail.
    let json = serde_json::to_string(params).expect("injection params are serializable");
    format!(
        "(function(){{var p={json};\
         if(window.__PONG__){{window.__PONG__.{method}(p);}}\
         else{{try{{window.__TAURI_INTERNALS__.invoke('report_health',\
         {{payload:{{code:0,detail:'probe agent not installed',latency_ms:0,nonce:p.nonce}}}});\
         }}catch(e){{}}}}}})()"
    )
}

/// Evaluate the claude.ai usage-panel scraper. Independent of `InjectionParams`
/// — the scraper needs nothing but a nonce to correlate its report.
pub fn build_usage_call(nonce: u64) -> String {
    format!(
        "(function(){{var p={{nonce:{nonce}}};\
         if(window.__PONG__&&window.__PONG__.scrapeUsage){{window.__PONG__.scrapeUsage(p);}}\
         else{{try{{window.__TAURI_INTERNALS__.invoke('report_usage',\
         {{payload:{{session_percent:null,session_reset_text:null,\
         weekly_percent:null,weekly_reset_text:null,nonce:p.nonce}}}});\
         }}catch(e){{}}}}}})()"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params_with(payload: &str, text_input: &str) -> InjectionParams {
        InjectionParams {
            expected_host: "dash.internal".into(),
            selectors: Selectors {
                authenticated: "#dashboard-main".into(),
                login_indicator: "input[type=password]".into(),
                action_button: None,
                text_input: text_input.into(),
                submit_button: None,
                response: None,
            },
            cleanup: Cleanup::default(),
            payload: payload.into(),
            settle_ms: 3000,
            typing_delay_ms: 60,
            element_timeout_ms: 10_000,
            nonce: 42,
        }
    }

    #[test]
    fn agent_script_defines_the_global_namespace() {
        assert!(AGENT_SCRIPT.contains("window.__PONG__"));
        assert!(AGENT_SCRIPT.contains("runCheck"));
        assert!(AGENT_SCRIPT.contains("heartbeat"));
        assert!(AGENT_SCRIPT.contains("scrapeUsage"));
    }

    #[test]
    fn check_call_targets_run_check() {
        let js = build_check_call(&params_with("ping", "textarea"));
        assert!(js.contains("window.__PONG__.runCheck(p)"), "{js}");
    }

    #[test]
    fn heartbeat_call_targets_heartbeat() {
        let js = build_heartbeat_call(&params_with("ping", "textarea"));
        assert!(js.contains("window.__PONG__.heartbeat(p)"), "{js}");
    }

    #[test]
    fn carries_the_nonce_for_correlation() {
        let js = build_check_call(&params_with("ping", "textarea"));
        assert!(js.contains("\"nonce\":42"), "{js}");
    }

    #[test]
    fn escapes_quotes_in_the_payload() {
        let js = build_check_call(&params_with("say \"hi\"", "textarea"));
        // The raw unescaped sequence must never appear; the escaped one must.
        assert!(!js.contains("say \"hi\""), "{js}");
        assert!(js.contains(r#"say \"hi\""#), "{js}");
    }

    #[test]
    fn escapes_backslashes_and_newlines() {
        let js = build_check_call(&params_with("a\\b\nc", "textarea"));
        assert!(js.contains(r"a\\b\nc"), "{js}");
        assert!(
            !js.contains('\n'),
            "no literal newline may leak into the eval"
        );
    }

    #[test]
    fn escapes_selectors_containing_quotes() {
        let js = build_check_call(&params_with("ping", "textarea[data-x=\"y\"]"));
        assert!(js.contains(r#"textarea[data-x=\"y\"]"#), "{js}");
    }

    #[test]
    fn omits_action_button_when_unset() {
        let js = build_check_call(&params_with("ping", "textarea"));
        assert!(js.contains("\"action_button\":null"), "{js}");
    }

    #[test]
    fn includes_action_button_when_set() {
        let mut p = params_with("ping", "textarea");
        p.selectors.action_button = Some("#new-chat".into());
        let js = build_check_call(&p);
        assert!(js.contains(r##""action_button":"#new-chat""##), "{js}");
    }

    #[test]
    fn omits_response_when_unset() {
        let js = build_check_call(&params_with("ping", "textarea"));
        assert!(js.contains("\"response\":null"), "{js}");
    }

    #[test]
    fn includes_response_when_set() {
        let mut p = params_with("ping", "textarea");
        p.selectors.response = Some("[data-testid=\"assistant-message\"]".into());
        let js = build_check_call(&p);
        assert!(
            js.contains(r#""response":"[data-testid=\"assistant-message\"]""#),
            "{js}"
        );
    }

    #[test]
    fn omits_cleanup_fields_when_unset() {
        let js = build_check_call(&params_with("ping", "textarea"));
        assert!(
            js.contains(
                r#""cleanup":{"menu_button":null,"delete_option":null,"confirm_button":null}"#
            ),
            "{js}"
        );
    }

    #[test]
    fn includes_cleanup_fields_when_set() {
        let mut p = params_with("ping", "textarea");
        p.cleanup = Cleanup {
            menu_button: Some("[data-testid=\"page-header\"] button".into()),
            delete_option: Some("[data-testid=\"delete-chat-trigger\"]".into()),
            confirm_button: Some(".text-on-danger".into()),
        };
        let js = build_check_call(&p);
        assert!(
            js.contains(r#""delete_option":"[data-testid=\"delete-chat-trigger\"]""#),
            "{js}"
        );
        assert!(js.contains(r#""confirm_button":".text-on-danger""#), "{js}");
    }

    #[test]
    fn falls_back_to_reporting_when_agent_is_missing() {
        let js = build_check_call(&params_with("ping", "textarea"));
        assert!(js.contains("probe agent not installed"), "{js}");
        assert!(js.contains("code:0"), "{js}");
    }

    #[test]
    fn usage_call_targets_scrape_usage() {
        let js = build_usage_call(7);
        assert!(js.contains("window.__PONG__.scrapeUsage(p)"), "{js}");
        assert!(js.contains("nonce:7"), "{js}");
    }

    #[test]
    fn usage_call_falls_back_to_reporting_when_agent_is_missing() {
        let js = build_usage_call(1);
        assert!(js.contains("report_usage"), "{js}");
        assert!(js.contains("session_percent:null"), "{js}");
    }

    #[test]
    fn carries_the_target_host_so_the_agent_can_refuse_other_origins() {
        let cfg = Config::from_json(r##"{"target_url":"https://dash.internal/app"}"##).unwrap();
        let params = InjectionParams::from_config(&cfg, 1);
        assert_eq!(params.expected_host, "dash.internal");

        let js = build_check_call(&params);
        assert!(js.contains(r#""expected_host":"dash.internal""#), "{js}");
    }

    #[test]
    fn host_is_empty_when_the_url_cannot_be_parsed() {
        // Validation rejects such a URL long before this point; the agent
        // treats an empty host as "no restriction" rather than blocking itself.
        let cfg = Config {
            target_url: "::not a url::".into(),
            ..Config::default()
        };
        assert_eq!(InjectionParams::from_config(&cfg, 1).expected_host, "");
    }

    #[test]
    fn built_from_config_mirrors_the_config_values() {
        let cfg = Config::from_json(r##"{"payload":"hello","settle_ms":1500}"##).unwrap();
        let params = InjectionParams::from_config(&cfg, 7);
        assert_eq!(params.payload, "hello");
        assert_eq!(params.settle_ms, 1500);
        assert_eq!(params.nonce, 7);
        assert_eq!(params.selectors, cfg.selectors);
    }
}
