//! Card = the subset of a work item a board needs, plus its revision for concurrency.
//!
//! The field list to fetch comes from the board's `cardsettings` plus the fixed set the domain
//! needs (id, rev, type, state, board column/lane, order, parent).

use std::collections::BTreeMap;

pub type CardId = u32;

#[derive(Debug, Clone)]
pub struct Card {
    pub id: CardId,
    pub rev: u32,
    pub work_item_type: String,
    pub title: String,
    pub state: String,
    pub board_column: Option<String>,
    pub board_column_done: bool,
    pub board_lane: Option<String>,
    pub assigned_to: Option<String>,
    pub tags: Vec<String>,
    pub order: Option<f64>,
    pub parent: Option<CardId>,
    /// Remaining card-setting fields, raw.
    pub extra: BTreeMap<String, serde_json::Value>,
}
