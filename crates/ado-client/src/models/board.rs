//! DTOs for `work/boards`, `backlogconfiguration`, `cardsettings`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::FieldReference;

/// Entry of `GET work/boards`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardReference {
    pub id: String,
    pub name: String,
}

/// `GET work/boards/{board}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardDto {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub revision: u32,
    pub columns: Vec<BoardColumnDto>,
    #[serde(default)]
    pub rows: Vec<BoardRowDto>,
    pub fields: BoardFieldsDto,
    #[serde(default)]
    pub is_valid: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardColumnDto {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub item_limit: u32,
    #[serde(default)]
    pub state_mappings: HashMap<String, String>,
    #[serde(default)]
    pub is_split: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
    /// `incoming` | `inProgress` | `outgoing`.
    pub column_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardRowDto {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardFieldsDto {
    pub column_field: FieldReference,
    pub row_field: FieldReference,
    pub done_field: FieldReference,
}

/// `GET work/backlogconfiguration`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklogConfigurationDto {
    #[serde(default)]
    pub portfolio_backlogs: Vec<BacklogLevelDto>,
    pub requirement_backlog: BacklogLevelDto,
    pub task_backlog: BacklogLevelDto,
    #[serde(default)]
    pub work_item_type_mapped_states: Vec<WorkItemTypeStatesDto>,
    #[serde(default)]
    pub backlog_fields: BacklogFieldsDto,
    /// `asRequirements` | `asTasks` | `off`.
    #[serde(default)]
    pub bugs_behavior: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklogLevelDto {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub rank: u32,
    #[serde(default)]
    pub is_hidden: Option<bool>,
    #[serde(default)]
    pub work_item_types: Vec<WorkItemTypeRef>,
    #[serde(default)]
    pub default_work_item_type: Option<WorkItemTypeRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemTypeRef {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemTypeStatesDto {
    pub work_item_type_name: String,
    /// State name → category (`Proposed`, `InProgress`, `Resolved`, `Completed`, `Removed`).
    #[serde(default)]
    pub states: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklogFieldsDto {
    /// `Order`, `Effort`, `Team`, … → field reference name.
    #[serde(default)]
    pub type_fields: HashMap<String, String>,
}

/// `GET work/boards/{board}/cardsettings`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardSettingsDto {
    /// Work item type → fields shown on its cards.
    #[serde(default)]
    pub cards: HashMap<String, Vec<HashMap<String, serde_json::Value>>>,
}

impl CardSettingsDto {
    /// Every `fieldIdentifier` across all types, deduplicated.
    pub fn field_identifiers(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .cards
            .values()
            .flatten()
            .filter_map(|f| f.get("fieldIdentifier")?.as_str().map(str::to_owned))
            .collect();
        out.sort();
        out.dedup();
        out
    }
}
