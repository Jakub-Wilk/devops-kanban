//! 7pace time tracking for the kanban: connects with the 7pace token (keyring, never the ADO
//! PAT), loads the user's own worklogs for the board's cards, runs the timer and writes entries.
//!
//! Cost rules (AGENTS.md, 7pace): card totals come from ADO's `CompletedWork`, never from 7pace;
//! entries are fetched once per card (in batches of 100) and again only after a write or Reload;
//! the timer is read on Kanban mount, window focus, after start/stop and every few minutes,
//! because the 7pace web UI or desktop client can change it. After any write the card is
//! refetched from ADO a few seconds later, once 7pace has synced the new total.
//!
//! The Timetable tab reads one week of the user's entries by date (`load_week`, one paged call),
//! again after any write and on Reload, never on a timer. Drags there are applied at once and
//! put back if 7pace refuses (`reschedule`).
//!
//! Stopping goes through the time dialog (`stop_and_log`): 7pace saves the track as a worklog
//! itself, and the dialog's changes are then applied to that entry, so nothing is logged twice.
//!
//! Like `services`, tasks run on the root scope (`spawn_forever`): most calls come from the time
//! dialog, which closes on success and would otherwise cancel the refetch of the card's entries.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use kanban_core::CardId;
use kanban_core::time::{
    Activity, TimeEntry, TimerState, format_duration, parse_time_of_day, tz_minutes, wire_timestamp,
};
use kanban_core::timetable;
use sevenpace_client::models::{StartTracking, WorkLogWrite};
use sevenpace_client::{SevenPaceClient, SevenPaceConfig, SevenPaceError, SevenPaceToken};
use time::{Date, OffsetDateTime, PrimitiveDateTime, UtcOffset};

use crate::services::{self, toast};
use crate::state::{AppState, ItemRef, TimeDialogState, TimeTracking, ToastKind};

/// How long 7pace takes to write `CompletedWork` back to ADO, roughly.
const TOTAL_SYNC_DELAY: Duration = Duration::from_secs(6);
/// 7pace saves a stopped track as a worklog shortly after answering the stop: look for it this
/// many times, this far apart.
const STOP_LOOKUP_TRIES: u32 = 4;
const STOP_LOOKUP_GAP: Duration = Duration::from_millis(1500);
/// Re-read the timer this often, in case it was changed elsewhere.
pub const TIMER_REFRESH: Duration = Duration::from_secs(5 * 60);

/// The offset read at startup, see [`init_local_offset`].
static STARTUP_OFFSET: OnceLock<UtcOffset> = OnceLock::new();

/// Reads the local UTC offset. Call first thing in `main`: on Linux `time` refuses to read it
/// once the process has other threads (tokio, the webview), and would silently give UTC.
pub fn init_local_offset() {
    let _ = STARTUP_OFFSET.set(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC));
}

/// The local UTC offset: the current one where the platform allows reading it (so a DST change
/// is picked up), else the one read at startup.
pub fn local_offset() -> UtcOffset {
    UtcOffset::current_local_offset()
        .ok()
        .or_else(|| STARTUP_OFFSET.get().copied())
        .unwrap_or(UtcOffset::UTC)
}

pub fn now_local() -> OffsetDateTime {
    OffsetDateTime::now_utc().to_offset(local_offset())
}

fn build_client(st: AppState, token: &str) -> Result<SevenPaceClient, String> {
    let settings = st.settings.peek();
    let base_url = url::Url::parse(&settings.time_tracking.base_url).map_err(|e| {
        format!(
            "Invalid 7pace URL {:?}: {e}",
            settings.time_tracking.base_url
        )
    })?;
    Ok(SevenPaceClient::new(
        SevenPaceConfig {
            base_url,
            api_version: settings.time_tracking.api_version.clone(),
        },
        Arc::new(SevenPaceToken::new(token)),
    ))
}

/// Is time tracking usable right now?
pub fn connected(st: AppState) -> bool {
    st.sevenpace.read().is_some() && st.time.read().me.is_some()
}

