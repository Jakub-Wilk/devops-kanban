//! 7pace time tracking for the kanban: connects with the 7pace token (keyring, never the ADO
//! PAT), loads the user's own worklogs for the board's cards, runs the timer and writes entries.
//!
//! Cost rules (AGENTS.md, 7pace): card totals come from ADO's `CompletedWork`, never from 7pace;
//! entries are fetched once per card (in batches of 100) and again only after a write or Reload;
//! the timer is read on Kanban mount, window focus, after start/stop and every few minutes,
//! because the 7pace web UI or desktop client can change it. After any write the card is
//! refetched from ADO a few seconds later, once 7pace has synced the new total.

use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;
use kanban_core::CardId;
use kanban_core::time::{
    Activity, TimeEntry, TimerState, format_duration, parse_duration, tz_minutes, wire_timestamp,
};
use sevenpace_client::models::{StartTracking, WorkLogWrite};
use sevenpace_client::{SevenPaceClient, SevenPaceConfig, SevenPaceError, SevenPaceToken};
use time::{OffsetDateTime, PrimitiveDateTime, UtcOffset};

use crate::services::{self, toast};
use crate::state::{AppState, TimeDialogState, TimeTracking, ToastKind};

/// How long 7pace takes to write `CompletedWork` back to ADO, roughly.
const TOTAL_SYNC_DELAY: Duration = Duration::from_secs(6);
/// Re-read the timer this often, in case it was changed elsewhere.
pub const TIMER_REFRESH: Duration = Duration::from_secs(5 * 60);

pub fn local_offset() -> UtcOffset {
    UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC)
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
    spawn(async move {
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
    spawn(async move {
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
}

/// Reads the running timer from 7pace.
pub fn refresh_timer(mut st: AppState) {
    let Some(client) = st.sevenpace.peek().clone() else {
        return;
    };
    spawn(async move {
        match client.current_tracking().await {
            Ok(dto) => {
                let timer = TimerState::from_dto(&dto, now_local(), local_offset());
                if st.time.peek().timer != timer {
                    st.time.write().timer = timer;
                }
            }
            Err(e) => tracing::warn!(error = %e, "couldn't read the 7pace timer"),
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
    spawn(async move {
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

/// Stops the running timer; 7pace saves it as a worklog.
pub fn stop(mut st: AppState) {
    let Some(client) = st.sevenpace.peek().clone() else {
        return;
    };
    let running = st.time.peek().timer.as_ref().map(|t| t.work_item);
    st.time.write().busy = true;
    spawn(async move {
        let result = client.stop_tracking().await;
        st.time.write().busy = false;
        match result {
            Ok(_) => {
                st.time.write().timer = None;
                if let Some(id) = running {
                    after_write(st, id);
                }
            }
            Err(e) => {
                report(st, &e, "Couldn't stop the timer");
                refresh_timer(st);
            }
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
    let seconds =
        parse_duration(&form.duration).ok_or("Enter a duration like 1:30, 1h 30m, 90m or 1.5.")?;
    let start = start_of(&form, seconds)?;
    let body = WorkLogWrite {
        time_stamp: wire_timestamp(start),
        length: seconds,
        work_item_id: Some(form.card),
        comment: Some(form.comment.trim().to_owned()).filter(|c| !c.is_empty()),
        activity_type_id: Some(form.activity.clone()).filter(|a| !a.is_empty()),
    };
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

/// The entry's start: the form's date and time, or "ends now" when the time is empty.
fn start_of(form: &TimeDialogState, seconds: i64) -> Result<PrimitiveDateTime, String> {
    let date = time::Date::parse(
        form.date.trim(),
        time::macros::format_description!("[year]-[month]-[day]"),
    )
    .map_err(|_| "Pick a date.".to_owned())?;
    if form.start.trim().is_empty() {
        let now = now_local();
        let end = PrimitiveDateTime::new(date, now.time());
        return Ok(end - time::Duration::seconds(seconds));
    }
    let start = time::Time::parse(
        form.start.trim(),
        time::macros::format_description!("[hour]:[minute]"),
    )
    .map_err(|_| "Enter the start time as HH:MM.".to_owned())?;
    Ok(PrimitiveDateTime::new(date, start))
}

/// Reloads the card's entries now and its ADO total once 7pace has synced it.
fn after_write(st: AppState, card: CardId) {
    load_entries(st, vec![card], true);
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
