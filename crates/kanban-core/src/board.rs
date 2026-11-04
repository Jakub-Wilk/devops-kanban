//! Board model built from `work/boards/{board}` + `backlogconfiguration` + fetched cards.
//!
//! Column membership is read from `System.BoardColumn` (read-only, but always populated);
//! the writable per-board `WEF_<guid>_Kanban.*` field names are kept in [`BoardFields`] for
//! [`crate::MovePlan`].

use std::collections::HashMap;

use crate::card::CardId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Incoming,
    InProgress,
    Outgoing,
}

#[derive(Debug, Clone)]
pub struct Column {
    pub id: String,
    pub name: String,
    pub kind: ColumnKind,
    /// WIP limit; 0 = none.
    pub item_limit: u32,
    /// Split into Doing / Done sub-columns.
    pub is_split: bool,
    /// Work item type → state this column maps to.
    pub state_mappings: HashMap<String, String>,
}

/// Swimlane. The default lane has an all-zero id and no name.
#[derive(Debug, Clone)]
pub struct Lane {
    pub id: String,
    pub name: Option<String>,
}

/// Writable extension fields for this specific board.
#[derive(Debug, Clone)]
pub struct BoardFields {
    /// `WEF_<guid>_Kanban.Column`
    pub column: String,
    /// `WEF_<guid>_Kanban.Column.Done`
    pub done: String,
    /// `WEF_<guid>_Kanban.Lane`
    pub lane: String,
}

#[derive(Debug, Clone)]
pub struct Board {
    pub project: String,
    pub team: String,
    pub name: String,
    pub columns: Vec<Column>,
    pub lanes: Vec<Lane>,
    pub fields: BoardFields,
    /// Work item types shown on this board (from the backlog level).
    pub work_item_types: Vec<String>,
    /// Card ids per column id, ordered by the backlog order field (StackRank).
    pub placement: HashMap<String, Vec<CardId>>,
}
