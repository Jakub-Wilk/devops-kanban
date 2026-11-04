//! 7pace time tracking UI: the per-card time section on the kanban (timer, add time, the user's
//! entries), the running-timer chip in the top bar, the add/edit time dialog, and `TimeKeeper`,
//! which connects at startup, ticks the clock while a timer runs and re-reads the timer on window
//! focus and every few minutes.

use std::time::Duration;

use dioxus::desktop::tao::event::Event;
use dioxus::desktop::{WindowEvent, use_wry_event_handler};
use dioxus::prelude::*;
use kanban_core::Card;
use kanban_core::time::{TimeEntry, format_clock, format_duration, parse_duration};

use crate::components::icons;
use crate::components::widgets::{Icon, short_date};
use crate::state::{AppState, TimeDialogState, use_app};
use crate::time_tracking::{self, TIMER_REFRESH, now_local};

/// Entries shown on a card before "Show all".
const SHOWN_ENTRIES: usize = 3;

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

/// Fetches the user's entries for the kanban's shown cards (only ones not loaded yet), and again
/// after Reload; re-reads the timer when the kanban opens. Renders nothing.
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

/// Opens the dialog to add time to `card`, or to edit `entry`.
pub fn open_time_dialog(mut st: AppState, card: &Card, entry: Option<&TimeEntry>) {
    let default_activity = st.time.peek().default_activity.clone().unwrap_or_default();
    let fmt_date = time::macros::format_description!("[year]-[month]-[day]");
    let fmt_time = time::macros::format_description!("[hour]:[minute]");
    let state = match entry {
        Some(e) => TimeDialogState {
            card: card.id,
            card_title: card.title.clone(),
            editing: Some(e.id.clone()),
            can_delete: e.can_delete,
            date: e.start.date().format(fmt_date).unwrap_or_default(),
            start: e.start.time().format(fmt_time).unwrap_or_default(),
            duration: format_duration(e.seconds),
            activity: e
                .activity
                .as_ref()
                .map(|a| a.id.clone())
                .unwrap_or(default_activity),
            comment: e.comment.clone().unwrap_or_default(),
        },
        None => TimeDialogState {
            card: card.id,
            card_title: card.title.clone(),
            editing: None,
            can_delete: false,
            date: now_local().date().format(fmt_date).unwrap_or_default(),
            start: String::new(),
            duration: String::new(),
            activity: default_activity,
            comment: String::new(),
        },
    };
    st.time_dialog.set(Some(state));
}

