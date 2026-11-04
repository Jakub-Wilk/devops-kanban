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

use ado_client::PatchDocument;
use serde_json::{Value, json};

use crate::board::{Board, DEFAULT_LANE_ID};
use crate::card::{Card, CardId, fields};

#[derive(Debug, Clone, PartialEq)]
pub struct MoveTarget {
    pub column_id: String,
    pub done: bool,
    pub lane_id: Option<String>,
    pub previous_id: Option<CardId>,
    pub next_id: Option<CardId>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MoveError {
    #[error("column {0} is not on this board")]
    UnknownColumn(String),
    #[error("{work_item_type} can't be placed in column {column}")]
    TypeNotAllowed {
        work_item_type: String,
        column: String,
    },
}

/// The operations a move expands to; executed by the app's service layer.
#[derive(Debug, Clone, PartialEq)]
pub struct MovePlan {
    pub card: CardId,
    pub expected_rev: u32,
    /// (field reference name, new value)
    pub field_changes: Vec<(String, Value)>,
    pub reorder: Option<(Option<CardId>, Option<CardId>)>,
    pub warnings: Vec<String>,
    /// What the card looks like after the move, for the optimistic update.
    pub target_column: String,
    pub target_done: bool,
    pub target_lane: Option<String>,
    pub new_state: Option<String>,
}

impl MovePlan {
    pub fn new(board: &Board, card: &Card, target: &MoveTarget) -> Result<Self, MoveError> {
        let column = board
            .column(&target.column_id)
            .ok_or_else(|| MoveError::UnknownColumn(target.column_id.clone()))?;
        let mapped_state = column
            .state_mappings
            .get(&card.work_item_type)
            .ok_or_else(|| MoveError::TypeNotAllowed {
                work_item_type: card.work_item_type.clone(),
                column: column.name.clone(),
            })?;

        let current_column = board.column_of(card).map(|c| c.id.as_str());
        let column_changes = current_column != Some(column.id.as_str());
        let done = column.is_split && target.done;
        let mut field_changes = Vec::new();
        if column_changes {
            field_changes.push((board.fields.column.clone(), json!(column.name)));
        }
        if column.is_split && (column_changes || done != card.board_column_done) {
            field_changes.push((board.fields.done.clone(), json!(done)));
        }

        let current_lane = board.lane_of(card);
        let target_lane_id = target
            .lane_id
            .clone()
            .unwrap_or_else(|| current_lane.clone());
        let lane = board.lanes.iter().find(|l| l.id == target_lane_id);
        let target_lane = lane.and_then(|l| l.name.clone());
        if target_lane_id != current_lane && lane.is_some() {
            // The default lane is written as an empty value.
            let value = if target_lane_id == DEFAULT_LANE_ID {
                json!("")
            } else {
                json!(target_lane.clone().unwrap_or_default())
            };
            field_changes.push((board.fields.lane.clone(), value));
        }

        let new_state = (mapped_state != &card.state).then(|| mapped_state.clone());
        if let Some(state) = &new_state {
            field_changes.push((fields::STATE.to_owned(), json!(state)));
        }

        let reorder = (target.previous_id.is_some() || target.next_id.is_some())
            .then_some((target.previous_id, target.next_id));

        let mut warnings = Vec::new();
        if column_changes && column.item_limit > 0 {
            let count = board.placement.get(&column.id).map_or(0, Vec::len);
            if count + 1 > column.item_limit as usize {
                warnings.push(format!(
                    "{} is over its WIP limit ({}/{})",
                    column.name,
                    count + 1,
                    column.item_limit
                ));
            }
        }

        Ok(Self {
            card: card.id,
            expected_rev: card.rev,
            field_changes,
            reorder,
            warnings,
            target_column: column.name.clone(),
            target_done: done,
            target_lane,
            new_state,
        })
    }

    /// Nothing to write.
    pub fn is_noop(&self) -> bool {
        self.field_changes.is_empty() && self.reorder.is_none()
    }

    /// The JSON Patch for the field changes (`test /rev` first), or `None` if only reordering.
    pub fn to_patch(&self) -> Option<PatchDocument> {
        if self.field_changes.is_empty() {
            return None;
        }
        Some(
            self.field_changes
                .iter()
                .fold(PatchDocument::update(self.expected_rev), |doc, (f, v)| {
                    doc.field(f, v.clone())
                }),
        )
    }

    /// Applies the move to a local copy (optimistic update).
    pub fn apply(&self, card: &mut Card) {
        card.board_column = Some(self.target_column.clone());
        card.board_column_done = self.target_done;
        card.board_lane = self.target_lane.clone();
        if let Some(state) = &self.new_state {
            card.state = state.clone();
        }
    }
}