/// (Re)connects from settings and the keyring: validates the token with `me`, then loads activity
/// types and the running timer. Clears everything when time tracking is off or has no token.
pub fn connect(mut st: AppState) {
    st.sevenpace.set(None);
    st.time.set(TimeTracking::default());
    let settings = st.settings.peek().clone();
    if !settings.time_tracking.enabled || !settings.is_configured() {
        return;
    }
    let token = match kanban_core::config::load_7pace_token(&settings.organization) {
        Ok(Some(t)) => t,
        Ok(None) => {
            st.time.write().error = Some("No 7pace token yet.".into());
            return;
        }
        Err(e) => {
            st.time.write().error = Some(format!("Couldn't read the 7pace token: {e}"));
            return;
        }
    };
    spawn_forever(async move {
        if let Err(e) = connect_with(st, &token).await {
            st.time.write().error = Some(e);
        }
    });
}

async fn connect_with(mut st: AppState, token: &str) -> Result<(), String> {
    let client = build_client(st, token)?;
    let me = client.me().await.map_err(|e| describe(&e))?;
    let name = me
        .user
        .as_ref()
        .and_then(|u| u.display_name.clone().or_else(|| u.unique_name.clone()))
        .unwrap_or_else(|| "7pace user".into());
    let activities = client.activity_types().await.unwrap_or_default();
    {
        let mut t = st.time.write();
        t.me = Some(name);
        t.error = None;
        t.activities_enabled = activities.enabled;
        t.activities = activities
            .activity_types
            .into_iter()
            .map(|a| Activity {
                id: a.id,
                name: a.name,
                color: a.color,
            })
            .collect();
        t.default_activity = me
            .default_activity_type
            .map(|a| a.id)
            .or(activities.system_default_activity_type_id);
    }
    st.sevenpace.set(Some(client));
    refresh_timer(st);
    Ok(())
}

/// Validates `token` against 7pace, stores it in the keyring and connects.
pub async fn save_token(mut st: AppState, token: String) -> Result<String, String> {
    let token = token.trim().to_owned();
    if token.is_empty() {
        return Err("Paste the 7pace API token first.".into());
    }
    let client = build_client(st, &token)?;
    let me = client.me().await.map_err(|e| describe(&e))?;
    let org = st.settings.peek().organization.clone();
    kanban_core::config::store_7pace_token(&org, &token)
        .map_err(|e| format!("Couldn't store the token in the keyring: {e}"))?;
    if !st.settings.peek().time_tracking.enabled {
        st.settings.write().time_tracking.enabled = true;
        services::save_settings(st);
    }
    connect(st);
    Ok(me
        .user
        .and_then(|u| u.display_name)
        .unwrap_or_else(|| "7pace user".into()))
}

pub fn remove_token(mut st: AppState) {
    let org = st.settings.peek().organization.clone();
    if let Err(e) = kanban_core::config::delete_7pace_token(&org) {
        toast(
            st,
            ToastKind::Warn,
            format!("Couldn't remove the 7pace token: {e}"),
        );
    }
    st.sevenpace.set(None);
    st.time.set(TimeTracking::default());
    connect(st);
}

/// The user's own entries on `ids` that aren't loaded yet (all of them with `force`).
pub fn load_entries(mut st: AppState, ids: Vec<CardId>, force: bool) {
    let Some(client) = st.sevenpace.peek().clone() else {
        return;
    };
    let ids: Vec<CardId> = {
        let t = st.time.peek();
        ids.into_iter()
            .filter(|id| force || !t.loaded.contains(id))
            .collect()
    };
    if ids.is_empty() {
        return;
    }
    // Mark them now so a re-render doesn't fetch the same ids twice.
    st.time.write().loaded.extend(ids.iter().copied());
    spawn_forever(async move {
        match client.worklogs(&ids).await {
            Ok(dtos) => {
                let local = local_offset();
                let mut t = st.time.write();
                for id in &ids {
                    t.entries.insert(*id, Vec::new());
                }
                for dto in &dtos {
                    if let Some(e) = TimeEntry::from_dto(dto, local)
                        && let Some(id) = e.work_item
                    {
                        t.entries.entry(id).or_default().push(e);
                    }
                }
                for id in &ids {
                    if let Some(list) = t.entries.get_mut(id) {
                        list.sort_by_key(|e| std::cmp::Reverse(e.start));
                    }
                }
            }
            Err(e) => {
                let mut t = st.time.write();
                for id in &ids {
                    t.loaded.remove(id);
                }
                drop(t);
                report(st, &e, "Couldn't load time entries");
            }
        }
    });
}

