//! User settings: `config.toml` under `directories::ProjectDirs` + secrets in the OS keyring
//! (service `devops-kanban`; account = organization for the PAT, `7pace:<organization>` for the
//! 7pace token). Env overrides `DEVOPS_KANBAN_ORG`, `DEVOPS_KANBAN_PAT` and
//! `DEVOPS_KANBAN_7PACE_TOKEN` take precedence (dev only). Schema: `docs/settings.md`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

pub const KEYRING_SERVICE: &str = "devops-kanban";
const MIN_POLL_SECS: u64 = 10;
/// `one_off_tag` until the user changes it.
pub const DEFAULT_ONE_OFF_TAG: &str = "Ad-Hoc";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub organization: String,
    pub default_project: Option<String>,
    pub default_team: Option<String>,
    pub default_board: Option<String>,
    pub poll_interval_secs: u64,
    pub cache_enabled: bool,
    pub log_level: String,
    /// Last tab shown on the board screen: `list`, `kanban` or `timeline`.
    pub last_view: Option<String>,
    /// Width in px of the Timeline's item list (the separator position).
    pub timeline_info_width: Option<u32>,
    /// The window was maximised when last used; the next launch opens maximised.
    pub window_maximized: bool,
    /// The window's normal (not maximised) inner size, logical px. Written by the app.
    pub window_size: Option<[u32; 2]>,
    /// The inner size it had when maximised, logical px. A window that opens maximised is
    /// created at this size, so its webview never draws at the normal size first.
    pub maximized_size: Option<[u32; 2]>,
    /// Tag marking one-off work items: with it set, the item dialog offers a checkbox that adds
    /// the tag and creates the item without a parent (the only way to do so). Empty = off.
    pub one_off_tag: String,
    pub api: ApiSettings,
    pub time_tracking: TimeTrackingSettings,
    /// How List rows and kanban cards are drawn (*View options* next to the board tabs).
    pub appearance: AppearanceSettings,
    /// Per-board UI preferences, keyed `"<project>/<team>/<board>"`.
    pub boards: BTreeMap<String, BoardPrefs>,
    /// Per-team filters of the List and Timeline tabs, keyed `"<project>/<team>"`.
    pub views: BTreeMap<String, ViewPrefs>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiSettings {
    pub base_url: String,
    pub api_version: String,
    pub max_batch: usize,
}

/// 7pace Timetracker. Off by default; needs a 7pace API token in the keyring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimeTrackingSettings {
    pub enabled: bool,
    /// `https://{organization}.timehub.7pace.com`
    pub base_url: String,
    pub api_version: String,
}

/// Which edge of a row or card carries the 4 px accent bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccentSide {
    #[default]
    Left,
    Right,
    Off,
}

impl AccentSide {
    pub const ALL: [Self; 3] = [Self::Left, Self::Right, Self::Off];

    /// CSS class for the bar, or `""` when off.
    pub fn class(self) -> &'static str {
        match self {
            Self::Left => "acc-left",
            Self::Right => "acc-right",
            Self::Off => "acc-off",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Off => "Off",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    pub list: ListStyle,
    pub kanban: CardStyle,
    /// Timeline rows (their info column), with the List's options.
    pub timeline: ListStyle,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            list: ListStyle::default(),
            kanban: CardStyle::default(),
            // The timeline's original look: plain rows.
            timeline: ListStyle {
                tint: false,
                accent: AccentSide::Off,
                icon: false,
            },
        }
    }
}

/// List rows. The tint and the accent bar use the work item type's colour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ListStyle {
    /// Lean the row background towards the type colour.
    pub tint: bool,
    pub accent: AccentSide,
    /// The type's icon (from Azure DevOps) before the title.
    pub icon: bool,
}

impl Default for ListStyle {
    fn default() -> Self {
        Self {
            tint: true,
            accent: AccentSide::Left,
            icon: false,
        }
    }
}

/// Kanban cards. The accent bar and the tint use the card's state colour.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CardStyle {
    pub accent: AccentSide,
    /// Lean each card's background towards its state colour (the accent bar's colour).
    pub tint: bool,
    /// The column under the mouse widens and the others narrow (same total width).
    pub hover_widen: bool,
    /// Cards open while hovered, and a pin (instead of the expand chevron) keeps a card open at
    /// the top of its column.
    pub hover_open: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoardPrefs {
    /// Column names hidden on the kanban.
    pub hidden_columns: Vec<String>,
    /// Epic the kanban is filtered to.
    pub epic: Option<u32>,
    /// Assignee filter of the kanban (see [`TabFilters::assignees`]).
    pub assignees: Vec<String>,
    /// Area filter of the kanban (see [`TabFilters::areas`]).
    pub areas: Vec<String>,
    /// Cards pinned open at the top of their column (`appearance.kanban.hover_open`).
    pub pinned: Vec<u32>,
    /// Filters switched off: kept, but not applied (see [`BoardPrefs::applied`]).
    pub epic_off: bool,
    pub assignees_off: bool,
    pub areas_off: bool,
}

