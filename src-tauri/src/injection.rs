//! Builds the JavaScript evaluated inside the hidden webview.
//!
//! All user-controlled values (selectors, payload) cross into JS as a single
//! `serde_json`-encoded object literal, so escaping is handled by the serializer
//! rather than by hand-rolled string concatenation.

use crate::config::{Config, Selectors};

/// The probe agent, installed once per navigation via `initialization_script`.
pub const AGENT_SCRIPT: &str = include_str!("agent.js");

/// Parameters handed to the agent for a single check.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct InjectionParams {
    pub selectors: Selectors,
    pub payload: String,
    pub settle_ms: u64,
    pub typing_delay_ms: u64,
    /// Correlates the eventual report with this run; stale reports are dropped.
    pub nonce: u64,
}

impl InjectionParams {
    pub fn from_config(cfg: &Config, nonce: u64) -> Self {
        Self {
            selectors: cfg.selectors.clone(),
            payload: cfg.payload.clone(),
            settle_ms: cfg.settle_ms,
            typing_delay_ms: cfg.typing_delay_ms,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn params_with(payload: &str, text_input: &str) -> InjectionParams {
        InjectionParams {
            selectors: Selectors {
                authenticated: "#dashboard-main".into(),
                login_indicator: "input[type=password]".into(),
                action_button: None,
                text_input: text_input.into(),
            },
            payload: payload.into(),
            settle_ms: 3000,
            typing_delay_ms: 60,
            nonce: 42,
        }
    }

    #[test]
    fn agent_script_defines_the_global_namespace() {
        assert!(AGENT_SCRIPT.contains("window.__PONG__"));
        assert!(AGENT_SCRIPT.contains("runCheck"));
        assert!(AGENT_SCRIPT.contains("heartbeat"));
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
    fn falls_back_to_reporting_when_agent_is_missing() {
        let js = build_check_call(&params_with("ping", "textarea"));
        assert!(js.contains("probe agent not installed"), "{js}");
        assert!(js.contains("code:0"), "{js}");
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
