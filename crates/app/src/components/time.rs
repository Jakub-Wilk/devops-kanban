//! 7pace time tracking UI: on each kanban card the timer and add-time actions (header), the total
//! (footer) and the user's entries (when the card is expanded), the running-timer chip in the
//! top bar (compact, widens on hover/focus), the add/edit time dialog (also opened by Stop, to
//! log the timer's time, and from the Timetable, where it can ask for the work item), and
//! `TimeKeeper`,
//! which connects at startup, ticks the clock while a timer runs and re-reads the timer on window
//! focus and every few minutes.

use std::time::Duration;

use dioxus::desktop::tao::event::Event;
use dioxus::desktop::{WindowEvent, use_wry_event_handler};
use dioxus::prelude::*;
use kanban_core::time::{
    MAX_ENTRY_MINUTES, TimeEntry, clock_input, format_clock, format_duration, format_hmm,
    format_time_of_day, hmm_from_input, parse_hmm, parse_time_of_day,
};
use kanban_core::{Card, CardId};
use time::PrimitiveDateTime;

use crate::components::icons;
use crate::components::item_fields::{ParentOption, ParentPicker};
use crate::components::pickers::{Calendar, day_label, long_date, relative_day};
use crate::components::widgets::{Icon, short_date};
use crate::state::{AppState, TimeDialogState, use_app};
use crate::time_tracking::{self, TIMER_REFRESH, now_local};

/// Entries shown on a card before "Show all", and in the dialog's "Your time".
const SHOWN_ENTRIES: usize = 3;
/// "Starts at" before the user picks a time.
const DEFAULT_START: &str = "09:00";
/// The duration steps: a quarter hour, an hour, two hours.
const STEPS: [i64; 6] = [15, 60, 120, -15, -60, -120];
/// Activities in the dialog's "Recent" group.
const RECENT_ACTIVITIES: usize = 3;

/// Mounted once at the root.
#[component]
pub fn TimeKeeper() -> Element {
    let mut st = use_app();
    use_hook(|| time_tracking::connect(st));
    use_wry_event_handler(move |event, _| {
        if let Event::WindowEvent {
            event: WindowEvent::Focused(true),
            ..
        } = event
        {
            time_tracking::refresh_timer(st);
        }
    });
    use_future(move || async move {
        let mut since_refresh = Duration::ZERO;
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if st.time.peek().timer.is_some() {
                st.clock.set(now_local());
            }
            since_refresh += Duration::from_secs(1);
            if since_refresh >= TIMER_REFRESH {
                since_refresh = Duration::ZERO;
                time_tracking::refresh_timer(st);
            }
        }
    });
    rsx! { TimeDialog {} }
}

/// Fetches the user's entries for `ids` — every card the kanban shows and the tracked one — (only
/// ones not loaded yet), and again after Reload; re-reads the timer when the kanban opens.
/// Renders nothing.
#[component]
pub fn EntryLoader(ids: Vec<kanban_core::CardId>) -> Element {
    let st = use_app();
    let connected = time_tracking::connected(st);
    let generation = st.time.read().generation;
    // The 7pace UI may have started or stopped the timer since we last looked.
    use_hook(|| time_tracking::refresh_timer(st));
    use_effect(use_reactive!(|ids, connected, generation| {
        let _ = generation;
        if connected {
            time_tracking::load_entries(st, ids, false);
        }
    }));
    rsx! {}
}

/// Opens the dialog to add time to `card` (empty), or to edit `entry`.
pub fn open_time_dialog(mut st: AppState, card: CardId, entry: Option<&TimeEntry>) {
    let default_activity = st.time.peek().default_activity.clone().unwrap_or_default();
    let state = match entry {
        Some(e) => {
            let minutes = (e.seconds + 30) / 60;
            TimeDialogState {
                card,
                pick_item: false,
                editing: Some(e.id.clone()),
                can_delete: e.can_delete,
                from_timer: false,
                timer_minutes: 0,
                date: e.start.date(),
                minutes,
                duration_text: format_hmm(minutes),
                ends_now: false,
                start: format_time_of_day(e.start.time()),
                activity: e
                    .activity
                    .as_ref()
                    .map(|a| a.id.clone())
                    .unwrap_or(default_activity),
                comment: e.comment.clone().unwrap_or_default(),
            }
        }
        None => TimeDialogState {
            card,
            pick_item: false,
            editing: None,
            can_delete: false,
            from_timer: false,
            timer_minutes: 0,
            date: now_local().date(),
            minutes: 0,
            duration_text: String::new(),
            ends_now: true,
            start: DEFAULT_START.into(),
            activity: default_activity,
            comment: String::new(),
        },
    };
    st.time_dialog.set(Some(state));
}

