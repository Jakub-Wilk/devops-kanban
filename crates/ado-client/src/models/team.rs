//! DTOs for projects, teams, team settings, team field values and iterations.

use serde::{Deserialize, Serialize};

use super::FieldReference;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamDto {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// `GET work/teamsettings/teamfieldvalues`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamFieldValuesDto {
    pub field: FieldReference,
    #[serde(default)]
    pub default_value: Option<String>,
    #[serde(default)]
    pub values: Vec<TeamFieldValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamFieldValue {
    pub value: String,
    #[serde(default)]
    pub include_children: bool,
}

/// `GET work/teamsettings`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamSettingsDto {
    #[serde(default)]
    pub bugs_behavior: Option<String>,
    #[serde(default)]
    pub backlog_iteration: Option<IterationRef>,
    #[serde(default)]
    pub default_iteration: Option<IterationRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IterationRef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub path: Option<String>,
}

/// `GET work/teamsettings/iterations`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IterationDto {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub attributes: IterationAttributes,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IterationAttributes {
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub finish_date: Option<String>,
    /// `past` | `current` | `future`.
    #[serde(default)]
    pub time_frame: Option<String>,
}
