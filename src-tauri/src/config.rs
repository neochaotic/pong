//! Application configuration: on-disk `config.json` parsing and validation.
//!
//! The file is the single source of truth for *what* to monitor (the target URL),
//! *when* to monitor it (a 6-field cron expression) and *how* to drive the page
//! (CSS selectors). Everything has a default so a missing or `{}` file still boots.

use std::path::Path;
use std::str::FromStr;

/// Upper bound for the post-injection settle window.
const MAX_SETTLE_MS: u64 = 60_000;
/// Upper bound for the per-keystroke delay of the synthetic typist.
const MAX_TYPING_DELAY_MS: u64 = 2_000;
/// Upper bound for how long to wait on a single element.
const MAX_ELEMENT_TIMEOUT_MS: u64 = 120_000;

/// Errors produced while loading or validating a configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("config is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid cron expression `{expr}`: {reason}")]
    Cron { expr: String, reason: String },
    #[error("invalid target URL `{url}`: {reason}")]
    Url { url: String, reason: String },
    #[error("selector `{field}` must not be empty")]
    EmptySelector { field: &'static str },
    #[error("`{field}` must be between {min} and {max} (got {value})")]
    OutOfRange {
        field: &'static str,
        min: u64,
        max: u64,
        value: u64,
    },
}

fn default_target_url() -> String {
    // A real login page makes the out-of-the-box experience meaningful: sign in
    // once through the dashboard window and confirm the session survives a
    // restart. Paired with `Interaction::ProbeOnly`, nothing is ever typed.
    "https://github.com/login".to_string()
}
/// Every 15 minutes, on the second. Six fields: sec min hour dom month dow.
fn default_cron() -> String {
    "0 */15 * * * *".to_string()
}
fn default_authenticated() -> String {
    // Present only once GitHub has a session.
    "meta[name=\"user-login\"]".to_string()
}
fn default_login_indicator() -> String {
    "input[type=password]".to_string()
}
fn default_text_input() -> String {
    // Matches a plain textarea and a ProseMirror-style rich editor alike.
    "textarea, div[contenteditable=\"true\"]".to_string()
}
fn default_payload() -> String {
    "ping".to_string()
}
fn default_settle_ms() -> u64 {
    3_000
}
fn default_element_timeout_ms() -> u64 {
    10_000
}
fn default_typing_delay_ms() -> u64 {
    60
}
fn default_true() -> bool {
    true
}
fn default_interaction() -> Interaction {
    Interaction::ProbeOnly
}

/// How far a check should go once the session is confirmed alive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interaction {
    /// Click, type the payload and press Enter — a true synthetic transaction.
    #[default]
    Full,
    /// Only inspect the DOM for the auth/login markers. Nothing is clicked and
    /// nothing is typed.
    ///
    /// Use this whenever the target is not a scratch surface: typing into a
    /// real dashboard can post a comment, submit a form or otherwise mutate
    /// the account, once per cron tick, forever.
    ProbeOnly,
}

/// CSS selectors describing how to interact with the monitored dashboard.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Selectors {
    /// Present only when the session is authenticated (e.g. `#dashboard-main`).
    #[serde(default = "default_authenticated")]
    pub authenticated: String,
    /// Present only when the dashboard bounced us to a login screen.
    #[serde(default = "default_login_indicator")]
    pub login_indicator: String,
    /// Optional button clicked before typing (e.g. "new conversation").
    #[serde(default)]
    pub action_button: Option<String>,
    /// The text input / textarea that receives the synthetic payload.
    #[serde(default = "default_text_input")]
    pub text_input: String,
    /// Optional submit button. When set, the check waits for it to become
    /// enabled and clicks it instead of relying on the Enter key — which is
    /// what a React form with a disabled-until-valid button expects.
    #[serde(default)]
    pub submit_button: Option<String>,
}

impl Default for Selectors {
    fn default() -> Self {
        Self {
            authenticated: default_authenticated(),
            login_indicator: default_login_indicator(),
            action_button: None,
            text_input: default_text_input(),
            submit_button: None,
        }
    }
}

