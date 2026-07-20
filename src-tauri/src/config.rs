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
    //
    // claude.ai, not some generic placeholder — Pong is marketed specifically as
    // a Claude.ai companion, so a fresh install pointing anywhere else (this used
    // to default to github.com/login) reads as broken rather than "not yet
    // configured for you."
    "https://claude.ai/new".to_string()
}
/// 5am, Monday through Friday. Six fields: sec min hour dom month dow.
///
/// A quiet default: paired with `cron_enabled` defaulting to `false`, a fresh
/// install runs nothing until the user opts in, and once they do, this is a
/// once-a-weekday-morning cadence rather than something that immediately
/// starts hammering the target every few minutes.
fn default_cron() -> String {
    "0 0 5 * * Mon-Fri".to_string()
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
    /// Optional selector matching each reply bubble (e.g. one per assistant
    /// turn). When set, a successful check waits for the *last* match's text
    /// to stop changing and reports it as the check's detail, instead of a
    /// generic "dashboard responded".
    #[serde(default)]
    pub response: Option<String>,
}

impl Default for Selectors {
    fn default() -> Self {
        Self {
            authenticated: default_authenticated(),
            login_indicator: default_login_indicator(),
            action_button: None,
            text_input: default_text_input(),
            submit_button: None,
            response: None,
        }
    }
}

/// Optional post-check teardown: deletes whatever the check just created
/// (e.g. a chat/conversation), so a monitor running every few minutes does
/// not silently fill the dashboard with check artifacts forever.
///
/// Each step only runs if its selector is set, in this order, so a dashboard
/// whose delete flow has no confirmation step can leave `confirm_button`
/// unset. All three are independent: a missing step is simply skipped, not
/// an error.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Cleanup {
    /// Opens the menu that holds the delete option (e.g. a conversation's
    /// "⋯" button).
    #[serde(default)]
    pub menu_button: Option<String>,
    /// The delete/remove option, inside that menu or standalone.
    #[serde(default)]
    pub delete_option: Option<String>,
    /// Confirms the destructive action in a follow-up dialog, if any.
    #[serde(default)]
    pub confirm_button: Option<String>,
}

impl Cleanup {
    /// Whether any step is configured at all.
    pub fn is_configured(&self) -> bool {
        self.menu_button.is_some() || self.delete_option.is_some() || self.confirm_button.is_some()
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
    /// Whether the cron schedule actually runs.
    ///
    /// Defaults to `false`: a fresh install (or a hand-edited config with a
    /// typo'd cron) should not start driving the target on a schedule until
    /// someone deliberately turns it on.
    #[serde(default)]
    pub cron_enabled: bool,
    #[serde(default)]
    pub selectors: Selectors,
    /// Optional post-check teardown (e.g. deleting a test conversation).
    #[serde(default)]
    pub cleanup: Cleanup,
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
    /// Whether Pong is registered to launch at login.
    ///
    /// Defaults to `true`: a tray-resident monitor that does not survive a
    /// reboot without the user remembering to relaunch it by hand mostly
    /// defeats the point. Kept in sync with the OS's actual registration on
    /// every launch and every save, rather than trusted blindly, so it never
    /// silently drifts from what's really registered.
    #[serde(default = "default_true")]
    pub autostart_enabled: bool,
    /// Whether a check drives the page or merely inspects it.
    ///
    /// Defaults to `probe_only` so a freshly installed app never types into
    /// whatever happens to be configured.
    #[serde(default = "default_interaction")]
    pub interaction: Interaction,
    /// Claude.ai's usage-limits page, e.g. `https://claude.ai/settings/usage`.
    ///
    /// Unset by default — this opts the popover's usage dashboard in. Separate
    /// from the generic check pipeline: `agent.js`'s scraper for this page is
    /// hardcoded to claude.ai's current DOM shape, not driven by selectors.
    #[serde(default)]
    pub usage_url: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            target_url: default_target_url(),
            cron: default_cron(),
            cron_enabled: false,
            selectors: Selectors::default(),
            cleanup: Cleanup::default(),
            payload: default_payload(),
            settle_ms: default_settle_ms(),
            element_timeout_ms: default_element_timeout_ms(),
            typing_delay_ms: default_typing_delay_ms(),
            notifications_enabled: true,
            autostart_enabled: true,
            interaction: default_interaction(),
            usage_url: None,
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

        if let Some(usage_url) = &self.usage_url {
            let url = url::Url::parse(usage_url).map_err(|e| ConfigError::Url {
                url: usage_url.clone(),
                reason: e.to_string(),
            })?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err(ConfigError::Url {
                    url: usage_url.clone(),
                    reason: format!("scheme `{}` is not http/https", url.scheme()),
                });
            }
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
            ("response", &self.selectors.response),
            ("cleanup.menu_button", &self.cleanup.menu_button),
            ("cleanup.delete_option", &self.cleanup.delete_option),
            ("cleanup.confirm_button", &self.cleanup.confirm_button),
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
