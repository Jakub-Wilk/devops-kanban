//! Translates "card X dropped on column Y / lane Z / between A and B" into ADO operations.
//!
//! Rules:
//! - Patch `BoardFields::column` (+ `done`, `lane`) — never `System.BoardColumn*` (read-only,
//!   TF401326).
//! - If the target column's `state_mappings[card.type]` differs from `card.state`, also patch
//!   `System.State`.
//! - Prefix every patch with `test /rev = card.rev`.
//! - A position change inside a column is a separate `workitemsorder` call.
//! - Validate the type is allowed in the column and warn on WIP-limit overflow.

use crate::card::CardId;

#[derive(Debug, Clone)]
pub struct MoveTarget {
    pub column_id: String,
    pub done: bool,
    pub lane_id: Option<String>,
    pub previous_id: Option<CardId>,
    pub next_id: Option<CardId>,
}

/// The operations a move expands to; executed by the app's service layer.
#[derive(Debug, Clone)]
pub struct MovePlan {
    pub card: CardId,
    pub expected_rev: u32,
    /// (field reference name, new value)
    pub field_changes: Vec<(String, serde_json::Value)>,
    pub reorder: Option<(Option<CardId>, Option<CardId>)>,
    pub warnings: Vec<String>,
}