/// Filters of one team's board screen. The search box is not saved.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewPrefs {
    /// Iteration path from the header picker, shared by every tab.
    pub iteration: Option<String>,
    pub list: TabFilters,
    pub timeline: TabFilters,
    /// Order of the List's rows within each parent.
    pub list_sort: ListSort,
}

/// The List's order. Only `Backlog` (ADO's rank) allows drag-to-reorder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListSort {
    #[default]
    Backlog,
    TargetDate,
    Priority,
}

impl ListSort {
    pub const ALL: [Self; 3] = [Self::Backlog, Self::TargetDate, Self::Priority];

    pub fn label(self) -> &'static str {
        match self {
            Self::Backlog => "Backlog order",
            Self::TargetDate => "Target date",
            Self::Priority => "Priority",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::TargetDate => "target_date",
            Self::Priority => "priority",
        }
    }
}

/// The filter bar of the List or the Timeline. Empty lists mean "no filter". Each `*_off` switches
/// its filter off without losing the selection (see [`TabFilters::applied`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabFilters {
    pub epic_states: Vec<String>,
    /// `@me`, `@unassigned` or a `unique_name` (see [`crate::filters`]).
    pub assignees: Vec<String>,
    /// Area paths; each also matches the areas below it.
    pub areas: Vec<String>,
    pub item_states: Vec<String>,
    pub epic_states_off: bool,
    pub assignees_off: bool,
    pub areas_off: bool,
    pub item_states_off: bool,
}

impl Settings {
    /// The one-off tag, trimmed; `None` when turned off (blank).
    pub fn one_off_tag(&self) -> Option<&str> {
        Some(self.one_off_tag.trim()).filter(|t| !t.is_empty())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            organization: String::new(),
            default_project: None,
            default_team: None,
            default_board: None,
            poll_interval_secs: 30,
            cache_enabled: true,
            log_level: "info".into(),
            last_view: None,
            timeline_info_width: None,
            window_maximized: false,
            window_size: None,
            maximized_size: None,
            api: ApiSettings::default(),
            time_tracking: TimeTrackingSettings::default(),
            appearance: AppearanceSettings::default(),
            one_off_tag: DEFAULT_ONE_OFF_TAG.into(),
            boards: BTreeMap::new(),
            views: BTreeMap::new(),
        }
    }
}

impl Default for ApiSettings {
    fn default() -> Self {
        Self {
            base_url: "https://dev.azure.com".into(),
            api_version: "7.1".into(),
            max_batch: 200,
        }
    }
}

impl Default for TimeTrackingSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::new(),
            api_version: "3.2".into(),
        }
    }
}

pub fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("tv", "media-press", "devops-kanban")
}

impl Settings {
    pub fn path() -> Option<PathBuf> {
        project_dirs().map(|d| d.config_dir().join("config.toml"))
    }

    /// Loads `config.toml` (defaults when absent) and applies env overrides.
    pub fn load() -> Result<Self, CoreError> {
        let mut settings = match Self::path() {
            Some(path) if path.exists() => Self::from_toml(&std::fs::read_to_string(path)?)?,
            _ => Self::default(),
        };
        if let Ok(org) = std::env::var("DEVOPS_KANBAN_ORG")
            && !org.is_empty()
        {
            settings.organization = org;
        }
        Ok(settings)
    }

    pub fn from_toml(s: &str) -> Result<Self, CoreError> {
        let mut settings: Self = toml::from_str(s).map_err(|e| CoreError::Config(e.to_string()))?;
        settings.normalize();
        Ok(settings)
    }

    pub fn to_toml(&self) -> Result<String, CoreError> {
        toml::to_string_pretty(self).map_err(|e| CoreError::Config(e.to_string()))
    }

