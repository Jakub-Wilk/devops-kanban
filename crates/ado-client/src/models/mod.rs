//! Serde DTOs mirroring ADO JSON (camelCase). Kept close to the wire format; the domain model
//! lives in `kanban-core`.

pub mod board;
pub mod identity;
pub mod patch;
pub mod team;
pub mod workitem;

use serde::{Deserialize, Serialize};

pub use board::*;
pub use identity::*;
pub use patch::*;
pub use team::*;
pub use workitem::*;

/// The `{ "count": n, "value": [...] }` envelope of list endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse<T> {
    #[serde(default)]
    pub count: Option<u32>,
    pub value: Vec<T>,
}

/// `{ "referenceName": "…" }` as used in board and team field descriptors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldReference {
    pub reference_name: String,
    #[serde(default)]
    pub url: Option<String>,
}