/// The time section of a kanban card: start/stop, add time, total, and the user's entries.
#[component]
pub fn CardTime(card: Card) -> Element {
    let st = use_app();
    let mut show_all = use_signal(|| false);
    if !time_tracking::connected(st) {
        return rsx! {};
    }
    let id = card.id;
    let t = st.time.read();
    let entries: Vec<TimeEntry> = t.entries.get(&id).cloned().unwrap_or_default();
    let running = t.timer.as_ref().filter(|x| x.work_item == id).cloned();
    let busy = t.busy;
    drop(t);
    // Only the tracked card reads the clock, so only it re-renders every second.
    let elapsed = running.map(|r| r.elapsed_secs(*st.clock.read()));
    let total = card
        .completed_work
        .filter(|h| *h > 0.0)
        .map(|h| format_duration((h * 3600.0).round() as i64));
    let n = entries.len();
    let shown: Vec<TimeEntry> = if show_all() {
        entries
    } else {
        entries.into_iter().take(SHOWN_ENTRIES).collect()
    };
    let add_card = card.clone();
    rsx! {
        div { class: "time",
            div { class: "time-bar",
                button {
                    class: if elapsed.is_some() { "tbtn on" } else { "tbtn" },
                    r#type: "button",
                    draggable: "false",
                    disabled: busy,
                    "data-tip": if elapsed.is_some() { "Stop tracking" } else { "Start tracking time on this item" },
                    aria_label: if elapsed.is_some() { "Stop tracking" } else { "Start tracking" },
                    onclick: move |e| {
                        e.stop_propagation();
                        if elapsed.is_some() { time_tracking::stop(st) } else { time_tracking::start(st, id) }
                    },
                    Icon { svg: if elapsed.is_some() { icons::STOP } else { icons::PLAY } }
                    if let Some(s) = elapsed { span { class: "clock", "{format_clock(s)}" } }
                }
                button {
                    class: "tbtn",
                    r#type: "button",
                    draggable: "false",
                    "data-tip": "Add time",
                    aria_label: "Add time",
                    onclick: move |e| {
                        e.stop_propagation();
                        open_time_dialog(st, &add_card, None);
                    },
                    Icon { svg: icons::CLOCK_PLUS }
                }
                div { class: "spacer" }
                if let Some(total) = total {
                    span { class: "total", "data-tip": "Tracked in total, by everyone", "{total}" }
                }
            }
            if !shown.is_empty() {
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
                open_time_dialog(st, &card, Some(&entry));
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
        div { class: "timer-chip", role: "status",
            span { class: "rec" }
            span { class: "tc-title", "data-tip": "Tracking #{timer.work_item} {title}", "#{timer.work_item} {title}" }
            span { class: "clock", "{format_clock(elapsed)}" }
            button {
                class: "tc-stop",
                r#type: "button",
                disabled: busy,
                "data-tip": "Stop tracking",
                aria_label: "Stop tracking",
                onclick: move |_| time_tracking::stop(st),
                Icon { svg: icons::STOP }
            }
        }
    }
}

#[component]
fn TimeDialog() -> Element {
    let mut st = use_app();
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut confirm_delete = use_signal(|| false);
    let Some(form) = st.time_dialog.read().clone() else {
        return rsx! {};
    };
    let t = st.time.read();
    let activities = if t.activities_enabled {
        t.activities.clone()
    } else {
        Vec::new()
    };
    drop(t);
    let parsed = parse_duration(&form.duration);
    let feedback = match (form.duration.trim().is_empty(), parsed) {
        (true, _) => "e.g. 1:30, 1h 30m, 90m or 1.5".to_owned(),
        (false, Some(s)) => format!("= {}", format_duration(s)),
        (false, None) => "Not a duration".to_owned(),
    };
    let editing = form.editing.is_some();
    let mut set = move |f: &dyn Fn(&mut TimeDialogState)| {
        if let Some(d) = st.time_dialog.write().as_mut() {
            f(d);
        }
    };
    let mut close = move || {
        if !*saving.peek() {
            st.time_dialog.set(None);
            error.set(None);
            confirm_delete.set(false);
        }
    };
    let submit = move |e: FormEvent| {
        e.prevent_default();
        let Some(f) = st.time_dialog.peek().clone() else {
            return;
        };
        saving.set(true);
        error.set(None);
        spawn(async move {
            let result = time_tracking::save_entry(st, f).await;
            saving.set(false);
            match result {
                Ok(()) => close(),
                Err(e) => error.set(Some(e)),
            }
        });
    };
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
                Ok(()) => close(),
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
            onclick: move |_| close(),
            onkeydown: move |e| if e.key() == Key::Escape { close() },
            form {
                class: "dialog",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "time-title",
                onclick: move |e| e.stop_propagation(),
                onsubmit: submit,
                div { class: "head",
                    div { style: "display: flex; flex-direction: column; gap: 2px; min-width: 0",
                        h2 { id: "time-title", if editing { "Edit time" } else { "Add time" } }
                        span { class: "sub", style: "overflow: hidden; text-overflow: ellipsis; white-space: nowrap", "#{form.card} {form.card_title}" }
                    }
                    button { class: "twirl", r#type: "button", aria_label: "Close", onclick: move |_| close(), Icon { svg: icons::CLOSE } }
                }
                div { class: "body",
                    label { class: "f",
                        span { class: "lbl", "Duration" }
                        input {
                            class: "field",
                            r#type: "text",
                            required: true,
                            placeholder: "1:30",
                            value: "{form.duration}",
                            onmounted: move |e| async move { let _ = e.set_focus(true).await; },
                            oninput: move |e| set(&|f| f.duration = e.value()),
                        }
                        span { class: if parsed.is_none() && !form.duration.trim().is_empty() { "hint bad" } else { "hint" }, "{feedback}" }
                    }
                    div { class: "grid2",
                        label { class: "f",
                            span { class: "lbl", "Date" }
                            input {
                                class: "field",
                                r#type: "date",
                                required: true,
                                value: "{form.date}",
                                oninput: move |e| set(&|f| f.date = e.value()),
                            }
                        }
                        label { class: "f",
                            span { class: "lbl", "Start" }
                            input {
                                class: "field",
                                r#type: "time",
                                value: "{form.start}",
                                oninput: move |e| set(&|f| f.start = e.value()),
                            }
                            span { class: "hint", "Empty: the entry ends now." }
                        }
                    }
                    if !activities.is_empty() {
                        label { class: "f",
                            span { class: "lbl", "Activity" }
                            span { class: "select-wrap",
                                select {
                                    class: "field",
                                    onchange: move |e| set(&|f| f.activity = e.value()),
                                    for a in activities {
                                        option { key: "{a.id}", value: "{a.id}", selected: form.activity == a.id, "{a.name}" }
                                    }
                                }
                                span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
                            }
                        }
                    }
                    label { class: "f",
                        span { class: "lbl", "Comment" }
                        textarea {
                            class: "field",
                            rows: "2",
                            value: "{form.comment}",
                            oninput: move |e| set(&|f| f.comment = e.value()),
                        }
                    }
                    if let Some(e) = error() {
                        div { class: "note err", Icon { svg: icons::ALERT } span { "{e}" } }
                    }
                }
                div { class: "foot",
                    if editing && form.can_delete {
                        button {
                            class: if confirm_delete() { "ghost danger" } else { "ghost" },
                            r#type: "button",
                            style: "margin-right: auto",
                            disabled: saving(),
                            onclick: delete,
                            if confirm_delete() { "Really delete?" } else { "Delete" }
                        }
                    }
                    button { class: "ghost", r#type: "button", onclick: move |_| close(), "Cancel" }
                    button {
                        class: "primary",
                        r#type: "submit",
                        disabled: saving() || parsed.is_none(),
                        if saving() { span { class: "spinner" } }
                        if editing { "Save" } else { "Add time" }
                    }
                }
            }
        }
    }
}
