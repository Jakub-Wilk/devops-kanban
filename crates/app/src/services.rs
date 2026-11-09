//! Bridge between UI and `kanban-core`: builds the `AdoClient` from settings + keyring, runs
//! loads/moves/edits as async tasks, applies optimistic updates to the workspace and rolls back
//! on failures (refetching on `AdoError::Conflict`), and owns the `SyncEngine` background task.
//!
//! Tasks are started with `spawn_forever` (root scope), not `spawn`: `spawn` ties a task to the
//! calling component, and callers are often dialogs that close as soon as they hand off, which
//! would cancel the work mid-flight.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ado_client::{AdoClient, AdoError, ClientConfig, PatAuth, PatchDocument, WorkItemDto};
use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use kanban_core::card::{Card, CardId, Person, fields};
use kanban_core::{
    CoreError, ItemDraft, MovePlan, MoveTarget, Settings, StateCategory, SyncEngine, SyncEvent,
    Workspace, cache,
};
use serde_json::Value;
use time::OffsetDateTime;

use crate::state::{AppState, ItemMeta, MoveBack, NewInColumn, Status, Toast, ToastKind};

const TOAST_SECS: u64 = 6;

pub fn build_client(settings: &Settings, pat: &str) -> Result<AdoClient, String> {
    let base_url =
        url::Url::parse(&settings.api.base_url).map_err(|e| format!("api.base_url: {e}"))?;
    Ok(AdoClient::new(
        ClientConfig {
            base_url,
            organization: settings.organization.trim().to_owned(),
            api_version: settings.api.api_version.clone(),
            max_batch: settings.api.max_batch,
        },
        Arc::new(PatAuth::new(pat)),
    ))
}

pub fn toast(st: AppState, kind: ToastKind, text: impl Into<String>) {
    push_toast(st, kind, text.into(), None);
}

fn push_toast(mut st: AppState, kind: ToastKind, text: String, move_back: Option<MoveBack>) {
    let id = st.toasts.peek().iter().map(|t| t.id).max().unwrap_or(0) + 1;
    st.toasts.write().push(Toast {
        id,
        kind,
        text,
        move_back,
    });
    spawn_forever(async move {
        tokio::time::sleep(Duration::from_secs(TOAST_SECS)).await;
        st.toasts.write().retain(|t| t.id != id);
    });
}

pub fn save_settings(st: AppState) {
    if let Err(e) = st.settings.peek().save() {
        toast(st, ToastKind::Warn, format!("Couldn't save settings: {e}"));
    }
}

/// Validates the PAT against `connectionData`, stores it, and installs the client.
pub async fn connect(mut st: AppState, organization: String, pat: String) -> Result<(), String> {
    let mut settings = st.settings.peek().clone();
    settings.organization = organization.trim().to_owned();
    let client = build_client(&settings, pat.trim())?;
    client.connection_data().await.map_err(|e| match e {
        AdoError::Unauthorized => "Azure DevOps rejected the token. Check that it belongs to this organization and has Work Items (Read & write).".to_owned(),
        other => other.to_string(),
    })?;
    if let Err(e) = kanban_core::config::store_pat(&settings.organization, pat.trim()) {
        tracing::warn!(error = %e, "no keyring; keeping the PAT for this session only");
        toast(
            st,
            ToastKind::Warn,
            "No OS keyring available: the token is kept for this session only.",
        );
    }
    st.settings.set(settings);
    save_settings(st);
    st.pat.set(Some(pat.trim().to_owned()));
    st.client.set(Some(client));
    st.auth_error.set(None);
    Ok(())
}

/// Forgets the client after a 401/203 and sends the user back to setup.
pub fn unauthorized(mut st: AppState) {
    stop_sync(st);
    st.client.set(None);
    st.pat.set(None);
    st.auth_error.set(Some(
        "Your token expired or lacks the Work Items scope. Enter a new one.".into(),
    ));
    st.status.set(Status::Idle);
}

fn handle_error(st: AppState, e: &AdoError, what: &str) {
    if matches!(e, AdoError::Unauthorized) {
        unauthorized(st);
    } else {
        toast(st, ToastKind::Error, format!("{what}: {e}"));
    }
}

