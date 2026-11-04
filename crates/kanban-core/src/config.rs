//! User settings: `config.toml` under `directories::ProjectDirs` + the PAT in the OS keyring
//! (service `devops-kanban`, account = organization). Env overrides `DEVOPS_KANBAN_ORG` and
//! `DEVOPS_KANBAN_PAT` take precedence (dev only). Schema: `docs/settings.md`.

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSettings {
    pub base_url: String,
    pub api_version: String,
    pub max_batch: usize,
}
