//! Board model built from `work/boards/{board}` + fetched cards.
//!
//! Column membership is read from `System.BoardColumn` (read-only, but always populated);
//! the writable per-board `WEF_<guid>_Kanban.*` field names are kept in [`BoardFields`] for
//! [`crate::MovePlan`].

use std::collections::HashMap;

use ado_client::BoardDto;
use serde::{Deserialize, Serialize};

use crate::card::{Card, CardId};

/// The all-zero id ADO gives the default swimlane.
pub const DEFAULT_LANE_ID: &str = "00000000-0000-0000-0000-000000000000";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnKind {
    Incoming,
    InProgress,
    Outgoing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lane {
    pub id: String,
    pub name: Option<String>,
    pub color: Option<String>,
}

impl Lane {
    pub fn is_default(&self) -> bool {
        self.id == DEFAULT_LANE_ID || self.name.as_deref().is_none_or(str::is_empty)
    }
}

/// Writable extension fields for this specific board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardFields {
    /// `WEF_<guid>_Kanban.Column`
    pub column: String,
    /// `WEF_<guid>_Kanban.Column.Done`
    pub done: String,
    /// `WEF_<guid>_Kanban.Lane`
    pub lane: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub project: String,
    pub team: String,
    pub name: String,
    pub columns: Vec<Column>,
    pub lanes: Vec<Lane>,
    pub fields: BoardFields,
    /// Work item types shown on this board.
    pub work_item_types: Vec<String>,
    /// Card ids per column id, ordered by the backlog order field (StackRank).
    pub placement: HashMap<String, Vec<CardId>>,
}

impl Board {
    pub fn from_dto(project: &str, team: &str, dto: &BoardDto) -> Self {
        let columns: Vec<Column> = dto
            .columns
            .iter()
            .map(|c| Column {
                id: c.id.clone(),
                name: c.name.clone(),
                kind: match c.column_type.as_str() {
                    "incoming" => ColumnKind::Incoming,
                    "outgoing" => ColumnKind::Outgoing,
                    _ => ColumnKind::InProgress,
                },
                item_limit: c.item_limit,
                is_split: c.is_split.unwrap_or(false),
                state_mappings: c.state_mappings.clone(),
            })
            .collect();
        // The types on a board are exactly those its columns map; this already reflects
        // `bugsBehavior` for the team.
        let mut work_item_types: Vec<String> = columns
            .iter()
            .flat_map(|c| c.state_mappings.keys().cloned())
            .collect();
        work_item_types.sort();
        work_item_types.dedup();
        Self {
            project: project.to_owned(),
            team: team.to_owned(),
            name: dto.name.clone(),
            placement: columns.iter().map(|c| (c.id.clone(), Vec::new())).collect(),
            columns,
            lanes: dto
                .rows
                .iter()
                .map(|r| Lane {
                    id: r.id.clone(),
                    name: r.name.clone().filter(|n| !n.is_empty()),
                    color: r.color.clone(),
                })
                .collect(),
            fields: BoardFields {
                column: dto.fields.column_field.reference_name.clone(),
                done: dto.fields.done_field.reference_name.clone(),
                lane: dto.fields.row_field.reference_name.clone(),
            },
            work_item_types,
        }
    }

    pub fn column(&self, id: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.id == id)
    }

    /// The column a card is in: `System.BoardColumn`, else the first column mapping its state.
    pub fn column_of(&self, card: &Card) -> Option<&Column> {
        card.board_column
            .as_deref()
            .and_then(|name| self.columns.iter().find(|c| c.name == name))
            .or_else(|| {
                self.columns
                    .iter()
                    .find(|c| c.state_mappings.get(&card.work_item_type) == Some(&card.state))
            })
    }

    /// The lane id a card is in (default lane when unset or unknown).
    pub fn lane_of(&self, card: &Card) -> String {
        card.board_lane
            .as_deref()
            .and_then(|name| self.lanes.iter().find(|l| l.name.as_deref() == Some(name)))
            .or_else(|| self.lanes.iter().find(|l| l.is_default()))
            .map(|l| l.id.clone())
            .unwrap_or_else(|| DEFAULT_LANE_ID.to_owned())
    }

    /// Whether lanes other than the default exist.
    pub fn has_lanes(&self) -> bool {
        self.lanes.iter().any(|l| !l.is_default())
    }

    pub fn shows(&self, card: &Card) -> bool {
        self.work_item_types.contains(&card.work_item_type)
    }

    /// Rebuilds `placement` from `cards`, ordered by order field (unranked last), then id.
    pub fn place<'a>(&mut self, cards: impl IntoIterator<Item = &'a Card>) {
        let mut per_column: HashMap<String, Vec<&Card>> = self
            .columns
            .iter()
            .map(|c| (c.id.clone(), Vec::new()))
            .collect();
        for card in cards.into_iter().filter(|c| self.shows(c)) {
            if let Some(col) = self.column_of(card) {
                per_column.entry(col.id.clone()).or_default().push(card);
            }
        }
        self.placement = per_column
            .into_iter()
            .map(|(col, mut cards)| {
                cards.sort_by(|a, b| {
                    match (a.order, b.order) {
                        (Some(x), Some(y)) => x.total_cmp(&y),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                    .then(a.id.cmp(&b.id))
                });
                (col, cards.into_iter().map(|c| c.id).collect())
            })
            .collect();
    }

    /// Neighbours for dropping `card` at `index` of `column_id` (index counted without `card`).
    pub fn neighbours(
        &self,
        column_id: &str,
        card: CardId,
        index: usize,
    ) -> (Option<CardId>, Option<CardId>) {
        let ids: Vec<CardId> = self
            .placement
            .get(column_id)
            .map(|v| v.iter().copied().filter(|&id| id != card).collect())
            .unwrap_or_default();
        let index = index.min(ids.len());
        let previous = index.checked_sub(1).and_then(|i| ids.get(i).copied());
        (previous, ids.get(index).copied())
    }

    /// Moves `card` to `index` of `column_id` in `placement` (optimistic reorder).
    pub fn reposition(&mut self, card: CardId, column_id: &str, index: usize) {
        for ids in self.placement.values_mut() {
            ids.retain(|&id| id != card);
        }
        let ids = self.placement.entry(column_id.to_owned()).or_default();
        ids.insert(index.min(ids.len()), card);
    }

    /// Columns not in `hidden` (matched by name, which is stable across board revisions for users).
    pub fn visible_columns<'a>(
        &'a self,
        hidden: &'a [String],
    ) -> impl Iterator<Item = &'a Column> + 'a {
        self.columns
            .iter()
            .filter(move |c| !hidden.contains(&c.name))
    }
}