/// The full application configuration.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Config {
    /// Dashboard entry point loaded by the hidden webview.
    #[serde(default = "default_target_url")]
    pub target_url: String,
    /// Six-field cron expression driving the health checks.
    #[serde(default = "default_cron")]
    pub cron: String,
    #[serde(default)]
    pub selectors: Selectors,
    /// The string typed into `selectors.text_input` during a check.
    #[serde(default = "default_payload")]
    pub payload: String,
    /// How long to wait for the DOM to react after submitting.
    #[serde(default = "default_settle_ms")]
    pub settle_ms: u64,
    /// How long to wait for an element to appear and become interactive.
    ///
    /// A single-page app mounts asynchronously; querying once and giving up
    /// reports a healthy dashboard as broken.
    #[serde(default = "default_element_timeout_ms")]
    pub element_timeout_ms: u64,
    /// Delay between synthetic keystrokes.
    #[serde(default = "default_typing_delay_ms")]
    pub typing_delay_ms: u64,
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,
    /// Whether a check drives the page or merely inspects it.
    ///
    /// Defaults to `probe_only` so a freshly installed app never types into
    /// whatever happens to be configured.
    #[serde(default = "default_interaction")]
    pub interaction: Interaction,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            target_url: default_target_url(),
            cron: default_cron(),
            selectors: Selectors::default(),
            payload: default_payload(),
            settle_ms: default_settle_ms(),
            element_timeout_ms: default_element_timeout_ms(),
            typing_delay_ms: default_typing_delay_ms(),
            notifications_enabled: true,
            interaction: default_interaction(),
        }
    }
}

impl Config {
    /// Parse and validate a configuration from a JSON string.
    pub fn from_json(raw: &str) -> Result<Self, ConfigError> {
        let cfg: Config = serde_json::from_str(raw)?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Load a configuration from disk, seeding the file with defaults when absent.
    pub fn load_or_create(path: &Path) -> Result<Self, ConfigError> {
        if path.exists() {
            return Self::from_json(&std::fs::read_to_string(path)?);
        }

        let cfg = Config::default();
        cfg.save(path)?;
        Ok(cfg)
    }

    /// Persist the configuration as pretty-printed JSON, creating parent dirs.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Validate the already-populated struct.
    pub fn validate(&self) -> Result<(), ConfigError> {
        // The cron string must parse with the same crate the scheduler uses.
        cron::Schedule::from_str(&self.cron).map_err(|e| ConfigError::Cron {
            expr: self.cron.clone(),
            reason: e.to_string(),
        })?;

        let url = url::Url::parse(&self.target_url).map_err(|e| ConfigError::Url {
            url: self.target_url.clone(),
            reason: e.to_string(),
        })?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(ConfigError::Url {
                url: self.target_url.clone(),
                reason: format!("scheme `{}` is not http/https", url.scheme()),
            });
        }

        for (field, value) in [
            ("authenticated", &self.selectors.authenticated),
            ("login_indicator", &self.selectors.login_indicator),
            ("text_input", &self.selectors.text_input),
        ] {
            if value.trim().is_empty() {
                return Err(ConfigError::EmptySelector { field });
            }
        }
        for (field, value) in [
            ("action_button", &self.selectors.action_button),
            ("submit_button", &self.selectors.submit_button),
        ] {
            if let Some(selector) = value {
                if selector.trim().is_empty() {
                    return Err(ConfigError::EmptySelector { field });
                }
            }
        }

        check_range("settle_ms", self.settle_ms, 0, MAX_SETTLE_MS)?;
        check_range(
            "element_timeout_ms",
            self.element_timeout_ms,
            0,
            MAX_ELEMENT_TIMEOUT_MS,
        )?;
        check_range(
            "typing_delay_ms",
            self.typing_delay_ms,
            0,
            MAX_TYPING_DELAY_MS,
        )?;

        Ok(())
    }
}

fn check_range(field: &'static str, value: u64, min: u64, max: u64) -> Result<(), ConfigError> {
    if value < min || value > max {
        return Err(ConfigError::OutOfRange {
            field,
            min,
            max,
            value,
        });
    }
    Ok(())
}