/// Paints the cached snapshot (if any) and loads the live workspace for `project`/`team`.
pub fn open_workspace(mut st: AppState, project: String, team: String, force: bool) {
    let same = st
        .workspace
        .peek()
        .as_ref()
        .is_some_and(|w| w.project == project && w.team == team);
    if same && !force && *st.status.peek() != Status::Idle {
        return;
    }
    let Some(client) = st.client.peek().clone() else {
        return;
    };
    stop_sync(st);
    let settings = st.settings.peek().clone();
    if !same {
        let cached = settings
            .cache_enabled
            .then(|| cache::load(&settings.organization, &project, &team))
            .flatten();
        st.status.set(if cached.is_some() {
            Status::Refreshing
        } else {
            Status::Loading
        });
        st.workspace.set(cached);
    } else {
        st.status.set(Status::Refreshing);
    }
    spawn_forever(async move {
        match Workspace::load(&client, &project, &team).await {
            Ok(ws) => {
                if settings.cache_enabled
                    && let Err(e) = cache::save(&settings.organization, &ws)
                {
                    tracing::warn!(error = %e, "couldn't write cache");
                }
                st.workspace.set(Some(ws));
                st.status.set(Status::Ready);
                st.last_sync.set(Some(OffsetDateTime::now_utc()));
                start_sync(st);
            }
            Err(CoreError::Ado(AdoError::Unauthorized)) => unauthorized(st),
            Err(e) => st.status.set(Status::Error(e.to_string())),
        }
    });
}

fn start_sync(mut st: AppState) {
    let Some(client) = st.client.peek().clone() else {
        return;
    };
    let Some((scope, fields, since)) = st
        .workspace
        .peek()
        .as_ref()
        .map(|w| (w.scope.clone(), w.fields.clone(), w.synced_to))
    else {
        return;
    };
    let engine = SyncEngine {
        client,
        interval: Duration::from_secs(st.settings.peek().poll_interval_secs),
        scope,
        fields,
        in_flight: st.in_flight.peek().clone(),
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(engine.run(since.unwrap_or_else(OffsetDateTime::now_utc), tx));
    let task = spawn_forever(async move {
        while let Some(event) = rx.recv().await {
            match event {
                SyncEvent::Changed(items) => {
                    if let Some(ws) = st.workspace.write().as_mut() {
                        let changed = ws.upsert(&items);
                        tracing::debug!(count = changed.len(), "sync applied");
                    }
                    st.last_sync.set(Some(OffsetDateTime::now_utc()));
                }
                SyncEvent::Failed(msg) => tracing::warn!(%msg, "sync round failed"),
                SyncEvent::Unauthorized => {
                    unauthorized(st);
                    break;
                }
            }
        }
    });
    st.sync_task.set(Some(task));
}

/// Cancels the sync task; dropping its receiver stops the engine.
pub fn stop_sync(mut st: AppState) {
    if let Some(task) = st.sync_task.take() {
        task.cancel();
    }
}

fn begin(mut st: AppState, id: CardId) {
    st.in_flight
        .peek()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id);
    st.pending.write().insert(id);
}

fn end(mut st: AppState, id: CardId) {
    st.in_flight
        .peek()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id);
    st.pending.write().remove(&id);
}

/// Replaces a card with the server copy (keeps placement unless `replace` is set).
fn store_server_copy(mut st: AppState, dto: &WorkItemDto, replace: bool) {
    if let Some(ws) = st.workspace.write().as_mut() {
        if replace {
            ws.upsert(std::slice::from_ref(dto));
        } else {
            let card = Card::from_work_item(dto, &ws.scope.order_field);
            ws.cards.insert(card.id, card);
        }
    }
}

/// After a conflict: refetch the item and show the server state.
async fn refetch(st: AppState, client: &AdoClient, project: &str, id: CardId) {
    match client.work_item(project, id).await {
        Ok(dto) => store_server_copy(st, &dto, true),
        Err(e) => handle_error(st, &e, "Reload failed"),
    }
}

/// Refetches one card from ADO after `delay` (e.g. for 7pace's `CompletedWork` sync).
pub fn reload_card_later(st: AppState, id: CardId, delay: Duration) {
    spawn_forever(async move {
        tokio::time::sleep(delay).await;
        let Some(client) = st.client.peek().clone() else {
            return;
        };
        let Some(project) = st.workspace.peek().as_ref().map(|w| w.project.clone()) else {
            return;
        };
        if !st.pending.peek().contains(&id) {
            refetch(st, &client, &project, id).await;
        }
    });
}

/// Drops `id` on `column_id` (and `lane_id`) at `index` of the `board` kanban.
pub fn move_card(
    st: AppState,
    board: &str,
    id: CardId,
    column_id: String,
    lane_id: Option<String>,
    index: usize,
) {
    move_card_inner(st, board, id, column_id, lane_id, index, true);
}

