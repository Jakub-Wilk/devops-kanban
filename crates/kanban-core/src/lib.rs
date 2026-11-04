//! Domain layer between the UI and the API clients (`ado-client`, `sevenpace-client`).
//!
//! Owns everything that is "kanban logic" rather than HTTP or rendering: building a [`Board`]
//! from ADO board config + work items, deciding which fields a move must patch, background
//! delta sync, time tracking, the on-disk cache and user settings. No Dioxus dependency.

pub mod board;
pub mod cache;
pub mod card;
pub mod config;
pub mod error;
pub mod move_plan;
pub mod states;
pub mod sync;
pub mod time;

pub use board::{Board, Column, ColumnKind, Lane};
pub use card::{Card, CardId, Person};
pub use config::Settings;
pub use error::CoreError;
pub use move_plan::{MoveError, MovePlan, MoveTarget};
pub use states::{StateCategory, StateMap};
pub use sync::SyncEngine;
