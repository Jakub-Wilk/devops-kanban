//! DTOs for work items (`id`, `rev`, `fields: Map<String, Value>`, `relations`), WIQL results,
//! batch requests, comments, work item type states.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemDto {
    pub id: u32,
    pub rev: u32,
    #[serde(default)]
    pub fields: Map<String, Value>,
    #[serde(default)]
    pub relations: Option<Vec<RelationDto>>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationDto {
    pub rel: String,
    pub url: String,
    #[serde(default)]
    pub attributes: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WiqlRequest {
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WiqlResponse {
    #[serde(default)]
    pub as_of: Option<String>,
    #[serde(default)]
    pub work_items: Vec<WorkItemRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItemRef {
    pub id: u32,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemsBatchRequest {
    pub ids: Vec<u32>,
    pub fields: Vec<String>,
    /// `omit` skips ids that no longer exist instead of failing the batch.
    pub error_policy: String,
}

/// `PATCH work/workitemsorder`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderRequest {
    pub ids: Vec<u32>,
    pub previous_id: u32,
    pub next_id: u32,
    pub parent_id: u32,
    pub iteration_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderResult {
    pub id: u32,
    pub order: f64,
}

/// `GET wit/workitemtypes/{type}/states`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemStateDto {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentDto {
    pub id: u32,
    pub text: String,
    #[serde(default)]
    pub created_date: Option<String>,
    #[serde(default)]
    pub created_by: Option<super::IdentityRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentList {
    #[serde(default)]
    pub comments: Vec<CommentDto>,
}