/// Puts a card back where a toast's "Move back" says it came from. Runs as a normal move with
/// the card's current revision, so a concurrent edit still surfaces as a conflict.
pub fn move_back(st: AppState, m: MoveBack) {
    move_card_inner(st, &m.board, m.id, m.column_id, m.lane_id, m.index, false);
}

fn move_card_inner(
    mut st: AppState,
    board: &str,
    id: CardId,
    column_id: String,
    lane_id: Option<String>,
    index: usize,
    offer_move_back: bool,
) {
    let Some(client) = st.client.peek().clone() else {
        return;
    };
    let prepared = {
        let ws_ref = st.workspace.peek();
        let Some(ws) = ws_ref.as_ref() else { return };
        let (Some(b), Some(card)) = (ws.boards.get(board), ws.cards.get(&id)) else {
            return;
        };
        let current_col = b.column_of(card).map(|c| c.id.clone());
        let current_idx = current_col
            .as_ref()
            .and_then(|c| b.placement.get(c))
            .and_then(|ids| ids.iter().position(|&x| x == id));
        let lane_same = lane_id.as_ref().is_none_or(|l| *l == b.lane_of(card));
        if current_col.as_deref() == Some(column_id.as_str())
            && current_idx == Some(index)
            && lane_same
        {
            return;
        }
        // Where it is now, for "Move back" and the toast text.
        let origin = current_col.clone().map(|col| MoveBack {
            board: board.to_owned(),
            id,
            column_id: col,
            lane_id: b.has_lanes().then(|| b.lane_of(card)),
            index: current_idx.unwrap_or(0),
        });
        let target_name = b
            .columns
            .iter()
            .find(|c| c.id == column_id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let column_changed = current_col.as_deref() != Some(column_id.as_str());
        let (previous_id, next_id) = b.neighbours(&column_id, id, index);
        let target = MoveTarget {
            column_id: column_id.clone(),
            done: false,
            lane_id,
            previous_id,
            next_id,
        };
        match MovePlan::new(b, card, &target) {
            Ok(plan) => Some((
                plan,
                card.clone(),
                b.placement.clone(),
                ws.project.clone(),
                ws.team.clone(),
                (origin, target_name, column_changed),
            )),
            Err(e) => {
                drop(ws_ref);
                toast(st, ToastKind::Warn, e.to_string());
                return;
            }
        }
    };
    let Some((
        plan,
        before,
        placement_before,
        project,
        team,
        (origin, target_name, column_changed),
    )) = prepared
    else {
        return;
    };
    if plan.is_noop() {
        return;
    }
    if let Some(ws) = st.workspace.write().as_mut() {
        if let Some(card) = ws.cards.get_mut(&id) {
            plan.apply(card);
        }
        if let Some(b) = ws.boards.get_mut(board) {
            b.reposition(id, &column_id, index);
        }
    }
    for w in &plan.warnings {
        toast(st, ToastKind::Warn, w.clone());
    }
    begin(st, id);
    let board = board.to_owned();
    spawn_forever(async move {
        let mut result = Ok(());
        if let Some(patch) = plan.to_patch() {
            match client.update_work_item(&project, id, &patch).await {
                Ok(dto) => store_server_copy(st, &dto, false),
                Err(e) => result = Err(e),
            }
        }
        if result.is_ok()
            && let Some((prev, next)) = plan.reorder
        {
            match client.reorder(&project, &team, id, prev, next).await {
                Ok(orders) => {
                    if let Some(ws) = st.workspace.write().as_mut() {
                        for o in orders {
                            if let Some(c) = ws.cards.get_mut(&o.id) {
                                c.order = Some(o.order);
                            }
                        }
                    }
                }
                Err(e) => result = Err(e),
            }
        }
        end(st, id);
        match result {
            Ok(()) => {
                if offer_move_back && column_changed {
                    push_toast(
                        st,
                        ToastKind::Info,
                        format!("Moved #{id} to {target_name}."),
                        origin,
                    );
                }
            }
            Err(AdoError::Conflict { .. }) => {
                toast(
                    st,
                    ToastKind::Warn,
                    format!("#{id} was changed by someone else. Showing the latest version."),
                );
                refetch(st, &client, &project, id).await;
            }
            Err(e) => {
                if let Some(ws) = st.workspace.write().as_mut() {
                    ws.cards.insert(id, before);
                    if let Some(b) = ws.boards.get_mut(&board) {
                        b.placement = placement_before;
                    }
                }
                handle_error(st, &e, &format!("Couldn't move #{id}"));
            }
        }
    });
}

/// Applies `changes` locally, PATCHes them, and reconciles with the server copy.
pub fn patch_item(
    mut st: AppState,
    id: CardId,
    changes: Vec<(String, Value)>,
    local: impl FnOnce(&mut Card),
) {
    let Some(client) = st.client.peek().clone() else {
        return;
    };
    let Some((before, project)) = st
        .workspace
        .peek()
        .as_ref()
        .and_then(|w| Some((w.cards.get(&id)?.clone(), w.project.clone())))
    else {
        return;
    };
    let patch = changes
        .iter()
        .fold(PatchDocument::update(before.rev), |d, (f, v)| {
            d.field(f, v.clone())
        });
    if patch.is_empty_update() {
        return;
    }
    if let Some(ws) = st.workspace.write().as_mut()
        && let Some(card) = ws.cards.get_mut(&id)
    {
        local(card);
    }
    begin(st, id);
    spawn_forever(async move {
        let result = client.update_work_item(&project, id, &patch).await;
        end(st, id);
        match result {
            Ok(dto) => store_server_copy(st, &dto, true),
            Err(AdoError::Conflict { .. }) => {
                toast(
                    st,
                    ToastKind::Warn,
                    format!("#{id} was changed by someone else. Showing the latest version."),
                );
                refetch(st, &client, &project, id).await;
            }
            Err(e) => {
                if let Some(ws) = st.workspace.write().as_mut() {
                    ws.cards.insert(id, before);
                    ws.place();
                }
                handle_error(st, &e, &format!("Couldn't update #{id}"));
            }
        }
    });
}

pub fn assign_to_me(st: AppState, id: CardId) {
    let Some(me) = st.workspace.peek().as_ref().and_then(|w| w.me.clone()) else {
        toast(
            st,
            ToastKind::Warn,
            "Couldn't tell who you are from the token.",
        );
        return;
    };
    let value = Value::String(me.unique_name.clone());
    patch_item(
        st,
        id,
        vec![(fields::ASSIGNED_TO.into(), value)],
        move |c| {
            c.assigned_to = Some(me);
        },
    );
}

pub fn set_target_date(st: AppState, id: CardId, date: Option<time::Date>) {
    let value = date.map_or(Value::Null, |d| {
        Value::String(kanban_core::card::format_date_field(d))
    });
    patch_item(
        st,
        id,
        vec![(fields::TARGET_DATE.into(), value)],
        move |c| {
            c.target_date = date;
        },
    );
}

pub fn set_state(st: AppState, id: CardId, state: String) {
    let value = Value::String(state.clone());
    patch_item(st, id, vec![(fields::STATE.into(), value)], move |c| {
        c.state = state;
    });
}

/// Ticks (first Completed state, e.g. Closed) or unticks (first Proposed state, e.g. New) a task.
pub fn set_task_done(st: AppState, id: CardId, done: bool) {
    let state = st.workspace.peek().as_ref().and_then(|w| {
        let t = w.cards.get(&id)?.work_item_type.clone();
        let category = if done {
            StateCategory::Completed
        } else {
            StateCategory::Proposed
        };
        w.states.states_in(&[t], category).into_iter().next()
    });
    match state {
        Some(s) => set_state(st, id, s),
        None => toast(
            st,
            ToastKind::Warn,
            "This work item type has no status for that.",
        ),
    }
}

/// Moves a list row between `prev` and `next` in backlog order.
pub fn reorder_item(mut st: AppState, id: CardId, prev: Option<CardId>, next: Option<CardId>) {
    let Some(client) = st.client.peek().clone() else {
        return;
    };
    let Some((project, team, before)) = st
        .workspace
        .peek()
        .as_ref()
        .and_then(|w| Some((w.project.clone(), w.team.clone(), w.cards.get(&id)?.order)))
    else {
        return;
    };
    // Optimistic order between the neighbours so the list re-sorts immediately.
    if let Some(ws) = st.workspace.write().as_mut() {
        let order_of = |x: Option<CardId>| x.and_then(|x| ws.cards.get(&x)?.order);
        let new = match (order_of(prev), order_of(next)) {
            (Some(a), Some(b)) => (a + b) / 2.0,
            (Some(a), None) => a + 1.0,
            (None, Some(b)) => b - 1.0,
            (None, None) => 0.0,
        };
        if let Some(c) = ws.cards.get_mut(&id) {
            c.order = Some(new);
        }
    }
    begin(st, id);
    spawn_forever(async move {
        let result = client.reorder(&project, &team, id, prev, next).await;
        end(st, id);
        match result {
            Ok(orders) => {
                if let Some(ws) = st.workspace.write().as_mut() {
                    for o in orders {
                        if let Some(c) = ws.cards.get_mut(&o.id) {
                            c.order = Some(o.order);
                        }
                    }
                    ws.place();
                }
            }
            Err(e) => {
                if let Some(ws) = st.workspace.write().as_mut()
                    && let Some(c) = ws.cards.get_mut(&id)
                {
                    c.order = before;
                }
                handle_error(st, &e, &format!("Couldn't reorder #{id}"));
            }
        }
    });
}

/// The full item for the edit dialog: the current card (stored, so the board catches up too),
/// its sanitised description and who changed/created it.
pub struct ItemDetails {
    pub card: Card,
    pub description: String,
    pub meta: ItemMeta,
}

pub async fn load_item(st: AppState, id: CardId) -> Result<ItemDetails, String> {
    let client = st.client.peek().clone().ok_or("Not connected.")?;
    let project = st
        .workspace
        .peek()
        .as_ref()
        .map(|w| w.project.clone())
        .ok_or("No board loaded.")?;
    let dto = client.work_item(&project, id).await.map_err(|e| {
        if matches!(e, AdoError::Unauthorized) {
            unauthorized(st);
        }
        format!("Couldn't load #{id}: {e}")
    })?;
    if !st.pending.peek().contains(&id) {
        store_server_copy(st, &dto, true);
    }
    let order_field = st
        .workspace
        .peek()
        .as_ref()
        .map(|w| w.scope.order_field.clone())
        .unwrap_or_default();
    let f = &dto.fields;
    let person = |k: &str| {
        f.get(k)
            .and_then(|v| serde_json::from_value::<ado_client::IdentityRef>(v.clone()).ok())
            .map(|i| Person::from(&i))
    };
    let when = |k: &str| {
        f.get(k).and_then(Value::as_str).and_then(|s| {
            OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
        })
    };
    Ok(ItemDetails {
        card: Card::from_work_item(&dto, &order_field),
        description: f
            .get(fields::DESCRIPTION)
            .and_then(Value::as_str)
            .map(kanban_core::description::sanitize)
            .unwrap_or_default(),
        meta: ItemMeta {
            changed_by: person(fields::CHANGED_BY),
            changed: when(fields::CHANGED_DATE),
            created_by: person(fields::CREATED_BY),
            created: when(fields::CREATED_DATE),
            web_url: client.work_item_web_url(&project, id),
        },
    })
}

/// How a dialog save went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved,
    /// Someone else changed the item first; nothing was written and the card shows their version.
    Conflict,
    /// Already reported in a toast.
    Failed,
}

