//! Domain layer between the UI and `ado-client`.
//!
//! Owns everything that is "kanban logic" rather than HTTP or rendering: building a [`Board`]
//! from ADO board config + work items, deciding which fields a move must patch, background
//! delta sync, the on-disk cache and user settings. No Dioxus dependency.

pub mod board;
pub mod cache;
pub mod card;
pub mod config;
pub mod move_plan;
pub mod sync;

pub use board::{Board, Column, ColumnKind, Lane};
pub use card::{Card, CardId};
pub use config::Settings;
pub use move_plan::{MovePlan, MoveTarget};
pub use sync::SyncEngine;