/// Fetches entries again for every card the kanban shows (Reload).
pub fn reload(mut st: AppState) {
    let mut t = st.time.write();
    t.loaded.clear();
    t.generation += 1;
    drop(t);
    refresh_timer(st);
    reload_week(st);
}

/// Loads the user's entries for the week starting `monday` (Timetable). 7pace doesn't say whether
/// `$fromTimestamp`/`$toTimestamp` are local or UTC, so this asks for a day more on each side and
/// keeps the entries whose local time touches the week. Items outside the workspace get their
/// titles from ADO afterwards.
pub fn load_week(mut st: AppState, monday: Date) {
    let Some(client) = st.sevenpace.peek().clone() else {
        return;
    };
    {
        let mut t = st.time.write();
        if t.week.monday != Some(monday) {
            t.week.monday = Some(monday);
            t.week.loaded = false;
            t.week.entries.clear();
        }
        t.week.loading = true;
    }
    spawn_forever(async move {
        let from = (monday - time::Duration::days(1)).midnight();
        let to = (monday + time::Duration::days(8)).midnight();
        let result = client
            .worklogs_between(&wire_timestamp(from), &wire_timestamp(to))
            .await;
        // Another week was asked for meanwhile: its own load fills the state.
        if st.time.peek().week.monday != Some(monday) {
            return;
        }
        match result {
            Ok(dtos) => {
                let local = local_offset();
                let week_from = monday.midnight();
                let week_to = (monday + time::Duration::days(7)).midnight();
                let mut entries: Vec<TimeEntry> = dtos
                    .iter()
                    .filter_map(|d| TimeEntry::from_dto(d, local))
                    .filter(|e| {
                        e.start < week_to
                            && (timetable::end_of(e) > week_from || e.start >= week_from)
                    })
                    .collect();
                entries.sort_by(|a, b| (a.start, &a.id).cmp(&(b.start, &b.id)));
                let missing: Vec<CardId> = {
                    let ws = st.workspace.peek();
                    let known = st.time.peek();
                    let mut ids: Vec<CardId> = entries
                        .iter()
                        .filter_map(|e| e.work_item)
                        .filter(|id| {
                            !ws.as_ref().is_some_and(|w| w.cards.contains_key(id))
                                && !known.week.items.contains_key(id)
                        })
                        .collect();
                    ids.sort_unstable();
                    ids.dedup();
                    ids
                };
                {
                    let mut t = st.time.write();
                    t.week.entries = entries;
                    t.week.loading = false;
                    t.week.loaded = true;
                }
                load_item_refs(st, missing);
            }
            Err(e) => {
                st.time.write().week.loading = false;
                report(st, &e, "Couldn't load the week's time entries");
            }
        }
    });
}

/// Loads the shown week again, if the Timetable has shown one.
pub fn reload_week(st: AppState) {
    // Read it out first: a guard in the `if let` would live through `load_week`'s write.
    let monday = st.time.peek().week.monday;
    if let Some(monday) = monday {
        load_week(st, monday);
    }
}