/// Creates the dialog's item with its parent link, in `column` when given and its state was
/// kept. Returns the new id.
pub async fn create_item(
    mut st: AppState,
    draft: &ItemDraft,
    column: Option<&NewInColumn>,
) -> Option<CardId> {
    let client = st.client.peek().clone()?;
    let project = st.workspace.peek().as_ref().map(|w| w.project.clone())?;
    let mut doc = draft
        .patch_fields(None)
        .into_iter()
        .fold(PatchDocument::create(), |d, (f, v)| d.field(&f, v));
    // The board's own column field (never System.BoardColumn), only while it agrees with the
    // state; otherwise ADO places the card by its state.
    if let Some(c) = column.filter(|c| c.state == draft.state && !c.field.is_empty()) {
        doc = doc.field(&c.field, Value::String(c.name.clone()));
    }
    if let Some(p) = draft.parent {
        doc = doc.parent(&client.work_item_url(p));
        // Tasks land in their parent's sprint, as on ADO's own board.
        let iteration = st.workspace.peek().as_ref().and_then(|w| {
            w.task_types
                .contains(&draft.work_item_type)
                .then(|| w.cards.get(&p).map(|c| c.iteration_path.clone()))
                .flatten()
        });
        if let Some(it) = iteration.filter(|i| !i.is_empty()) {
            doc = doc.field(fields::ITERATION_PATH, Value::String(it));
        }
    }
    match client
        .create_work_item(&project, &draft.work_item_type, &doc)
        .await
    {
        Ok(dto) => {
            let in_scope = st.workspace.write().as_mut().is_some_and(|ws| {
                ws.upsert(std::slice::from_ref(&dto));
                ws.cards.contains_key(&dto.id)
            });
            if in_scope {
                toast(st, ToastKind::Info, format!("Created #{}", dto.id));
            } else {
                toast(
                    st,
                    ToastKind::Warn,
                    format!(
                        "Created #{}, but its area is outside this team's board.",
                        dto.id
                    ),
                );
            }
            Some(dto.id)
        }
        Err(e) => {
            handle_error(st, &e, "Couldn't create the work item");
            None
        }
    }
}