    pub fn save(&self) -> Result<(), CoreError> {
        let path = Self::path().ok_or_else(|| CoreError::Config("no config directory".into()))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_toml()?)?;
        Ok(())
    }

    fn normalize(&mut self) {
        self.poll_interval_secs = self.poll_interval_secs.max(MIN_POLL_SECS);
        self.api.max_batch = self.api.max_batch.clamp(1, 200);
        if self.time_tracking.base_url.is_empty() && !self.organization.is_empty() {
            self.time_tracking.base_url =
                format!("https://{}.timehub.7pace.com", self.organization);
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.organization.trim().is_empty()
    }

    pub fn board_key(project: &str, team: &str, board: &str) -> String {
        format!("{project}/{team}/{board}")
    }

    pub fn board_prefs(&self, project: &str, team: &str, board: &str) -> BoardPrefs {
        self.boards
            .get(&Self::board_key(project, team, board))
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_board_prefs(&mut self, project: &str, team: &str, board: &str, prefs: BoardPrefs) {
        let key = Self::board_key(project, team, board);
        if prefs == BoardPrefs::default() {
            self.boards.remove(&key);
        } else {
            self.boards.insert(key, prefs);
        }
    }

    pub fn view_key(project: &str, team: &str) -> String {
        format!("{project}/{team}")
    }

    pub fn view_prefs(&self, project: &str, team: &str) -> ViewPrefs {
        self.views
            .get(&Self::view_key(project, team))
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_view_prefs(&mut self, project: &str, team: &str, prefs: ViewPrefs) {
        let key = Self::view_key(project, team);
        if prefs == ViewPrefs::default() {
            self.views.remove(&key);
        } else {
            self.views.insert(key, prefs);
        }
    }
}

/// The ADO PAT: `DEVOPS_KANBAN_PAT`, else the keyring. `Ok(None)` when not stored.
pub fn load_pat(organization: &str) -> Result<Option<String>, CoreError> {
    if let Ok(pat) = std::env::var("DEVOPS_KANBAN_PAT")
        && !pat.is_empty()
    {
        return Ok(Some(pat));
    }
    keyring_get(organization)
}

/// Stores the PAT in the keyring. Fails (and stores nothing) without a keyring backend.
pub fn store_pat(organization: &str, pat: &str) -> Result<(), CoreError> {
    keyring_entry(organization)?
        .set_password(pat)
        .map_err(|e| CoreError::Keyring(e.to_string()))
}

pub fn delete_pat(organization: &str) -> Result<(), CoreError> {
    match keyring_entry(organization)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(CoreError::Keyring(e.to_string())),
    }
}

/// Keyring account of the 7pace API token.
fn sevenpace_account(organization: &str) -> String {
    format!("7pace:{organization}")
}

/// The 7pace API token: `DEVOPS_KANBAN_7PACE_TOKEN`, else the keyring. `Ok(None)` when not stored.
pub fn load_7pace_token(organization: &str) -> Result<Option<String>, CoreError> {
    if let Ok(token) = std::env::var("DEVOPS_KANBAN_7PACE_TOKEN")
        && !token.is_empty()
    {
        return Ok(Some(token));
    }
    keyring_get(&sevenpace_account(organization))
}

/// Stores the 7pace token in the keyring. Fails (and stores nothing) without a keyring backend.
pub fn store_7pace_token(organization: &str, token: &str) -> Result<(), CoreError> {
    keyring_entry(&sevenpace_account(organization))?
        .set_password(token)
        .map_err(|e| CoreError::Keyring(e.to_string()))
}

pub fn delete_7pace_token(organization: &str) -> Result<(), CoreError> {
    match keyring_entry(&sevenpace_account(organization))?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(CoreError::Keyring(e.to_string())),
    }
}

fn keyring_entry(account: &str) -> Result<keyring::Entry, CoreError> {
    keyring::Entry::new(KEYRING_SERVICE, account).map_err(|e| CoreError::Keyring(e.to_string()))
}