/// Titles and types of `ids` from ADO, across projects (≤200 per call). Best effort: the
/// Timetable shows the bare id when this fails.
fn load_item_refs(mut st: AppState, ids: Vec<CardId>) {
    let Some(ado) = st.client.peek().clone() else {
        return;
    };
    if ids.is_empty() {
        return;
    }
    spawn_forever(async move {
        let fields = [
            kanban_core::card::fields::ID,
            kanban_core::card::fields::TITLE,
            kanban_core::card::fields::TYPE,
        ]
        .map(str::to_owned);
        match ado.work_items_batch_org(&ids, &fields).await {
            Ok(items) => {
                let mut t = st.time.write();
                for w in items {
                    let text = |k: &str| {
                        w.fields
                            .get(k)
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_owned()
                    };
                    t.week.items.insert(
                        w.id,
                        ItemRef {
                            title: text(kanban_core::card::fields::TITLE),
                            work_item_type: text(kanban_core::card::fields::TYPE),
                        },
                    );
                }
            }
            Err(e) => tracing::warn!(error = %e, "couldn't read the titles of logged items"),
        }
    });
}

/// Moves or resizes `entry` to `start` and `seconds` (a Timetable drag, already kept clear of
/// other entries). Shown at once; put back with the error if 7pace refuses, then refetched.
/// Only time and length are sent, so the comment and activity stay as they are.
pub fn reschedule(mut st: AppState, entry: TimeEntry, start: PrimitiveDateTime, seconds: i64) {
    let Some(client) = st.sevenpace.peek().clone() else {
        return;
    };
    if start == entry.start && seconds == entry.seconds {
        return;
    }
    let mut set = move |e: &TimeEntry| {
        let mut t = st.time.write();
        if let Some(x) = t.week.entries.iter_mut().find(|x| x.id == e.id) {
            *x = e.clone();
        }
        t.week
            .entries
            .sort_by(|a, b| (a.start, &a.id).cmp(&(b.start, &b.id)));
    };
    set(&TimeEntry {
        start,
        seconds,
        ..entry.clone()
    });
    spawn_forever(async move {
        let body = WorkLogWrite {
            time_stamp: wire_timestamp(start),
            length: seconds,
            work_item_id: entry.work_item,
            comment: None,
            activity_type_id: None,
        };
        match client.update_worklog(&entry.id, &body).await {
            Ok(_) => match entry.work_item {
                Some(card) => after_write(st, card),
                None => reload_week(st),
            },
            Err(e) => {
                set(&entry);
                report(st, &e, "Couldn't change the entry");
                reload_week(st);
            }
        }
    });
}

/// Reads the running timer from 7pace, which may have been started or stopped elsewhere (the
/// 7pace web UI or desktop client). A failure keeps the last known timer and is reported once,
/// until a read succeeds again.
pub fn refresh_timer(mut st: AppState) {
    let Some(client) = st.sevenpace.peek().clone() else {
        return;
    };
    spawn_forever(async move {
        match client.current_tracking().await {
            Ok(dto) => {
                let timer = TimerState::from_dto(&dto, now_local(), local_offset());
                let t = st.time.peek();
                if t.timer != timer || t.timer_read_failed {
                    drop(t);
                    let mut t = st.time.write();
                    t.timer = timer;
                    t.timer_read_failed = false;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "couldn't read the 7pace timer");
                if !st.time.peek().timer_read_failed {
                    st.time.write().timer_read_failed = true;
                    report(st, &e, "Couldn't read the 7pace timer");
                }
            }
        }
    });
}

/// Starts the timer on `id`, switching from another card if one is running.
pub fn start(mut st: AppState, id: CardId) {
    let Some(client) = st.sevenpace.peek().clone() else {
        return;
    };
    let previous = st.time.peek().timer.as_ref().map(|t| t.work_item);
    let activity = st.time.peek().default_activity.clone();
    st.time.write().busy = true;
    spawn_forever(async move {
        let body = StartTracking {
            time_zone: tz_minutes(local_offset()),
            tfs_id: id,
            remark: None,
            activity_type_id: activity,
        };
        let result = client.start_tracking(&body).await;
        st.time.write().busy = false;
        match result {
            Ok(dto) => {
                if let Some(msg) = refused(&dto) {
                    toast(
                        st,
                        ToastKind::Warn,
                        format!("7pace didn't start the timer: {msg}"),
                    );
                }
                st.time.write().timer = TimerState::from_dto(&dto, now_local(), local_offset());
                if let Some(p) = previous.filter(|p| *p != id) {
                    toast(
                        st,
                        ToastKind::Info,
                        format!("Timer switched from #{p} to #{id}."),
                    );
                    after_write(st, p);
                }
                // 7pace may answer before the new track shows up; confirm shortly after.
                refresh_timer(st);
            }
            Err(e) => report(st, &e, "Couldn't start the timer"),
        }
    });
}