/// Writes what changed between `original` (revision `rev`) and `draft`, re-parenting if the
/// parent changed. The card updates at once and rolls back if ADO refuses.
pub async fn update_item(
    mut st: AppState,
    id: CardId,
    rev: u32,
    original: &ItemDraft,
    draft: &ItemDraft,
) -> SaveOutcome {
    let Some(client) = st.client.peek().clone() else {
        return SaveOutcome::Failed;
    };
    let Some((project, before, people)) = st
        .workspace
        .peek()
        .as_ref()
        .and_then(|w| Some((w.project.clone(), w.cards.get(&id)?.clone(), assignees(w))))
    else {
        return SaveOutcome::Failed;
    };
    let changes = draft.patch_fields(Some(original));
    let reparent = draft.parent != original.parent;
    if changes.is_empty() && !reparent {
        return SaveOutcome::Saved;
    }
    let mut doc = PatchDocument::update(rev);
    if reparent {
        // Relation indices come from the current item.
        match client.work_item(&project, id).await {
            Ok(full) if full.rev != rev => {
                store_server_copy(st, &full, true);
                return SaveOutcome::Conflict;
            }
            Ok(full) => {
                if let Some(i) = full
                    .relations
                    .iter()
                    .flatten()
                    .position(|r| r.rel == "System.LinkTypes.Hierarchy-Reverse")
                {
                    doc = doc.remove_relation(i);
                }
                if let Some(p) = draft.parent {
                    doc = doc.parent(&client.work_item_url(p));
                }
            }
            Err(e) => {
                handle_error(st, &e, "Couldn't load the item");
                return SaveOutcome::Failed;
            }
        }
    }
    doc = changes.into_iter().fold(doc, |d, (f, v)| d.field(&f, v));
    if let Some(ws) = st.workspace.write().as_mut() {
        if let Some(c) = ws.cards.get_mut(&id) {
            c.title = draft.title.trim().to_owned();
            c.parent = draft.parent;
            c.tags.clone_from(&draft.tags);
            c.state.clone_from(&draft.state);
            c.priority = Some(draft.priority);
            c.assigned_to = people.into_iter().find(|p| p.unique_name == draft.assignee);
            c.target_date = draft.target_date;
            c.area_path.clone_from(&draft.area);
        }
        ws.place();
    }
    begin(st, id);
    let result = client.update_work_item(&project, id, &doc).await;
    end(st, id);
    match result {
        Ok(dto) => {
            store_server_copy(st, &dto, true);
            SaveOutcome::Saved
        }
        Err(AdoError::Conflict { .. }) => {
            if let Some(ws) = st.workspace.write().as_mut() {
                ws.cards.insert(id, before);
                ws.place();
            }
            SaveOutcome::Conflict
        }
        Err(e) => {
            if let Some(ws) = st.workspace.write().as_mut() {
                ws.cards.insert(id, before);
                ws.place();
            }
            handle_error(st, &e, &format!("Couldn't save #{id}"));
            SaveOutcome::Failed
        }
    }
}

/// People to offer in assignee pickers: everyone on a card, plus me.
pub fn assignees(ws: &Workspace) -> Vec<Person> {
    let mut people = kanban_core::card::people(ws.cards.values());
    if let Some(me) = &ws.me
        && !people.iter().any(|p| p.unique_name == me.unique_name)
    {
        people.push(me.clone());
    }
    people
}

/// Root (epic) id → title, for chips and pickers.
pub fn root_titles(ws: &Workspace) -> HashMap<CardId, String> {
    let roots = ws.root_types();
    ws.cards
        .values()
        .filter(|c| roots.contains(&c.work_item_type))
        .map(|c| (c.id, c.title.clone()))
        .collect()
}
