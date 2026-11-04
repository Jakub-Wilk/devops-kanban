//! User settings: `config.toml` under `directories::ProjectDirs` + secrets in the OS keyring
//! (service `devops-kanban`; account = organization for the PAT, `7pace:<organization>` for the
//! 7pace token). Env overrides `DEVOPS_KANBAN_ORG`, `DEVOPS_KANBAN_PAT` and
//! `DEVOPS_KANBAN_7PACE_TOKEN` take precedence (dev only). Schema: `docs/settings.md`.

use serde::{Deserialize, Serialize};

pub const KEYRING_SERVICE: &str = "devops-kanban";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub organization: String,
    pub default_project: Option<String>,
    pub default_team: Option<String>,
    pub default_board: Option<String>,
    pub poll_interval_secs: u64,
    pub cache_enabled: bool,
    pub log_level: String,
    pub api: ApiSettings,
    pub time_tracking: TimeTrackingSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSettings {
    pub base_url: String,
    pub api_version: String,
    pub max_batch: usize,
}

/// 7pace Timetracker. Off by default; needs a 7pace API token in the keyring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeTrackingSettings {
    pub enabled: bool,
    /// `https://{organization}.timehub.7pace.com`
    pub base_url: String,
    pub api_version: String,
}
