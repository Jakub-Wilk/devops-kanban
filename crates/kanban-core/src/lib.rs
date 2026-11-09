//! Domain layer between the UI and the API clients (`ado-client`, `sevenpace-client`).
//!
//! Owns everything that is "kanban logic" rather than HTTP or rendering: building a [`Board`]
//! from ADO board config + work items, deciding which fields a move must patch, the epic
//! hierarchy, the item dialog's draft (diff, conflict rebase), view filters, timeline layout, the timetable week (overlaps, drag rules), WIQL scoping, background delta sync, time tracking, the on-disk
//! caches (workspace, avatars) and user settings. No Dioxus dependency.

pub mod avatars;
pub mod board;
pub mod cache;
pub mod card;
pub mod config;
pub mod description;
pub mod error;
pub mod filters;
pub mod hierarchy;
pub mod item;
pub mod move_plan;
pub mod queries;
pub mod states;
pub mod sync;
pub mod time;
pub mod timeline;
pub mod timetable;
pub mod workspace;

pub use board::{Board, Column, ColumnKind, Lane};
pub use card::{Card, CardId, Person};
pub use config::{BoardPrefs, Settings, TabFilters, ViewPrefs};
pub use error::CoreError;
pub use hierarchy::Hierarchy;
pub use item::{ItemDraft, ItemField};
pub use move_plan::{MoveError, MovePlan, MoveTarget};
pub use queries::Scope;
pub use states::{StateCategory, StateMap};
pub use sync::{InFlight, SyncEngine, SyncEvent};
pub use timeline::{TimelineItem, TimelineLayout};
pub use workspace::{BacklogLevel, Iteration, Workspace};