fn keyring_get(account: &str) -> Result<Option<String>, CoreError> {
    match keyring_entry(account)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(CoreError::Keyring(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_config_parses() {
        let s = Settings::from_toml(include_str!("../../../config.example.toml")).unwrap();
        assert_eq!(s.organization, "media-press");
        assert_eq!(s.api.api_version, "7.1");
        assert_eq!(s.poll_interval_secs, 30);
    }

    #[test]
    fn defaults_and_clamps() {
        let s = Settings::from_toml(
            "organization = \"x\"\npoll_interval_secs = 2\n[api]\nmax_batch = 999",
        )
        .unwrap();
        assert_eq!(s.poll_interval_secs, 10);
        assert_eq!(s.api.max_batch, 200);
        assert!(s.cache_enabled);
        assert_eq!(s.time_tracking.base_url, "https://x.timehub.7pace.com");
    }

    #[test]
    fn appearance_defaults_and_round_trip() {
        // Configs written before the setting existed keep today's look.
        let old = Settings::from_toml("organization = \"x\"").unwrap();
        assert_eq!(old.appearance, AppearanceSettings::default());
        assert!(old.appearance.list.tint);
        assert_eq!(old.appearance.list.accent, AccentSide::Left);
        assert!(!old.appearance.list.icon);
        assert_eq!(old.appearance.kanban.accent, AccentSide::Left);
        assert!(!old.appearance.kanban.tint);
        assert!(!old.appearance.timeline.tint);
        assert_eq!(old.appearance.timeline.accent, AccentSide::Off);
        assert!(!old.appearance.kanban.hover_widen);
        assert!(!old.appearance.kanban.hover_open);

        let s = Settings::from_toml(
            "organization = \"x\"\n[appearance.list]\naccent = \"right\"\nicon = true\n[appearance.kanban]\naccent = \"off\"",
        )
        .unwrap();
        assert_eq!(s.appearance.list.accent, AccentSide::Right);
        assert!(s.appearance.list.icon);
        assert!(s.appearance.list.tint);
        assert_eq!(s.appearance.kanban.accent, AccentSide::Off);
        let back = Settings::from_toml(&s.to_toml().unwrap()).unwrap();
        assert_eq!(back.appearance, s.appearance);
    }

    #[test]
    fn one_off_tag() {
        // Configs without the key get the default.
        let old = Settings::from_toml("organization = \"x\"").unwrap();
        assert_eq!(old.one_off_tag(), Some("Ad-Hoc"));
        let mut s = old.clone();
        s.one_off_tag = " one-off ".into();
        let back = Settings::from_toml(&s.to_toml().unwrap()).unwrap();
        assert_eq!(back.one_off_tag(), Some("one-off"));
        // Turned off stays off after a save and reload.
        s.one_off_tag = String::new();
        let back = Settings::from_toml(&s.to_toml().unwrap()).unwrap();
        assert_eq!(back.one_off_tag(), None);
    }

    #[test]
    fn hidden_columns_round_trip() {
        let mut s = Settings {
            organization: "o".into(),
            ..Settings::default()
        };
        s.set_board_prefs(
            "IT Finance",
            "IT Finance Team",
            "Stories",
            BoardPrefs {
                hidden_columns: vec!["Closed".into(), "TEST-FAILED".into()],
                epic: Some(118181),
                assignees: vec!["@me".into()],
                areas: vec!["IT Finance\\Penny".into()],
                pinned: vec![117349],
                assignees_off: true,
                ..BoardPrefs::default()
            },
        );
        let back = Settings::from_toml(&s.to_toml().unwrap()).unwrap();
        assert_eq!(
            back.board_prefs("IT Finance", "IT Finance Team", "Stories")
                .pinned,
            vec![117349]
        );
        assert_eq!(
            back.board_prefs("IT Finance", "IT Finance Team", "Stories")
                .hidden_columns,
            vec!["Closed", "TEST-FAILED"]
        );
        assert_eq!(
            back.board_prefs("IT Finance", "IT Finance Team", "Stories")
                .epic,
            Some(118181)
        );
        s.set_board_prefs(
            "IT Finance",
            "IT Finance Team",
            "Stories",
            BoardPrefs::default(),
        );
        assert!(s.boards.is_empty());
    }

    #[test]
    fn view_prefs_round_trip() {
        let mut s = Settings {
            organization: "o".into(),
            ..Settings::default()
        };
        let prefs = ViewPrefs {
            iteration: Some("IT Finance\\Sprint 1".into()),
            list: TabFilters {
                epic_states: vec!["New".into(), "Active".into()],
                assignees: vec!["@me".into(), "@unassigned".into()],
                areas: vec!["IT Finance\\Rozwój".into()],
                item_states: vec!["Blocked".into()],
                areas_off: true,
                ..TabFilters::default()
            },
            timeline: TabFilters {
                assignees: vec!["@me".into()],
                ..TabFilters::default()
            },
            list_sort: ListSort::TargetDate,
        };
        s.set_view_prefs("IT Finance", "IT Finance Team", prefs.clone());
        let back = Settings::from_toml(&s.to_toml().unwrap()).unwrap();
        assert_eq!(back.view_prefs("IT Finance", "IT Finance Team"), prefs);
        s.set_view_prefs("IT Finance", "IT Finance Team", ViewPrefs::default());
        assert!(s.views.is_empty());
    }
}