/// Saves the dialog (create, or update when editing). Returns `Err` with a message for the form.
pub async fn save_entry(st: AppState, form: TimeDialogState) -> Result<(), String> {
    let client = st
        .sevenpace
        .peek()
        .clone()
        .ok_or("7pace isn't connected.")?;
    let body = write_of(&form)?;
    let seconds = body.length;
    let result = match &form.editing {
        Some(id) => client.update_worklog(id, &body).await,
        None => client.create_worklog(&body).await,
    };
    match result {
        Ok(_) => {
            let what = if form.editing.is_some() {
                "Updated the entry"
            } else {
                "Logged"
            };
            toast(
                st,
                ToastKind::Info,
                format!("{what}: {} on #{}.", format_duration(seconds), form.card),
            );
            after_write(st, form.card);
            Ok(())
        }
        Err(e) => Err(describe(&e)),
    }
}

/// The dialog opened from Stop: stops the timer (7pace saves the track as a worklog itself, so
/// no second entry is created), then finds that worklog and applies what the user changed in
/// the dialog (duration, timing, activity, comment). Untouched, 7pace's exact length stays.
pub async fn stop_and_log(mut st: AppState, form: TimeDialogState) -> Result<(), String> {
    let client = st
        .sevenpace
        .peek()
        .clone()
        .ok_or("7pace isn't connected.")?;
    let Some(timer) = st.time.peek().timer.clone() else {
        // Stopped elsewhere meanwhile (7pace UI, desktop client): log it as a normal entry.
        return save_entry(
            st,
            TimeDialogState {
                from_timer: false,
                ..form
            },
        )
        .await;
    };
    let mut body = write_of(&form)?;
    st.time.write().busy = true;
    let stopped = client.stop_tracking().await;
    st.time.write().busy = false;
    if let Err(e) = stopped {
        refresh_timer(st);
        return Err(describe(&e));
    }
    st.time.write().timer = None;
    let card = timer.work_item;
    let started = timer.started.to_offset(local_offset());
    let started = PrimitiveDateTime::new(started.date(), started.time());
    let logged = client
        .worklogs(&[card])
        .await
        .map_err(|e| format!("Stopped, but couldn't read the new entry: {}", describe(&e)))?;
    let local = local_offset();
    let find = |logged: &[sevenpace_client::models::WorkLogDto]| {
        logged
            .iter()
            .filter_map(|d| TimeEntry::from_dto(d, local))
            .filter(|e| e.work_item == Some(card))
            .min_by_key(|e| (e.start - started).whole_seconds().abs())
            .filter(|e| (e.start - started).whole_minutes().abs() <= 5)
    };
    let mut entry = find(&logged);
    // The worklog may not exist yet right after the stop.
    for _ in 1..STOP_LOOKUP_TRIES {
        if entry.is_some() {
            break;
        }
        tokio::time::sleep(STOP_LOOKUP_GAP).await;
        match client.worklogs(&[card]).await {
            Ok(logged) => entry = find(&logged),
            Err(e) => tracing::warn!(error = %e, "couldn't re-read the stopped timer's worklog"),
        }
    }
    let Some(entry) = entry else {
        toast(
            st,
            ToastKind::Warn,
            format!(
                "Stopped. 7pace logged the timer on #{card}, but the entry wasn't found to apply your changes; edit it on the card."
            ),
        );
        after_write(st, card);
        return Ok(());
    };
    // Duration and timing untouched: keep 7pace's own start and exact length.
    if form.minutes == form.timer_minutes && form.ends_now && form.date == now_local().date() {
        body.time_stamp = wire_timestamp(entry.start);
        body.length = entry.seconds;
    }
    let same = body.length == entry.seconds
        && body.time_stamp == wire_timestamp(entry.start)
        && body.comment == entry.comment
        && (body.activity_type_id.is_none()
            || body.activity_type_id == entry.activity.as_ref().map(|a| a.id.clone()));
    if !same && let Err(e) = client.update_worklog(&entry.id, &body).await {
        after_write(st, card);
        return Err(format!(
            "Stopped and logged, but your changes weren't applied: {}",
            describe(&e)
        ));
    }
    toast(
        st,
        ToastKind::Info,
        format!("Logged {} on #{card}.", format_duration(body.length)),
    );
    after_write(st, card);
    Ok(())
}