/// Opens the dialog for time at a known place (the Timetable's "Log time", or "More options"
/// after a drag): `card` preset or asked for, starting at `start` for `seconds` (`0` = empty).
pub fn open_time_dialog_at(
    mut st: AppState,
    card: Option<CardId>,
    start: PrimitiveDateTime,
    seconds: i64,
    activity: Option<String>,
) {
    let default_activity = st.time.peek().default_activity.clone().unwrap_or_default();
    let minutes = (seconds + 30) / 60;
    st.time_dialog.set(Some(TimeDialogState {
        card: card.unwrap_or(0),
        pick_item: true,
        editing: None,
        can_delete: false,
        from_timer: false,
        timer_minutes: 0,
        date: start.date(),
        minutes,
        duration_text: format_hmm(minutes),
        ends_now: seconds == 0,
        start: format_time_of_day(start.time()),
        activity: activity.unwrap_or(default_activity),
        comment: String::new(),
    }));
}

/// Items to log time on, for the dialog's and the Timetable's picker: the ones logged on this
/// week and the tracked one first ("recent"), then every open card of the workspace.
pub fn loggable_items(st: AppState) -> Vec<ParentOption> {
    let ws = st.workspace.read();
    let t = st.time.read();
    let mut recent: Vec<CardId> = t.week.entries.iter().filter_map(|e| e.work_item).collect();
    recent.extend(t.timer.as_ref().map(|x| x.work_item));
    let mut out: Vec<ParentOption> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let option = |id: CardId, is_recent: bool| -> Option<ParentOption> {
        if let Some(c) = ws.as_ref().and_then(|w| w.cards.get(&id)) {
            let w = ws.as_ref()?;
            return Some(ParentOption {
                id,
                title: c.title.clone(),
                type_color: w.type_color(&c.work_item_type),
                context: c.work_item_type.clone(),
                state: c.state.clone(),
                category: w.category(c),
                state_color: w.state_color(c),
                recent: is_recent,
            });
        }
        let r = t.week.items.get(&id)?;
        Some(ParentOption {
            id,
            title: r.title.clone(),
            type_color: "6b7686".into(),
            context: format!("{} · another team or project", r.work_item_type),
            state: String::new(),
            category: kanban_core::StateCategory::Proposed,
            state_color: String::new(),
            recent: is_recent,
        })
    };
    for id in recent {
        if seen.insert(id)
            && let Some(o) = option(id, true)
        {
            out.push(o);
        }
    }
    if let Some(w) = ws.as_ref() {
        let mut cards: Vec<_> = w
            .cards
            .values()
            .filter(|c| {
                !matches!(
                    w.category(c),
                    kanban_core::StateCategory::Completed | kanban_core::StateCategory::Removed
                )
            })
            .map(|c| c.id)
            .collect();
        cards.sort_unstable_by(|a, b| b.cmp(a));
        for id in cards {
            if seen.insert(id)
                && let Some(o) = option(id, false)
            {
                out.push(o);
            }
        }
    }
    out
}

/// Stop: opens the dialog with the timer's time. The timer keeps running until the user
/// confirms; Cancel leaves it running.
pub fn open_stop_dialog(mut st: AppState) {
    let Some(timer) = st.time.peek().timer.clone() else {
        return;
    };
    let minutes = (timer.elapsed_secs(now_local()) / 60).clamp(1, MAX_ENTRY_MINUTES);
    let activity = st.time.peek().default_activity.clone().unwrap_or_default();
    st.time_dialog.set(Some(TimeDialogState {
        card: timer.work_item,
        pick_item: false,
        editing: None,
        can_delete: false,
        from_timer: true,
        timer_minutes: minutes,
        date: now_local().date(),
        minutes,
        duration_text: format_hmm(minutes),
        ends_now: true,
        start: DEFAULT_START.into(),
        activity,
        comment: String::new(),
    }));
}

