//! App-wide reactive state, provided via context at the root.
//!
//! Holds: settings, the ADO client, connection/load status, the loaded [`Workspace`] (board
//! configs + cards), ids with a write in flight (for optimistic updates and to keep sync off
//! them), the transient drag state, the open dialogs and toasts, and 7pace time tracking (client,
//! the user's entries per card, the running timer). Every field is a `Signal`, so
//! `AppState` is `Copy` and can move into event handlers and tasks.

use std::collections::{HashMap, HashSet};

use ado_client::AdoClient;
use dioxus::core::Task;
use dioxus::prelude::*;
use kanban_core::sync::InFlight;
use kanban_core::time::{Activity, TimeEntry, TimerState};
use kanban_core::{CardId, Settings, Workspace};
use sevenpace_client::SevenPaceClient;
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Idle,
    /// Nothing to show yet.
    Loading,
    /// Showing the cached snapshot while the live load runs.
    Refreshing,
    Ready,
    Error(String),
}

/// What is being dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Drag {
    /// A kanban card.
    Card(CardId),
    /// A row of the epic list (an epic, or a child within its epic).
    Row { id: CardId, root: Option<CardId> },
}

/// Where a dragged kanban card would land.
#[derive(Debug, Clone, PartialEq)]
pub struct DropSlot {
    pub column_id: String,
    pub lane_id: Option<String>,
    /// Insert before this card; `None` = end of the column. Card-relative (not an index) so
    /// filtered views still land in the right place in the full column.
    pub before: Option<CardId>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToastKind {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub text: String,
}

/// The new/edit dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogState {
    /// `None` = create.
    pub editing: Option<CardId>,
    /// Creating/editing a root (epic) rather than a work item.
    pub is_root: bool,
    pub form: ItemForm,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemForm {
    pub title: String,
    pub work_item_type: String,
    pub parent: Option<CardId>,
    pub area: String,
    pub priority: u8,
    /// `unique_name`, empty = unassigned.
    pub assignee: String,
    /// `YYYY-MM-DD`, empty = none.
    pub target_date: String,
    pub state: String,
}

/// 7pace time tracking (see `time_tracking.rs`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimeTracking {
    /// The 7pace user's name, once the token is validated: "connected".
    pub me: Option<String>,
    /// Why time tracking isn't available (bad token, API disabled), for Settings.
    pub error: Option<String>,
    pub activities_enabled: bool,
    pub activities: Vec<Activity>,
    pub default_activity: Option<String>,
    /// The user's own entries per card, newest first.
    pub entries: HashMap<CardId, Vec<TimeEntry>>,
    /// Cards whose entries were fetched (so empty lists aren't fetched again).
    pub loaded: HashSet<CardId>,
    /// Bumped by Reload so the kanban fetches entries again.
    pub generation: u64,
    pub timer: Option<TimerState>,
    /// A start/stop is in flight.
    pub busy: bool,
}

/// The add/edit time dialog. Form fields are the raw input values.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeDialogState {
    pub card: CardId,
    pub card_title: String,
    /// Worklog uuid when editing.
    pub editing: Option<String>,
    pub can_delete: bool,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// `HH:MM`; empty = ends now.
    pub start: String,
    /// Free text, see `kanban_core::time::parse_duration`.
    pub duration: String,
    /// Activity type id; empty = the user's default.
    pub activity: String,
    pub comment: String,
}

#[derive(Clone, Copy)]
pub struct AppState {
    pub settings: Signal<Settings>,
    /// PAT for this session (from the keyring or typed in when no keyring is available).
    pub pat: Signal<Option<String>>,
    pub client: Signal<Option<AdoClient>>,
    pub status: Signal<Status>,
    pub workspace: Signal<Option<Workspace>>,
    /// Ids with a write in flight; shared with the sync engine.
    pub in_flight: Signal<InFlight>,
    /// Same ids, reactive, for the "saving" shimmer on cards.
    pub pending: Signal<HashSet<CardId>>,
    pub drag: Signal<Option<Drag>>,
    pub drop_slot: Signal<Option<DropSlot>>,
    pub dialog: Signal<Option<DialogState>>,
    pub toasts: Signal<Vec<Toast>>,
    pub last_sync: Signal<Option<OffsetDateTime>>,
    pub sync_task: Signal<Option<Task>>,
    /// Shown on the setup screen after the PAT was rejected.
    pub auth_error: Signal<Option<String>>,
    pub sevenpace: Signal<Option<SevenPaceClient>>,
    pub time: Signal<TimeTracking>,
    pub time_dialog: Signal<Option<TimeDialogState>>,
    /// Wall clock for running timers, ticked every second while one runs.
    pub clock: Signal<OffsetDateTime>,
}

impl AppState {
    pub fn new(settings: Settings, pat: Option<String>, client: Option<AdoClient>) -> Self {
        Self {
            settings: Signal::new(settings),
            pat: Signal::new(pat),
            client: Signal::new(client),
            status: Signal::new(Status::Idle),
            workspace: Signal::new(None),
            in_flight: Signal::new(InFlight::default()),
            pending: Signal::new(HashSet::new()),
            drag: Signal::new(None),
            drop_slot: Signal::new(None),
            dialog: Signal::new(None),
            toasts: Signal::new(Vec::new()),
            last_sync: Signal::new(None),
            sync_task: Signal::new(None),
            auth_error: Signal::new(None),
            sevenpace: Signal::new(None),
            time: Signal::new(TimeTracking::default()),
            time_dialog: Signal::new(None),
            clock: Signal::new(OffsetDateTime::now_utc()),
        }
    }
}

pub fn use_app() -> AppState {
    use_context::<AppState>()
}