/// The worklog body for the dialog's values.
fn write_of(form: &TimeDialogState) -> Result<WorkLogWrite, String> {
    if form.card == 0 {
        return Err("Pick the work item to log the time on.".into());
    }
    if form.minutes <= 0 {
        return Err("Enter how long you worked.".into());
    }
    let seconds = form.minutes * 60;
    let start = start_of(form, seconds)?;
    Ok(WorkLogWrite {
        time_stamp: wire_timestamp(start),
        length: seconds,
        work_item_id: Some(form.card),
        comment: Some(form.comment.trim().to_owned()).filter(|c| !c.is_empty()),
        activity_type_id: Some(form.activity.clone()).filter(|a| !a.is_empty()),
    })
}

pub async fn delete_entry(st: AppState, card: CardId, id: String) -> Result<(), String> {
    let client = st
        .sevenpace
        .peek()
        .clone()
        .ok_or("7pace isn't connected.")?;
    client.delete_worklog(&id).await.map_err(|e| describe(&e))?;
    after_write(st, card);
    Ok(())
}

/// The entry's start: the form's date at its start time, or ending now when "Ends now".
fn start_of(form: &TimeDialogState, seconds: i64) -> Result<PrimitiveDateTime, String> {
    if form.ends_now {
        let end = PrimitiveDateTime::new(form.date, now_local().time());
        return Ok(end - time::Duration::seconds(seconds));
    }
    let start = parse_time_of_day(&form.start)
        .ok_or_else(|| "Enter the start time as HH:MM, 24-hour (00:00 to 23:59).".to_owned())?;
    Ok(PrimitiveDateTime::new(form.date, start))
}

/// Reloads the card's entries and the Timetable's week now, and the card's ADO total once 7pace
/// has synced it.
fn after_write(st: AppState, card: CardId) {
    load_entries(st, vec![card], true);
    reload_week(st);
    services::reload_card_later(st, card, TOTAL_SYNC_DELAY);
}

/// `trackSettings` can refuse a start without an HTTP error.
fn refused(dto: &sevenpace_client::models::TrackingStateDto) -> Option<String> {
    let s = dto.track_settings.as_ref()?;
    let state = s.response_state.as_ref().map(|v| match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    });
    match state.as_deref() {
        Some("Error" | "AuthError" | "2" | "3") => Some(
            s.response_message
                .clone()
                .unwrap_or_else(|| "no reason given".into()),
        ),
        _ => None,
    }
}

fn describe(e: &SevenPaceError) -> String {
    match e {
        SevenPaceError::Unauthorized => {
            "7pace rejected the token. Create a new one in 7pace (Settings → Reporting & API) and paste it in Settings.".into()
        }
        other => other.to_string(),
    }
}

/// Toasts 7pace's message verbatim; a rejected token also marks time tracking as disconnected
/// (ADO keeps working).
fn report(mut st: AppState, e: &SevenPaceError, what: &str) {
    if matches!(e, SevenPaceError::Unauthorized) {
        st.sevenpace.set(None);
        let mut t = st.time.write();
        t.me = None;
        t.error = Some(describe(e));
    }
    toast(st, ToastKind::Error, format!("{what}: {}", describe(e)));
}