/// Timer and "add time" for a kanban card's header. The tracked card shows its live clock and a
/// stop button; others show start and add time, revealed on hover or keyboard focus.
#[component]
pub fn CardTimeActions(card: Card) -> Element {
    let st = use_app();
    if !time_tracking::connected(st) {
        return rsx! {};
    }
    let id = card.id;
    let t = st.time.read();
    let running = t.timer.as_ref().filter(|x| x.work_item == id).cloned();
    let busy = t.busy;
    drop(t);
    // Only the tracked card reads the clock, so only it re-renders every second.
    let elapsed = running.map(|r| r.elapsed_secs(*st.clock.read()));
    rsx! {
        if let Some(s) = elapsed {
            span { class: "k-live", "data-tip": "Tracking", i {} span { class: "clock", "{format_clock(s)}" } }
            button {
                class: "k-ico stop",
                r#type: "button",
                draggable: "false",
                disabled: busy,
                "data-tip": "Stop tracking",
                aria_label: "Stop tracking",
                onclick: move |e| {
                    e.stop_propagation();
                    open_stop_dialog(st);
                },
                Icon { svg: icons::STOP }
            }
        } else {
            button {
                class: "k-ico q",
                r#type: "button",
                draggable: "false",
                disabled: busy,
                "data-tip": "Start tracking time on this item",
                aria_label: "Start tracking",
                onclick: move |e| {
                    e.stop_propagation();
                    time_tracking::start(st, id);
                },
                Icon { svg: icons::PLAY }
            }
            button {
                class: "k-ico q",
                r#type: "button",
                draggable: "false",
                "data-tip": "Add time",
                aria_label: "Add time",
                onclick: move |e| {
                    e.stop_propagation();
                    open_time_dialog(st, id, None);
                },
                Icon { svg: icons::CLOCK_PLUS }
            }
        }
    }
}

/// Time tracked on the card by everyone, from `CompletedWork` (works without 7pace).
#[component]
pub fn CardTimeTotal(completed_work: Option<f64>) -> Element {
    let Some(total) = completed_work
        .filter(|h| *h > 0.0)
        .map(|h| format_duration((h * 3600.0).round() as i64))
    else {
        return rsx! {};
    };
    rsx! {
        span { class: "total", "data-tip": "Tracked in total, by everyone",
            Icon { svg: icons::CLOCK }
            "{total}"
        }
    }
}

/// The user's 7pace entries on an expanded card.
#[component]
pub fn CardTimeLog(card: Card) -> Element {
    let st = use_app();
    let mut show_all = use_signal(|| false);
    if !time_tracking::connected(st) {
        return rsx! {};
    }
    let entries: Vec<TimeEntry> = st
        .time
        .read()
        .entries
        .get(&card.id)
        .cloned()
        .unwrap_or_default();
    let n = entries.len();
    let loaded = st.time.read().loaded.contains(&card.id);
    let shown: Vec<TimeEntry> = if show_all() {
        entries
    } else {
        entries.into_iter().take(SHOWN_ENTRIES).collect()
    };
    let id = card.id;
    rsx! {
        div { class: "time-log",
            div { class: "sec",
                "Your time"
                button {
                    class: "link sm",
                    r#type: "button",
                    draggable: "false",
                    onclick: move |e| {
                        e.stop_propagation();
                        open_time_dialog(st, id, None);
                    },
                    Icon { svg: icons::PLUS_14 }
                    "Time"
                }
            }
            if shown.is_empty() {
                span { class: "none", if loaded { "No entries yet" } else { "Loading…" } }
            }
            div { class: "entries",
                for e in shown {
                    EntryRow { key: "{e.id}", card: card.clone(), entry: e }
                }
                if n > SHOWN_ENTRIES {
                    button {
                        class: "link sm",
                        r#type: "button",
                        draggable: "false",
                        onclick: move |e| {
                            e.stop_propagation();
                            show_all.toggle();
                        },
                        if show_all() { "Show fewer" } else { "Show all ({n})" }
                    }
                }
            }
        }
    }
}

#[component]
fn EntryRow(card: Card, entry: TimeEntry) -> Element {
    let st = use_app();
    let when = short_date(entry.start.date());
    let len = format_duration(entry.seconds);
    let dot = entry
        .activity
        .as_ref()
        .and_then(|a| a.color.clone())
        .map(|c| format!("background: #{c}"));
    let tip = match (entry.can_edit, &entry.activity) {
        (false, _) => "7pace doesn't allow editing this entry (approved week?)".to_owned(),
        (true, Some(a)) => format!("{} · click to edit", a.name),
        (true, None) => "Click to edit".to_owned(),
    };
    let can_edit = entry.can_edit;
    rsx! {
        button {
            class: "entry",
            r#type: "button",
            draggable: "false",
            disabled: !can_edit,
            "data-tip": "{tip}",
            onclick: move |e| {
                e.stop_propagation();
                open_time_dialog(st, card.id, Some(&entry));
            },
            span { class: "dot", style: dot.unwrap_or_default() }
            span { class: "when", "{when}" }
            span { class: "len", "{len}" }
            if let Some(c) = entry.comment.clone() { span { class: "note", "{c}" } }
        }
    }
}

