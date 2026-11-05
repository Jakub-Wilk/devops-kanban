//! App-wide reactive state, provided via context at the root.
//!
//! Holds: settings, the ADO client, connection/load status, the loaded [`Workspace`] (board
//! configs + cards), ids with a write in flight (for optimistic updates and to keep sync off
//! them), the transient drag state, the open dialogs (item: draft, original, conflict; time) and
//! toasts, and 7pace time tracking (client,
//! the user's entries per card, the running timer). Every field is a `Signal`, so
//! `AppState` is `Copy` and can move into event handlers and tasks.

use std::collections::{HashMap, HashSet};

use ado_client::AdoClient;
use dioxus::core::Task;
use dioxus::prelude::*;
use kanban_core::sync::InFlight;
use kanban_core::time::{Activity, TimeEntry, TimerState};
use kanban_core::{CardId, ItemDraft, ItemField, Person, Settings, Workspace};
use sevenpace_client::SevenPaceClient;
use time::{Date, OffsetDateTime};

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
    /// A row of the List. It can only be dropped among rows with the same `parent` (`None` for
    /// epics), so drags reorder siblings and never reparent.
    Row { id: CardId, parent: Option<CardId> },
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
    /// Offered after a kanban move: put the card back where it was.
    pub move_back: Option<MoveBack>,
}

/// Where a moved card came from. Moving back is a second, ordinary move (a new revision in ADO,
/// and process rules may set fields such as Closed Date again), not an undo.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveBack {
    pub board: String,
    pub id: CardId,
    pub column_id: String,
    pub lane_id: Option<String>,
    pub index: usize,
}

/// The new/edit work item dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogState {
    /// `None` = create.
    pub editing: Option<CardId>,
    /// Creating/editing a root (epic) rather than a work item.
    pub is_root: bool,
    /// What ADO has (edit) or the starting values (new). `form` minus this is the unsaved
    /// changes, and only those are written.
    pub original: ItemDraft,
    pub form: ItemDraft,
    /// The revision `original` came from; the PATCH tests it.
    pub rev: u32,
    /// Edit: the full item (description, who changed it) has loaded.
    pub loaded: bool,
    /// Edit: the full item couldn't be loaded (the description stays read-only).
    pub load_error: Option<String>,
    pub meta: Option<ItemMeta>,
    /// HTML the description editor starts with. The editor owns its content after that; it is
    /// read back only when saving.
    pub desc_initial: String,
    /// Bumped to remount the editor with `desc_initial`.
    pub desc_key: u32,
    /// The user typed in the description editor.
    pub desc_dirty: bool,
    /// Set after a save hit a concurrent edit; the form was rebased onto the new revision.
    pub conflict: Option<Conflict>,
    /// New: keep the dialog open for the next item after creating one.
    pub create_another: bool,
    /// New, from a kanban column's "New" button: the column to create the item in.
    pub column: Option<NewInColumn>,
}

/// A kanban column a new item starts in. Its state is preset in the dialog; if the user keeps
/// it, the board's own column field is written too, so the card lands in this column even when
/// another column maps the same state.
#[derive(Debug, Clone, PartialEq)]
pub struct NewInColumn {
    /// The board's `WEF_…_Kanban.Column` field.
    pub field: String,
    pub name: String,
    /// The state the column maps for the item's type.
    pub state: String,
}

/// Who touched the item, for the edit dialog's header.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemMeta {
    pub changed_by: Option<Person>,
    pub changed: Option<OffsetDateTime>,
    pub created_by: Option<Person>,
    pub created: Option<OffsetDateTime>,
    /// The item in the ADO web UI.
    pub web_url: String,
}

/// A save that hit someone else's edit.
#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    pub by: Option<String>,
    /// What they changed.
    pub fields: Vec<ItemField>,
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

/// The add/edit time dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeDialogState {
    pub card: CardId,
    /// Worklog uuid when editing.
    pub editing: Option<String>,
    pub can_delete: bool,
    /// Opened from Stop: saving stops the timer (7pace logs it) and then applies the form to
    /// that entry.
    pub from_timer: bool,
    /// From Stop: the timer's minutes when the dialog opened (unchanged = keep 7pace's length).
    pub timer_minutes: i64,
    /// The day the work was done.
    pub date: Date,
    pub minutes: i64,
    /// The duration field as typed (`H:MM`); follows `minutes` when a step button is used.
    pub duration_text: String,
    /// The entry ends now; otherwise it starts at `start`.
    pub ends_now: bool,
    /// `HH:MM`.
    pub start: String,
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
    /// Kanban cards whose tasks and time log are open. Only these (and the tracked card) load
    /// 7pace entries.
    pub card_open: Signal<HashSet<CardId>>,
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
            card_open: Signal::new(HashSet::new()),
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