/// The running timer in the top bar, on every tab.
#[component]
pub fn TimerChip() -> Element {
    let st = use_app();
    let t = st.time.read();
    let Some(timer) = t.timer.clone() else {
        return rsx! {};
    };
    let busy = t.busy;
    drop(t);
    let title = st
        .workspace
        .read()
        .as_ref()
        .and_then(|w| w.cards.get(&timer.work_item).map(|c| c.title.clone()))
        .or(timer.title.clone())
        .unwrap_or_default();
    let elapsed = timer.elapsed_secs(*st.clock.read());
    rsx! {
        // Compact: dot and clock. Hover or keyboard focus widens it to the item and Stop.
        div { class: "timer-chip", role: "status", aria_label: "Tracking #{timer.work_item} {title}, {format_clock(elapsed)}",
            span { class: "rec" }
            button { class: "tc-clock clock", r#type: "button", aria_label: "Show the tracked item", "{format_clock(elapsed)}" }
            span { class: "tc-more",
                span { class: "tc-title", "#{timer.work_item} {title}" }
                button {
                    class: "tc-stop",
                    r#type: "button",
                    disabled: busy,
                    "data-tip": "Stop tracking",
                    aria_label: "Stop tracking",
                    onclick: move |_| open_stop_dialog(st),
                    Icon { svg: icons::STOP }
                }
            }
        }
    }
}

/// Mounted inside `TimeKeeper`; renders while the dialog is open.
#[component]
fn TimeDialog() -> Element {
    let st = use_app();
    if st.time_dialog.read().is_none() {
        return rsx! {};
    }
    rsx! { TimeDialogBody {} }
}

fn hm(t: time::Time) -> String {
    format!("{:02}:{:02}", t.hour(), t.minute())
}

#[component]
fn TimeDialogBody() -> Element {
    let mut st = use_app();
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut confirm_delete = use_signal(|| false);
    let mut cal_open = use_signal(|| false);
    let mut act_open = use_signal(|| false);
    let mut act_filter = use_signal(String::new);
    let Some(form) = st.time_dialog.read().clone() else {
        return rsx! {};
    };
    let initial = use_hook(|| form.clone());
    let card_id = form.card;
    let has_card = card_id != 0;
    // "Your time on this item" needs the card's entries, which the kanban may not have loaded.
    use_effect(use_reactive!(|card_id| {
        if card_id != 0 {
            time_tracking::load_entries(st, vec![card_id], false);
        }
    }));

    let t = st.time.read();
    let activities = if t.activities_enabled {
        t.activities.clone()
    } else {
        Vec::new()
    };
    let default_activity = t.default_activity.clone().unwrap_or_default();
    let entries: Vec<TimeEntry> = t.entries.get(&card_id).cloned().unwrap_or_default();
    let mut recent_ids: Vec<String> = Vec::new();
    {
        let mut all: Vec<&TimeEntry> = t.entries.values().flatten().collect();
        all.sort_by_key(|e| std::cmp::Reverse(e.start));
        for e in all {
            if let Some(a) = &e.activity
                && !recent_ids.contains(&a.id)
            {
                recent_ids.push(a.id.clone());
            }
            if recent_ids.len() == RECENT_ACTIVITIES {
                break;
            }
        }
    }
    let timer_title = t
        .timer
        .as_ref()
        .filter(|x| x.work_item == card_id)
        .and_then(|x| x.title.clone());
    let elsewhere_title = t.week.items.get(&card_id).map(|r| r.title.clone());
    drop(t);

    // The card, for the context strip.
    let ws_guard = st.workspace.read();
    let card = ws_guard
        .as_ref()
        .and_then(|w| w.cards.get(&card_id))
        .cloned();
    let (type_color, context, total) = match (ws_guard.as_ref(), &card) {
        (Some(ws), Some(c)) => {
            let parent = c
                .parent
                .and_then(|p| ws.cards.get(&p))
                .map(|p| p.title.clone());
            let ctx = [
                Some(c.work_item_type.clone()),
                parent,
                Some(c.state.clone()),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            (
                ws.type_color(&c.work_item_type),
                ctx,
                c.completed_work
                    .filter(|h| *h > 0.0)
                    .map(|h| format_duration((h * 3600.0).round() as i64)),
            )
        }
        _ => ("6b7686".to_owned(), String::new(), None),
    };
    drop(ws_guard);
    let card_title = card
        .as_ref()
        .map(|c| c.title.clone())
        .or(timer_title)
        .or(elsewhere_title)
        .unwrap_or_default();

    let parsed = parse_hmm(&form.duration_text);
    let minutes = form.minutes;
    let has_time = parsed.is_some() && minutes > 0;
    let editing = form.editing.is_some();
    let now = now_local();
    let today = now.date();
    let start_time = parse_time_of_day(&form.start);
    let span = time::Duration::minutes(minutes);
    let (from, to) = if form.ends_now {
        (Some(now.time() - span), Some(now.time()))
    } else {
        (start_time, start_time.map(|s| s + span))
    };
    let range = from.zip(to).map(|(a, b)| (hm(a), hm(b)));

    let activity = activities
        .iter()
        .find(|a| a.id == form.activity)
        .or_else(|| activities.iter().find(|a| a.id == default_activity))
        .cloned();
    let act_color = |c: &Option<String>| {
        c.as_ref()
            .map_or_else(|| "var(--faint)".to_owned(), |c| format!("#{c}"))
    };
    let filter = act_filter().trim().to_lowercase();
    let matching: Vec<_> = activities
        .iter()
        .filter(|a| filter.is_empty() || a.name.to_lowercase().contains(&filter))
        .cloned()
        .collect();
    let recent_acts: Vec<_> = if filter.is_empty() {
        recent_ids
            .iter()
            .filter_map(|id| activities.iter().find(|a| a.id == *id).cloned())
            .collect()
    } else {
        Vec::new()
    };
    let mut act_groups = Vec::new();
    if !recent_acts.is_empty() {
        act_groups.push(("Recent".to_owned(), recent_acts));
    }
    if !matching.is_empty() {
        act_groups.push((
            if filter.is_empty() {
                "All activities".to_owned()
            } else {
                format!("Matches · {}", matching.len())
            },
            matching,
        ));
    }

    // "Your time on this item": before and after this save.
    let edited_secs = form
        .editing
        .as_ref()
        .and_then(|id| entries.iter().find(|e| e.id == *id))
        .map_or(0, |e| e.seconds);
    let mine_before: i64 = entries.iter().map(|e| e.seconds).sum();
    let mine_after = mine_before - edited_secs + if has_time { minutes * 60 } else { 0 };
    let loaded = st.time.read().loaded.contains(&card_id);
    let shown: Vec<TimeEntry> = entries.iter().take(SHOWN_ENTRIES).cloned().collect();
    let act_name = activity.as_ref().map(|a| a.name.clone());
    let act_dot = act_color(&activity.as_ref().and_then(|a| a.color.clone()));
    let act_suffix = act_name
        .as_ref()
        .map(|n| format!(" · {n}"))
        .unwrap_or_default();
    let new_note = format!("New{act_suffix}");

    let title = if form.from_timer {
        "Stop and log time"
    } else if editing {
        "Edit time"
    } else {
        "Add time"
    };
    let dur_hint = match parsed {
        None if form
            .duration_text
            .split(':')
            .nth(1)
            .and_then(|m| m.parse::<i64>().ok())
            .is_some_and(|m| m >= 60) =>
        {
            Some(("hint bad", "Minutes go up to 59".to_owned()))
        }
        None => Some(("hint bad", "At most 24:00".to_owned())),
        Some(_) if form.from_timer => Some(("hint", "From the timer · edit if needed".to_owned())),
        Some(_) => None,
    };
    let dirty = form != initial;

    let mut set = move |f: &dyn Fn(&mut TimeDialogState)| {
        if let Some(d) = st.time_dialog.write().as_mut() {
            f(d);
        }
    };
    let mut close = move || {
        if !*saving.peek() {
            st.time_dialog.set(None);
        }
    };
    let save = move || {
        let Some(f) = st.time_dialog.peek().clone() else {
            return;
        };
        if *saving.peek() || f.card == 0 || f.minutes <= 0 || parse_hmm(&f.duration_text).is_none()
        {
            return;
        }
        saving.set(true);
        error.set(None);
        spawn(async move {
            let result = if f.from_timer {
                time_tracking::stop_and_log(st, f).await
            } else {
                time_tracking::save_entry(st, f).await
            };
            saving.set(false);
            match result {
                Ok(()) => st.time_dialog.set(None),
                Err(e) => error.set(Some(e)),
            }
        });
    };
    let mut save_key = save;
    let mut save_click = save;
    let delete = move |_| {
        if !confirm_delete() {
            confirm_delete.set(true);
            return;
        }
        let Some(f) = st.time_dialog.peek().clone() else {
            return;
        };
        let Some(id) = f.editing.clone() else {
            return;
        };
        saving.set(true);
        spawn(async move {
            let result = time_tracking::delete_entry(st, f.card, id).await;
            saving.set(false);
            match result {
                Ok(()) => st.time_dialog.set(None),
                Err(e) => {
                    confirm_delete.set(false);
                    error.set(Some(e));
                }
            }
        });
    };

    rsx! {
        div {
            class: "overlay",
            onclick: move |_| if !dirty { close() },
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    close();
                } else if e.key() == Key::Enter && (e.modifiers().ctrl() || e.modifiers().meta()) {
                    save_key();
                }
            },
            div {
                class: "dlg",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "time-title",
                onclick: move |e| e.stop_propagation(),
                div { class: "dlg-head",
                    div {
                        div { class: "eyebrow", "7pace Timetracker" }
                        h2 { id: "time-title", "{title}" }
                    }
                    button { class: "x", r#type: "button", aria_label: "Close", onclick: move |_| close(), Icon { svg: icons::CLOSE } }
                }
                if !form.pick_item {
                    div { class: "ctx", style: "--tc: #{type_color}",
                        span { class: "stripe" }
                        span { class: "t",
                            b { span { class: "id", "#{card_id} " } "{card_title}" }
                            if !context.is_empty() { small { "{context}" } }
                        }
                        if let Some(tot) = total {
                            span { class: "tot", "Tracked in total" b { "{tot}" } }
                        }
                    }
                }

                div { class: "tbody",
                    // In the padded body like the other fields (the item strip sits above it).
                    if form.pick_item {
                        div { class: "fld pick-item",
                            span { class: "lbl", "Work item" }
                            ParentPicker {
                                options: loggable_items(st),
                                value: has_card.then_some(card_id),
                                noun: "work items",
                                label: "Work item",
                                recent_label: "Logged this week",
                                foot: "Done items are hidden".to_owned(),
                                autofocus: !has_card,
                                onpick: move |id: Option<CardId>| set(&move |f| f.card = id.unwrap_or(0)),
                            }
                        }
                    }
                    div { class: "row2",
                        div { class: "fld",
                            div { class: "fld-top",
                                label { class: "lbl", r#for: "time-dur", "How long" }
                                if let Some((class, text)) = dur_hint { span { class, "{text}" } }
                            }
                            div { class: "durbox",
                                input {
                                    id: "time-dur",
                                    class: "in",
                                    r#type: "text",
                                    inputmode: "numeric",
                                    maxlength: "5",
                                    placeholder: "0:00",
                                    "aria-description": "Hours and minutes",
                                    value: "{form.duration_text}",
                                    onmounted: move |e| async move {
                                        // The item picker takes the focus while no item is picked.
                                        if has_card {
                                            let _ = e.set_focus(true).await;
                                        }
                                    },
                                    oninput: move |e| {
                                        let text = hmm_from_input(&e.value());
                                        set(&move |f| {
                                            if let Some(m) = parse_hmm(&text) {
                                                f.minutes = m;
                                            }
                                            f.duration_text = text.clone();
                                        });
                                    },
                                }
                            }
                            div { class: "qgrid", role: "group", aria_label: "Add or remove time",
                                for d in STEPS {
                                    {
                                        let label = format!("{}{}", if d > 0 { "+" } else { "−" }, format_duration(d.abs() * 60));
                                        let next = minutes + d;
                                        rsx! {
                                            button {
                                                key: "{d}",
                                                class: if d > 0 { "qchip" } else { "qchip minus" },
                                                r#type: "button",
                                                aria_label: if d > 0 { "Add {format_duration(d * 60)}" } else { "Remove {format_duration(-d * 60)}" },
                                                disabled: !(0..=MAX_ENTRY_MINUTES).contains(&next),
                                                onclick: move |_| set(&move |f| {
                                                    f.minutes = (f.minutes + d).clamp(0, MAX_ENTRY_MINUTES);
                                                    f.duration_text = format_hmm(f.minutes);
                                                }),
                                                "{label}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "fld",
                            span { class: "lbl", id: "time-timing", "Timing" }
                            div { class: "timing", role: "radiogroup", aria_labelledby: "time-timing",
                                label { class: if form.ends_now { "trow on" } else { "trow" },
                                    input {
                                        r#type: "radio",
                                        name: "timing",
                                        checked: form.ends_now,
                                        onchange: move |_| set(&|f| f.ends_now = true),
                                    }
                                    "Ends now"
                                    if form.ends_now && has_time {
                                        if let Some((a, b)) = range.clone() { span { class: "muted", "{a}–{b}" } }
                                    }
                                }
                                label { class: if form.ends_now { "trow" } else { "trow on" },
                                    input {
                                        r#type: "radio",
                                        name: "timing",
                                        checked: !form.ends_now,
                                        onchange: move |_| set(&|f| f.ends_now = false),
                                    }
                                    "Starts at"
                                    // Text, not `type=time`: that follows the WebKit locale
                                    // and can show AM/PM. Times are always 24-hour.
                                    input {
                                        class: if !form.ends_now && start_time.is_none() { "tm bad" } else { "tm" },
                                        r#type: "text",
                                        inputmode: "numeric",
                                        maxlength: "5",
                                        placeholder: "HH:MM",
                                        aria_label: "Start time, 24-hour",
                                        aria_invalid: "{!form.ends_now && start_time.is_none()}",
                                        value: "{form.start}",
                                        onfocus: move |_| set(&|f| f.ends_now = false),
                                        oninput: move |e| {
                                            let v = clock_input(&e.value());
                                            set(&move |f| {
                                                f.start = v.clone();
                                                f.ends_now = false;
                                            });
                                        },
                                        onblur: move |_| set(&|f| {
                                            if let Some(t) = parse_time_of_day(&f.start) {
                                                f.start = format_time_of_day(t);
                                            }
                                        }),
                                    }
                                }
                            }
                            if has_time {
                                if let Some((a, b)) = range.clone() { span { class: "hint", "Logs {a} → {b}, local time" } }
                            }
                        }
                    }

                    div { class: "fld",
                        span { class: "lbl", id: "time-date-l", "Date" }
                        div { class: "datefld anchor",
                            button {
                                class: "step",
                                r#type: "button",
                                aria_label: "Previous day",
                                onclick: move |_| set(&|f| f.date = f.date.previous_day().unwrap_or(f.date)),
                                Icon { svg: icons::BACK }
                            }
                            button {
                                class: if cal_open() { "datebtn open" } else { "datebtn" },
                                r#type: "button",
                                aria_haspopup: "dialog",
                                aria_expanded: "{cal_open()}",
                                aria_labelledby: "time-date-l",
                                onclick: move |_| cal_open.toggle(),
                                Icon { svg: icons::CALENDAR_16 }
                                "{long_date(form.date)}"
                                span { class: "rel", "{relative_day(form.date)}" }
                            }
                            button {
                                class: "step",
                                r#type: "button",
                                aria_label: "Next day",
                                disabled: form.date >= today,
                                onclick: move |_| set(&move |f| f.date = f.date.next_day().filter(|d| *d <= today).unwrap_or(f.date)),
                                Icon { svg: icons::CHEVRON_RIGHT }
                            }
                            if cal_open() {
                                div { class: "popover-backdrop", onclick: move |_| cal_open.set(false) }
                                div {
                                    class: "menu",
                                    onkeydown: move |e| if e.key() == Key::Escape {
                                        e.stop_propagation();
                                        cal_open.set(false);
                                    },
                                    Calendar {
                                        value: Some(form.date),
                                        max: Some(today),
                                        onpick: move |d| {
                                            set(&move |f| f.date = d);
                                            cal_open.set(false);
                                        },
                                    }
                                }
                            }
                        }
                    }

                    if !activities.is_empty() {
                        div { class: "fld",
                            span { class: "lbl", id: "time-act-l", "Activity" }
                            div { class: "anchor",
                                button {
                                    class: if act_open() { "selectlike open" } else { "selectlike" },
                                    r#type: "button",
                                    aria_haspopup: "listbox",
                                    aria_expanded: "{act_open()}",
                                    aria_labelledby: "time-act-l",
                                    onclick: move |_| {
                                        act_open.toggle();
                                        act_filter.set(String::new());
                                    },
                                    span { class: "act", style: "--c: {act_dot}" }
                                    span { class: "grow", "{act_name.clone().unwrap_or_default()}" }
                                    span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
                                }
                                if act_open() {
                                    div { class: "popover-backdrop", onclick: move |_| act_open.set(false) }
                                    div {
                                        class: "menu",
                                        onkeydown: move |e| if e.key() == Key::Escape {
                                            e.stop_propagation();
                                            act_open.set(false);
                                        },
                                        label { class: "srch",
                                            Icon { svg: icons::SEARCH }
                                            input {
                                                r#type: "text",
                                                placeholder: "Type to filter {activities.len()} activities",
                                                aria_label: "Filter activities",
                                                value: "{act_filter}",
                                                onmounted: move |e| async move { let _ = e.set_focus(true).await; },
                                                oninput: move |e| act_filter.set(e.value()),
                                            }
                                        }
                                        div { class: "scroll", role: "listbox", aria_labelledby: "time-act-l",
                                            for (name, items) in act_groups {
                                                div { key: "{name}", class: "grp", role: "presentation", "{name}" }
                                                for a in items {
                                                    {
                                                        let id = a.id.clone();
                                                        let on = activity.as_ref().is_some_and(|x| x.id == a.id);
                                                        rsx! {
                                                            button {
                                                                key: "{name}-{a.id}",
                                                                class: if on { "item on" } else { "item" },
                                                                r#type: "button",
                                                                role: "option",
                                                                aria_selected: "{on}",
                                                                onclick: move |_| {
                                                                    let id = id.clone();
                                                                    set(&move |f| f.activity = id.clone());
                                                                    act_open.set(false);
                                                                },
                                                                span { class: "act", style: "--c: {act_color(&a.color)}" }
                                                                span { class: "grow", "{a.name}" }
                                                                if a.id == default_activity { small { "default" } }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            if !filter.is_empty() && activities.iter().all(|a| !a.name.to_lowercase().contains(&filter)) {
                                                div { class: "empty", "No activity matches “{act_filter}”." }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div { class: "fld",
                        div { class: "fld-top",
                            label { class: "lbl", r#for: "time-comment", "Comment" }
                            span { class: "req", "Optional" }
                        }
                        textarea {
                            id: "time-comment",
                            class: "in",
                            rows: "2",
                            placeholder: "What did you work on?",
                            value: "{form.comment}",
                            oninput: move |e| {
                                let v = e.value();
                                set(&move |f| f.comment = v.clone());
                            },
                        }
                    }

                    if has_card {
                    div { class: "fld",
                        div { class: "fld-top",
                            span { class: "lbl", "Your time on this item" }
                            if has_time && mine_after != mine_before {
                                span { class: "hint", "{format_duration(mine_before)} → " b { "{format_duration(mine_after)}" } }
                            } else {
                                span { class: "hint", "{format_duration(mine_before)}" }
                            }
                        }
                        div { class: "log",
                            if has_time && !editing {
                                div { class: "log-row new",
                                    span { class: "act", style: "--c: {act_dot}" }
                                    span { "{short_date(form.date)}" }
                                    span { class: "len", "{format_duration(minutes * 60)}" }
                                    span { class: "n", "{new_note}" }
                                }
                            }
                            for e in shown {
                                {
                                    let is_edited = form.editing.as_ref() == Some(&e.id);
                                    let (dot, day, len, note) = if is_edited {
                                        (
                                            act_dot.clone(),
                                            short_date(form.date),
                                            format_duration(minutes * 60),
                                            format!("Editing{act_suffix}"),
                                        )
                                    } else {
                                        (
                                            act_color(&e.activity.as_ref().and_then(|a| a.color.clone())),
                                            short_date(e.start.date()),
                                            format_duration(e.seconds),
                                            e.comment.clone().or_else(|| e.activity.as_ref().map(|a| a.name.clone())).unwrap_or_default(),
                                        )
                                    };
                                    rsx! {
                                        div { key: "{e.id}", class: if is_edited { "log-row new" } else { "log-row" },
                                            span { class: "act", style: "--c: {dot}" }
                                            span { "{day}" }
                                            span { class: "len", "{len}" }
                                            span { class: "n", "{note}" }
                                        }
                                    }
                                }
                            }
                            if entries.is_empty() && !(has_time && !editing) {
                                div { class: "none", if loaded { "You haven't logged time on this item yet." } else { "Loading…" } }
                            }
                        }
                    }
                    }

                    if let Some(e) = error() {
                        div { class: "banner err", style: "margin: 0", role: "alert",
                            span { class: "bi", Icon { svg: icons::ALERT } }
                            span { class: "grow", "{e}" }
                        }
                    }
                }

                div { class: "dlg-foot",
                    p { class: "summary",
                        if has_time {
                            if let Some((a, b)) = range.clone() {
                                "Logs "
                                b { "{format_duration(minutes * 60)}" }
                                if let Some(n) = act_name.clone() {
                                    " of "
                                    b { "{n}" }
                                }
                                " on "
                                b { "{day_label(form.date)}, {a}–{b}" }
                                "."
                            }
                        }
                    }
                    if editing && form.can_delete {
                        button {
                            class: if confirm_delete() { "btn danger left" } else { "btn left" },
                            r#type: "button",
                            disabled: saving(),
                            onclick: delete,
                            if confirm_delete() { "Really delete?" } else { "Delete" }
                        }
                    }
                    button { class: "btn", r#type: "button", onclick: move |_| close(), "Cancel" }
                    button {
                        class: "btn primary",
                        r#type: "button",
                        disabled: saving() || !has_time || !has_card || (form.from_timer && st.time.read().busy),
                        onclick: move |_| save_click(),
                        if saving() { span { class: "spinner" } }
                        if form.from_timer { "Stop and log" } else if editing { "Save" } else { "Add time" }
                        kbd { "Ctrl ↵" }
                    }
                }
            }
        }
    }
}
