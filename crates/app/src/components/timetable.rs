//! The Timetable tab: one week of the user's own 7pace entries as a schedule (Monday to Sunday
//! across, hours down), with the running timer and a now line on today, and a summary by day,
//! work item and activity beside it.
//!
//! Click an entry for its details (edit in the time dialog, start the timer, delete). Drag across
//! empty space to log time (a small popover asks for the item and activity), drag an entry to move
//! it (to another day too), drag its top or bottom edge to resize it. Drags snap to 15 minutes and
//! stop at neighbouring entries (`kanban_core::timetable`), so they never create an overlap.
//! Entries that already overlap (logged elsewhere) sit side by side and the shared span is red,
//! for the user to fix by hand. Every time is local and 24-hour.
//!
//! Drags use pointer events and keep their state in a signal: the press position, then pointer
//! deltas. Rows map to time through `HOUR_PX`; columns to days through the grid's width.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use dioxus::prelude::*;
use kanban_core::CardId;
use kanban_core::time::{Activity, TimeEntry, format_duration, format_time_of_day};
use kanban_core::timetable::{self, DEFAULT_HOURS, Edge, Overlap};
use time::{Date, PrimitiveDateTime, Time};

use crate::components::icons;
use crate::components::item_fields::ParentPicker;
use crate::components::pickers::day_label;
use crate::components::time::{loggable_items, open_time_dialog, open_time_dialog_at};
use crate::components::widgets::Icon;
use crate::routes::Route;
use crate::state::{AppState, TimeDialogState, use_app};
use crate::time_tracking::{self, local_offset, now_local};

/// Height of an hour.
const HOUR_PX: f64 = 56.0;
/// The hour labels' column.
const GUTTER_PX: f64 = 60.0;
/// Day column widths, Monday first: the weekend is narrower.
const WEIGHTS: [f64; 7] = [1.0, 1.0, 1.0, 1.0, 1.0, 0.7, 0.7];
/// A press that moves less than this many pixels is a click.
const CLICK_SLOP: f64 = 4.0;
/// Entries up to this long are drawn on one line.
const SHORT_SECS: i64 = 45 * 60;
const DAY_SECS: i64 = 24 * 3600;

/// A drag in progress. `cx0`/`cy0` are the client coordinates of the press, `y0` its height in
/// the day column, `day0` the column pressed.
#[derive(Debug, Clone, PartialEq)]
enum Drag {
    /// Across empty space: a new entry.
    Create {
        day0: usize,
        y0: f64,
        cy0: f64,
        dy: f64,
    },
    /// An entry's body: move it. `grab` is the pointer's offset from the entry's start.
    Move {
        entry: TimeEntry,
        day0: usize,
        grab: i64,
        y0: f64,
        cx0: f64,
        cy0: f64,
        dx: f64,
        dy: f64,
    },
    /// An entry's top or bottom edge.
    Resize {
        entry: TimeEntry,
        edge: Edge,
        day0: usize,
        y0: f64,
        cy0: f64,
        dy: f64,
    },
}

impl Drag {
    fn moved(&self) -> bool {
        match self {
            Self::Create { dy, .. } | Self::Resize { dy, .. } => dy.abs() >= CLICK_SLOP,
            Self::Move { dx, dy, .. } => dx.abs().max(dy.abs()) >= CLICK_SLOP,
        }
    }
}

/// Seconds since midnight at `y` pixels down a day column whose first hour is `lo`.
fn secs_at(y: f64, lo: u8) -> i64 {
    (i64::from(lo) * 3600 + (y / HOUR_PX * 3600.0).round() as i64).clamp(0, DAY_SECS - 60)
}

fn time_of(secs: i64) -> Time {
    let s = secs.clamp(0, DAY_SECS - 1);
    Time::from_hms((s / 3600) as u8, (s / 60 % 60) as u8, 0).unwrap_or(Time::MIDNIGHT)
}

/// Pixels from the top of a day column for `secs` since midnight.
fn px(secs: i64, lo: u8) -> f64 {
    (secs - i64::from(lo) * 3600) as f64 * HOUR_PX / 3600.0
}

/// The day column at `x` pixels into the grid body (gutter included) `width` wide.
fn col_at(x: f64, width: f64) -> usize {
    let total: f64 = WEIGHTS.iter().sum();
    let unit = (width - GUTTER_PX).max(1.0) / total;
    let mut left = GUTTER_PX;
    for (i, w) in WEIGHTS.iter().enumerate() {
        left += w * unit;
        if x < left {
            return i;
        }
    }
    6
}

fn col_center(i: usize, width: f64) -> f64 {
    let total: f64 = WEIGHTS.iter().sum();
    let unit = (width - GUTTER_PX).max(1.0) / total;
    GUTTER_PX + WEIGHTS[..i].iter().sum::<f64>() * unit + WEIGHTS[i] * unit / 2.0
}

fn secs_into(day: Date, t: PrimitiveDateTime) -> i64 {
    (t - day.midnight()).whole_seconds()
}

/// `9–15 November 2026`, `30 November – 6 December 2026`.
fn week_title(monday: Date) -> String {
    let sunday = monday + time::Duration::days(6);
    match (
        monday.year() == sunday.year(),
        monday.month() == sunday.month(),
    ) {
        (true, true) => format!(
            "{}–{} {} {}",
            monday.day(),
            sunday.day(),
            sunday.month(),
            sunday.year()
        ),
        (true, false) => format!(
            "{} {} – {} {} {}",
            monday.day(),
            monday.month(),
            sunday.day(),
            sunday.month(),
            sunday.year()
        ),
        _ => format!(
            "{} {} {} – {} {} {}",
            monday.day(),
            monday.month(),
            monday.year(),
            sunday.day(),
            sunday.month(),
            sunday.year()
        ),
    }
}

fn hm(t: PrimitiveDateTime) -> String {
    format_time_of_day(t.time())
}

fn activity_color(a: Option<&Activity>) -> String {
    a.and_then(|a| a.color.clone())
        .map_or_else(|| "var(--accent)".to_owned(), |c| format!("#{c}"))
}

/// The item's title: the workspace's card, or what ADO said for items elsewhere.
fn title_of(st: AppState, id: Option<CardId>) -> String {
    let Some(id) = id else {
        return "No work item".into();
    };
    st.workspace
        .read()
        .as_ref()
        .and_then(|w| w.cards.get(&id).map(|c| c.title.clone()))
        .or_else(|| st.time.read().week.items.get(&id).map(|r| r.title.clone()))
        .unwrap_or_default()
}

#[component]
pub fn Timetable() -> Element {
    let st = use_app();
    let connected = time_tracking::connected(st);
    let mut monday = use_signal(|| timetable::week_start(now_local().date()));
    let mut selected = use_signal(|| None::<String>);
    let mut confirm_delete = use_signal(|| false);
    let mut pending = use_signal(|| None::<(PrimitiveDateTime, i64)>);
    let mut drag = use_signal(|| None::<Drag>);
    let mut width = use_signal(|| 1000.0_f64);
    // The now line moves without a running timer too.
    let mut now = use_signal(now_local);
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            now.set(now_local());
        }
    });

    let m = monday();
    // Writes and Reload refresh the shown week themselves, so coming back to the tab doesn't.
    use_effect(use_reactive!(|m, connected| {
        let have = {
            let w = &st.time.peek().week;
            w.loaded && w.monday == Some(m)
        };
        if connected && !have {
            time_tracking::load_week(st, m);
        }
    }));

    if !connected {
        let error = st.time.read().error.clone();
        return rsx! {
            div { class: "note",
                Icon { svg: icons::INFO }
                span { style: "flex: 1",
                    if let Some(e) = error { "{e} " }
                    "Connect 7pace in Settings to see your week here."
                }
                Link { class: "ghost", to: Route::Settings {}, "Settings" }
            }
        };
    }

    let days = timetable::week_days(m);
    let today = now_local().date();
    let this_week = timetable::week_start(today);
    let t = st.time.read();
    let loaded = t.week.loaded && t.week.monday == Some(m);
    let loading = t.week.loading;
    let entries: Vec<TimeEntry> = if t.week.monday == Some(m) {
        t.week.entries.clone()
    } else {
        Vec::new()
    };
    let timer = t.timer.clone();
    let busy = t.busy;
    drop(t);

    // The running timer as an entry-like span (not draggable).
    let clock = if timer.is_some() {
        *st.clock.read()
    } else {
        now()
    };
    let now_local_t = {
        let c = clock.to_offset(local_offset());
        PrimitiveDateTime::new(c.date(), c.time())
    };
    let running: Option<(CardId, PrimitiveDateTime, i64)> = timer.as_ref().map(|x| {
        let s = x.started.to_offset(local_offset());
        let start = PrimitiveDateTime::new(s.date(), s.time());
        (
            x.work_item,
            start,
            (now_local_t - start).whole_seconds().max(60),
        )
    });
    let mut with_running = entries.clone();
    if let Some((id, start, secs)) = running {
        with_running.push(TimeEntry {
            id: "__running".into(),
            work_item: Some(id),
            start,
            seconds: secs,
            comment: None,
            activity: None,
            can_edit: false,
            can_delete: false,
        });
    }
    let (lo, hi) = timetable::hour_range(&with_running, &days, DEFAULT_HOURS);
    let body_h = f64::from(hi - lo) * HOUR_PX;

    let overlaps: Vec<Overlap> = timetable::overlaps(&entries);
    let clashing: HashSet<String> = overlaps
        .iter()
        .flat_map(|o| [o.a.clone(), o.b.clone()])
        .collect();
    let by_id: HashMap<String, TimeEntry> =
        entries.iter().map(|e| (e.id.clone(), e.clone())).collect();

    // Totals per day (entries only; the running timer is shown separately).
    let day_secs: Vec<i64> = days
        .iter()
        .map(|d| {
            timetable::day_segments(&entries, *d)
                .iter()
                .map(|s| s.to - s.from)
                .sum()
        })
        .collect();
    let week_secs: i64 = day_secs.iter().sum();
    let running_today = running
        .filter(|(_, start, _)| days.contains(&start.date()))
        .map(|(_, _, secs)| secs);

    // Per item and per activity, largest first.
    let mut per_item: HashMap<Option<CardId>, i64> = HashMap::new();
    let mut per_act: HashMap<String, (Option<Activity>, i64)> = HashMap::new();
    for d in &days {
        for s in timetable::day_segments(&entries, *d) {
            let e = &by_id[&s.id];
            *per_item.entry(e.work_item).or_default() += s.to - s.from;
            let key = e
                .activity
                .as_ref()
                .map(|a| a.id.clone())
                .unwrap_or_default();
            per_act.entry(key).or_insert((e.activity.clone(), 0)).1 += s.to - s.from;
        }
    }
    let mut per_item: Vec<(Option<CardId>, i64)> = per_item.into_iter().collect();
    per_item.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let item_max = per_item.first().map_or(1, |x| x.1.max(1));
    let mut per_act: Vec<(Option<Activity>, i64)> = per_act.into_values().collect();
    per_act.sort_by_key(|a| std::cmp::Reverse(a.1));
    let day_max = day_secs.iter().copied().max().unwrap_or(0).max(8 * 3600);

    let mode = drag();
    let w = width();

    // Where the dragged entry, or the new one, would land: (day index, from, length).
    let ghost: Option<(usize, i64, i64, bool)> = match &mode {
        Some(
            d @ Drag::Move {
                entry,
                day0,
                grab,
                y0,
                dx,
                dy,
                ..
            },
        ) if d.moved() => {
            let target = col_at(col_center(*day0, w) + dx, w);
            let wanted =
                days[target].midnight() + time::Duration::seconds(secs_at(y0 + dy, lo) - grab);
            match timetable::clamp_move(entry, wanted, &entries) {
                Some(s) => days
                    .iter()
                    .position(|d| *d == s.date())
                    .map(|i| (i, secs_into(s.date(), s), entry.seconds, true)),
                None => None,
            }
        }
        Some(Drag::Resize {
            entry,
            edge,
            day0,
            y0,
            dy,
            ..
        }) => {
            let wanted = days[*day0].midnight() + time::Duration::seconds(secs_at(y0 + dy, lo));
            let (s, len) = timetable::clamp_resize(entry, *edge, wanted, &entries);
            days.iter()
                .position(|d| *d == s.date())
                .map(|i| (i, secs_into(s.date(), s), len, true))
        }
        Some(d @ Drag::Create { day0, y0, dy, .. }) if d.moved() => timetable::free_slot(
            days[*day0],
            time_of(secs_at(*y0, lo)),
            time_of(secs_at(y0 + dy, lo)),
            &entries,
        )
        .map(|(s, len)| (*day0, secs_into(days[*day0], s), len, false)),
        _ => None,
    };
    let dragged_id = match &mode {
        Some(d @ (Drag::Move { entry, .. } | Drag::Resize { entry, .. })) if d.moved() => {
            Some(entry.id.clone())
        }
        _ => None,
    };

    let finish = move || {
        let Some(d) = drag.take() else {
            return;
        };
        let moved = d.moved();
        let st = st;
        let entries = st.time.peek().week.entries.clone();
        let days = timetable::week_days(*monday.peek());
        match d {
            Drag::Create { day0, y0, dy, .. } => {
                pending.set(None);
                selected.set(None);
                if moved
                    && let Some(slot) = timetable::free_slot(
                        days[day0],
                        time_of(secs_at(y0, lo)),
                        time_of(secs_at(y0 + dy, lo)),
                        &entries,
                    )
                {
                    pending.set(Some(slot));
                }
            }
            Drag::Move {
                entry,
                day0,
                grab,
                y0,
                cx0: _,
                cy0: _,
                dx,
                dy,
            } => {
                if !moved {
                    confirm_delete.set(false);
                    selected.set(Some(entry.id.clone()));
                    pending.set(None);
                    return;
                }
                let target = col_at(col_center(day0, *width.peek()) + dx, *width.peek());
                let wanted =
                    days[target].midnight() + time::Duration::seconds(secs_at(y0 + dy, lo) - grab);
                if let Some(start) = timetable::clamp_move(&entry, wanted, &entries) {
                    let secs = entry.seconds;
                    time_tracking::reschedule(st, entry, start, secs);
                }
            }
            Drag::Resize {
                entry,
                edge,
                day0,
                y0,
                dy,
                ..
            } => {
                if !moved {
                    selected.set(Some(entry.id.clone()));
                    return;
                }
                let wanted = days[day0].midnight() + time::Duration::seconds(secs_at(y0 + dy, lo));
                let (start, secs) = timetable::clamp_resize(&entry, edge, wanted, &entries);
                time_tracking::reschedule(st, entry, start, secs);
            }
        }
    };
    let mut finish_up = finish;
    let mut delete_selected = move || {
        let Some(id) = selected.peek().clone() else {
            return;
        };
        let Some(entry) = st
            .time
            .peek()
            .week
            .entries
            .iter()
            .find(|e| e.id == id)
            .cloned()
        else {
            return;
        };
        if !entry.can_delete {
            return;
        }
        if !*confirm_delete.peek() {
            confirm_delete.set(true);
            return;
        }
        confirm_delete.set(false);
        selected.set(None);
        spawn(async move {
            if let Err(e) =
                time_tracking::delete_entry(st, entry.work_item.unwrap_or(0), entry.id).await
            {
                crate::services::toast(
                    st,
                    crate::state::ToastKind::Error,
                    format!("Couldn't delete the entry: {e}"),
                );
            }
        });
    };

    let log_start = {
        let day = if days.contains(&today) { today } else { m };
        PrimitiveDateTime::new(day, now_local_t.time())
    };
    let run_label = running_today.map(format_duration);
    let week_n = m.iso_week();
    let hours: Vec<u8> = (lo..=hi).collect();
    let now_top = days
        .contains(&now_local_t.date())
        .then(|| px(secs_into(now_local_t.date(), now_local_t), lo));
    let grid_cols = format!(
        "{GUTTER_PX}px {}",
        WEIGHTS
            .iter()
            .map(|w| format!("minmax(0, {w}fr)"))
            .collect::<Vec<_>>()
            .join(" ")
    );

    rsx! {
        div { class: "toolbar tt-tools",
            div { class: "tt-nav", role: "group", aria_label: "Week",
                button {
                    class: "ghost icon-only",
                    r#type: "button",
                    aria_label: "Previous week",
                    "data-tip": "Previous week",
                    onclick: move |_| {
                        selected.set(None);
                        pending.set(None);
                        monday.set(m - time::Duration::days(7));
                    },
                    Icon { svg: icons::BACK }
                }
                button {
                    class: "ghost",
                    r#type: "button",
                    disabled: m == this_week,
                    onclick: move |_| {
                        selected.set(None);
                        pending.set(None);
                        monday.set(this_week);
                    },
                    "This week"
                }
                button {
                    class: "ghost icon-only",
                    r#type: "button",
                    aria_label: "Next week",
                    "data-tip": "Next week",
                    onclick: move |_| {
                        selected.set(None);
                        pending.set(None);
                        monday.set(m + time::Duration::days(7));
                    },
                    Icon { svg: icons::CHEVRON_RIGHT }
                }
            }
            div { class: "tt-range",
                h1 { "{week_title(m)}" }
                small { "Week {week_n} · your 7pace entries · local time, 24-hour" }
            }
            div { class: "spacer" }
            div { class: "tt-sum",
                "Logged this week "
                b { "{format_duration(week_secs)}" }
                if let Some(r) = run_label.clone() { span { "+ {r} running" } }
            }
            button {
                class: "primary",
                r#type: "button",
                onclick: move |_| open_time_dialog_at(st, None, log_start, 0, None),
                Icon { svg: icons::PLUS }
                "Log time"
            }
        }

        div { class: "tt-wrap",
            section {
                class: if mode.is_some() { "tt-box dragging" } else { "tt-box" },
                tabindex: "0",
                aria_label: "Time entries, week of {week_title(m)}",
                onkeydown: move |e| match e.key() {
                    Key::Escape => {
                        drag.set(None);
                        pending.set(None);
                        selected.set(None);
                        confirm_delete.set(false);
                    }
                    Key::Delete => delete_selected(),
                    _ => {}
                },
                div { class: "tt-head", style: "grid-template-columns: {grid_cols}",
                    div { class: "tt-corner" }
                    for (i, d) in days.iter().copied().enumerate() {
                        {
                            let n_over = overlaps.iter().filter(|o| {
                                let from = secs_into(d, o.from).max(0);
                                let to = secs_into(d, o.to).min(DAY_SECS);
                                to > from
                            }).count();
                            let is_today = d == today;
                            let class = format!(
                                "tt-dh{}{}{}",
                                if is_today { " today" } else { "" },
                                if d > today { " future" } else { "" },
                                if n_over > 0 { " clash" } else { "" },
                            );
                            let total = day_secs[i];
                            rsx! {
                                div { key: "{d}", class: "{class}",
                                    div { class: "top",
                                        span { class: "dw", "{day_label(d).split(' ').next().unwrap_or_default()}" }
                                        span { class: "dn", "{d.day()}" }
                                        if is_today { span { class: "badge", "Today" } }
                                    }
                                    span { class: "tot",
                                        if total > 0 { "{format_duration(total)}" } else { "–" }
                                        if is_today && let Some(r) = run_label.clone() { em { " + {r}" } }
                                        if n_over > 0 {
                                            " · "
                                            span { class: "ov", if n_over == 1 { "1 overlap" } else { "{n_over} overlaps" } }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div {
                    class: "tt-body",
                    style: "grid-template-columns: {grid_cols}; height: {body_h}px",
                    onresize: move |e| {
                        if let Ok(size) = e.get_content_box_size() {
                            width.set(size.width);
                        }
                    },
                    onpointermove: move |e| {
                        let c = e.client_coordinates();
                        let next = match drag.peek().clone() {
                            Some(Drag::Create { day0, y0, cy0, .. }) => Some(Drag::Create { day0, y0, cy0, dy: c.y - cy0 }),
                            Some(Drag::Move { entry, day0, grab, y0, cx0, cy0, .. }) => {
                                Some(Drag::Move { entry, day0, grab, y0, cx0, cy0, dx: c.x - cx0, dy: c.y - cy0 })
                            }
                            Some(Drag::Resize { entry, edge, day0, y0, cy0, .. }) => {
                                Some(Drag::Resize { entry, edge, day0, y0, cy0, dy: c.y - cy0 })
                            }
                            None => None,
                        };
                        if next.is_some() && next != *drag.peek() {
                            drag.set(next);
                        }
                    },
                    onpointerup: move |_| finish_up(),
                    onpointerleave: move |_| drag.set(None),
                    div { class: "tt-gut", aria_hidden: "true",
                        for h in hours.iter().copied() {
                            span {
                                key: "{h}",
                                class: "tt-hl",
                                style: "top: {(f64::from(h - lo) * HOUR_PX).max(8.0)}px",
                                if h < 24 { "{h:02}:00" } else { "24:00" }
                            }
                        }
                    }
                    for (i, d) in days.iter().copied().enumerate() {
                        {
                            let segs = timetable::day_segments(&entries, d);
                            let lanes = timetable::lanes(&segs);
                            let bands: Vec<(f64, f64)> = overlaps.iter().filter_map(|o| {
                                let from = secs_into(d, o.from).max(0);
                                let to = secs_into(d, o.to).min(DAY_SECS);
                                (to > from).then(|| (px(from, lo), (to - from) as f64 * HOUR_PX / 3600.0))
                            }).collect();
                            let class = format!(
                                "tt-day{}{}{}",
                                if d == today { " today" } else { "" },
                                if d > today { " future" } else { "" },
                                if timetable::is_weekend(d) { " weekend" } else { "" },
                            );
                            let flip = i >= 4;
                            let run_here = running.filter(|(_, s, _)| s.date() == d || (s.date() < d && secs_into(d, *s + time::Duration::seconds(running.map_or(0, |r| r.2))) > 0));
                            let sel_here = selected().and_then(|id| segs.iter().find(|s| s.id == id).cloned());
                            let ghost_here = ghost.filter(|g| g.0 == i);
                            let pending_here = pending().filter(|(s, _)| s.date() == d);
                            let is_empty = segs.is_empty() && run_here.is_none() && ghost_here.is_none() && pending_here.is_none();
                            rsx! {
                                div {
                                    key: "{d}",
                                    class: "{class}",
                                    style: "background-size: 100% {HOUR_PX}px; background-position: 0 0",
                                    onpointerdown: move |e| {
                                        if e.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) {
                                            return;
                                        }
                                        e.prevent_default();
                                        let y0 = e.element_coordinates().y;
                                        let c = e.client_coordinates();
                                        drag.set(Some(Drag::Create { day0: i, y0, cy0: c.y, dy: 0.0 }));
                                    },
                                    if is_empty && loaded {
                                        span { class: "tt-empty", "No entries" }
                                    }
                                    for s in segs.iter().cloned() {
                                        {
                                            let e = by_id[&s.id].clone();
                                            let (lane, n) = lanes.get(&s.id).copied().unwrap_or((0, 1));
                                            let top = px(s.from, lo);
                                            let h = ((s.to - s.from) as f64 * HOUR_PX / 3600.0 - 2.0).max(18.0);
                                            let clash = clashing.contains(&s.id);
                                            let is_sel = selected().as_deref() == Some(s.id.as_str());
                                            let class = format!(
                                                "tt-ev{}{}{}{}{}",
                                                if e.seconds <= SHORT_SECS { " short" } else { "" },
                                                if clash { " clash" } else { "" },
                                                if is_sel { " sel" } else { "" },
                                                if !e.can_edit { " ro" } else { "" },
                                                if dragged_id.as_deref() == Some(s.id.as_str()) { " from" } else { "" },
                                            );
                                            let color = activity_color(e.activity.as_ref());
                                            let start_t = timetable::end_of(&e);
                                            let when = format!(
                                                "{}–{}{}",
                                                hm(e.start),
                                                hm(start_t),
                                                if e.seconds >= 3600 && !clash { format!(" · {}", format_duration(e.seconds)) } else { String::new() },
                                            );
                                            let title = title_of(st, e.work_item);
                                            let id_label = e.work_item.map(|x| format!("#{x}")).unwrap_or_default();
                                            let aria = format!(
                                                "{id_label} {title}, {} {}–{}{}{}",
                                                day_label(e.start.date()),
                                                hm(e.start),
                                                hm(start_t),
                                                e.activity.as_ref().map(|a| format!(", {}", a.name)).unwrap_or_default(),
                                                if clash { ". Overlaps another entry" } else { "" },
                                            );
                                            let style = format!(
                                                "top: {top}px; height: {h}px; left: calc((100% - 8px) * {lane} / {n} + 4px); width: calc((100% - 8px) / {n} - {}px); --ac: {color}",
                                                if n > 1 { 2 } else { 0 },
                                            );
                                            let entry_move = e.clone();
                                            let entry_top = e.clone();
                                            let entry_bottom = e.clone();
                                            let entry_key = e.clone();
                                            let grab_base = secs_into(d, e.start);
                                            let can_edit = e.can_edit;
                                            rsx! {
                                                button {
                                                    key: "{s.id}",
                                                    class: "{class}",
                                                    r#type: "button",
                                                    style: "{style}",
                                                    aria_label: "{aria}",
                                                    onpointerdown: move |ev| {
                                                        ev.stop_propagation();
                                                        if ev.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) {
                                                            return;
                                                        }
                                                        ev.prevent_default();
                                                        if !can_edit {
                                                            confirm_delete.set(false);
                                                            selected.set(Some(entry_move.id.clone()));
                                                            pending.set(None);
                                                            return;
                                                        }
                                                        let c = ev.client_coordinates();
                                                        let y0 = top + ev.element_coordinates().y;
                                                        drag.set(Some(Drag::Move {
                                                            entry: entry_move.clone(),
                                                            day0: i,
                                                            grab: secs_at(y0, lo) - grab_base,
                                                            y0,
                                                            cx0: c.x,
                                                            cy0: c.y,
                                                            dx: 0.0,
                                                            dy: 0.0,
                                                        }));
                                                    },
                                                    onkeydown: move |ev| if ev.key() == Key::Enter {
                                                        confirm_delete.set(false);
                                                        selected.set(Some(entry_key.id.clone()));
                                                    },
                                                    if can_edit {
                                                        span {
                                                            class: "rz a",
                                                            aria_hidden: "true",
                                                            onpointerdown: move |ev| {
                                                                ev.stop_propagation();
                                                                ev.prevent_default();
                                                                let c = ev.client_coordinates();
                                                                drag.set(Some(Drag::Resize { entry: entry_top.clone(), edge: Edge::Start, day0: i, y0: top, cy0: c.y, dy: 0.0 }));
                                                            },
                                                        }
                                                    }
                                                    span { class: "l1",
                                                        span { class: "id", "{id_label}" }
                                                        span { class: "t", "{title}" }
                                                    }
                                                    span { class: "when", "{when}" }
                                                    if clash {
                                                        span { class: "warn", Icon { svg: icons::ALERT_12 } "Overlaps" }
                                                    }
                                                    if let Some(c) = e.comment.clone() { span { class: "nt", "{c}" } }
                                                    if can_edit {
                                                        span {
                                                            class: "rz b",
                                                            aria_hidden: "true",
                                                            onpointerdown: move |ev| {
                                                                ev.stop_propagation();
                                                                ev.prevent_default();
                                                                let c = ev.client_coordinates();
                                                                drag.set(Some(Drag::Resize { entry: entry_bottom.clone(), edge: Edge::End, day0: i, y0: top + h, cy0: c.y, dy: 0.0 }));
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    for (j, (top, h)) in bands.iter().copied().enumerate() {
                                        span { key: "band-{j}", class: "tt-band", aria_hidden: "true", style: "top: {top}px; height: {h}px" }
                                    }
                                    if let Some((id, start, secs)) = run_here {
                                        {
                                            let from = secs_into(d, start).max(0);
                                            let to = (secs_into(d, start) + secs).min(DAY_SECS);
                                            rsx! {
                                                div {
                                                    class: "tt-ev run",
                                                    role: "img",
                                                    aria_label: "Timer running on #{id} since {hm(start)}",
                                                    style: "top: {px(from, lo)}px; height: {((to - from) as f64 * HOUR_PX / 3600.0 - 2.0).max(18.0)}px",
                                                    span { class: "l1", span { class: "rec" } span { class: "id", "#{id}" } span { class: "t", "Running" } }
                                                    span { class: "when", "{hm(start)}–now" }
                                                }
                                            }
                                        }
                                    }
                                    if d == now_local_t.date() && let Some(top) = now_top {
                                        span { class: "tt-now", style: "top: {top}px" }
                                    }
                                    if let Some((_, from, len, is_move)) = ghost_here {
                                        {
                                            let start = d.midnight() + time::Duration::seconds(from);
                                            let end = start + time::Duration::seconds(len);
                                            let label = format!("{} {}–{}", day_label(d).split(' ').next().unwrap_or_default(), hm(start), hm(end));
                                            let color = match &mode {
                                                Some(Drag::Move { entry, .. } | Drag::Resize { entry, .. }) => activity_color(entry.activity.as_ref()),
                                                _ => "var(--accent)".into(),
                                            };
                                            rsx! {
                                                if is_move {
                                                    div {
                                                        class: "tt-ev moving",
                                                        style: "top: {px(from, lo)}px; height: {(len as f64 * HOUR_PX / 3600.0 - 2.0).max(18.0)}px; --ac: {color}",
                                                        aria_hidden: "true",
                                                        span { class: "badge", "{label} · {format_duration(len)}" }
                                                    }
                                                } else {
                                                    div {
                                                        class: "tt-new",
                                                        style: "top: {px(from, lo)}px; height: {(len as f64 * HOUR_PX / 3600.0 - 2.0).max(18.0)}px",
                                                        aria_hidden: "true",
                                                        span { "{hm(start)}–{hm(end)}" }
                                                        span { "{format_duration(len)}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if let Some((start, secs)) = pending_here {
                                        div {
                                            class: "tt-new",
                                            style: "top: {px(secs_into(d, start), lo)}px; height: {(secs as f64 * HOUR_PX / 3600.0 - 2.0).max(18.0)}px",
                                            aria_hidden: "true",
                                            span { "{hm(start)}–{hm(start + time::Duration::seconds(secs))}" }
                                            span { "{format_duration(secs)}" }
                                        }
                                        QuickLog {
                                            start,
                                            secs,
                                            top: px(secs_into(d, start), lo).min(body_h - 380.0).max(0.0),
                                            flip,
                                            onclose: move |_| pending.set(None),
                                        }
                                    }
                                    if let Some(s) = sel_here {
                                        if let Some(e) = by_id.get(&s.id).cloned() {
                                            EntryDetails {
                                                entry: e.clone(),
                                                top: px(s.from, lo).min(body_h - 260.0).max(0.0),
                                                flip,
                                                overlaps: overlaps.iter().filter(|o| o.a == s.id || o.b == s.id).cloned().collect::<Vec<_>>(),
                                                others: by_id.clone(),
                                                confirm_delete: confirm_delete(),
                                                busy,
                                                onclose: move |_| { selected.set(None); confirm_delete.set(false); },
                                                ondelete: move |_| delete_selected(),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "tt-foot",
                    span { "Drag on empty space to log time" }
                    span { "Drag an entry to move it, its top or bottom edge to resize (15-minute steps; stops at the neighbouring entry)" }
                    span { "Red marks time logged twice: fix the overlap by hand" }
                    span { kbd { "Del" } " deletes the selected entry" }
                    if loading { span { class: "tt-loading", "Loading…" } }
                }
            }

            aside { class: "tt-side", aria_label: "Week summary",
                section { class: "tt-card",
                    h2 { "This week" }
                    div { class: "tt-big",
                        "{format_duration(week_secs)}"
                        if let Some(r) = run_label.clone() { small { "+ {r} running" } }
                    }
                    div { class: "tt-days", aria_hidden: "true",
                        for (i, secs) in day_secs.iter().copied().enumerate() {
                            i {
                                key: "{i}",
                                class: if secs == 0 { "z" } else { "" },
                                style: if secs > 0 { "height: {(secs as f64 / day_max as f64 * 56.0).max(3.0)}px" } else { "" },
                            }
                        }
                    }
                    div { class: "tt-dl", aria_hidden: "true",
                        for l in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
                            span { key: "{l.0}", "{l.1}" }
                        }
                    }
                }
                section { class: "tt-card",
                    h2 { "By work item" }
                    if per_item.is_empty() { span { class: "tt-none", "Nothing logged yet." } }
                    for (id, secs) in per_item.iter().cloned() {
                        div { key: "{id:?}", class: "tt-it",
                            span { class: "name selectable",
                                if let Some(x) = id { span { class: "id", "{x}" } }
                                "{title_of(st, id)}"
                            }
                            span { class: "len", "{format_duration(secs)}" }
                            span { class: "bar", i { style: "width: {secs * 100 / item_max}%" } }
                        }
                    }
                }
                if per_act.iter().any(|(a, _)| a.is_some()) {
                    section { class: "tt-card",
                        h2 { "By activity" }
                        for (a, secs) in per_act.iter().cloned() {
                            div {
                                key: "{a.as_ref().map(|a| a.id.clone()).unwrap_or_default()}",
                                class: "tt-act",
                                style: "--c: {activity_color(a.as_ref())}",
                                i {}
                                "{a.as_ref().map(|a| a.name.clone()).unwrap_or_else(|| \"No activity\".into())}"
                                span { class: "len", "{format_duration(secs)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The selected entry's details, beside it: item, time, activity, comment, what it overlaps,
/// and Edit / Start timer / Delete.
#[component]
fn EntryDetails(
    entry: TimeEntry,
    top: f64,
    flip: bool,
    overlaps: Vec<Overlap>,
    others: HashMap<String, TimeEntry>,
    confirm_delete: bool,
    busy: bool,
    onclose: EventHandler<()>,
    ondelete: EventHandler<()>,
) -> Element {
    let st = use_app();
    let title = title_of(st, entry.work_item);
    let end = timetable::end_of(&entry);
    let when = format!(
        "{} · {}–{} · {}",
        day_label(entry.start.date()),
        hm(entry.start),
        hm(end),
        format_duration(entry.seconds)
    );
    let warnings: Vec<String> = overlaps
        .iter()
        .map(|o| {
            let other = if o.a == entry.id { &o.b } else { &o.a };
            let item = others
                .get(other)
                .and_then(|e| e.work_item)
                .map_or_else(|| "another entry".to_owned(), |x| format!("#{x}"));
            format!(
                "Overlaps {item} from {} to {}. Shorten or move one of them.",
                hm(o.from),
                hm(o.to)
            )
        })
        .collect();
    let card = entry.work_item;
    let edit_entry = entry.clone();
    rsx! {
        div {
            class: if flip { "tt-pop flip" } else { "tt-pop" },
            style: "top: {top}px",
            role: "dialog",
            aria_label: "Time entry",
            onpointerdown: move |e| e.stop_propagation(),
            div { class: "hd",
                div { class: "selectable",
                    b {
                        if let Some(x) = card { span { class: "id", "#{x} " } }
                        "{title}"
                    }
                    small { "{when}" }
                }
                button { class: "x", r#type: "button", aria_label: "Close", onclick: move |_| onclose.call(()), Icon { svg: icons::CLOSE } }
            }
            for w in warnings {
                div { class: "warnline", role: "alert", Icon { svg: icons::ALERT } span { "{w}" } }
            }
            div { class: "kv selectable",
                if let Some(a) = entry.activity.clone() {
                    span { "Activity" }
                    span { span { class: "dot", style: "--c: {activity_color(Some(&a))}" } "{a.name}" }
                }
                if let Some(c) = entry.comment.clone() {
                    span { "Comment" }
                    span { "{c}" }
                }
                if !entry.can_edit {
                    span { "Locked" }
                    span { "7pace doesn't allow editing this entry (approved week?)" }
                }
            }
            div { class: "acts",
                if entry.can_edit && let Some(id) = card {
                    button {
                        class: "ghost",
                        r#type: "button",
                        onclick: move |_| {
                            open_time_dialog(st, id, Some(&edit_entry));
                            onclose.call(());
                        },
                        "Edit"
                    }
                }
                if let Some(id) = card {
                    button {
                        class: "ghost",
                        r#type: "button",
                        disabled: busy,
                        aria_label: "Start the timer on #{id}",
                        onclick: move |_| time_tracking::start(st, id),
                        Icon { svg: icons::PLAY }
                        "Start timer"
                    }
                }
                if entry.can_delete {
                    button {
                        class: if confirm_delete { "ghost del on" } else { "ghost del" },
                        r#type: "button",
                        onclick: move |_| ondelete.call(()),
                        if confirm_delete { "Really delete?" } else { "Delete" }
                    }
                }
            }
        }
    }
}

/// After a drag across empty space: which item and activity to log `secs` from `start` on.
#[component]
fn QuickLog(
    start: PrimitiveDateTime,
    secs: i64,
    top: f64,
    flip: bool,
    onclose: EventHandler<()>,
) -> Element {
    let st = use_app();
    let recent_card = st.time.peek().week.entries.last().and_then(|e| e.work_item);
    let mut card = use_signal(move || recent_card);
    let default_activity = st.time.peek().default_activity.clone().unwrap_or_default();
    let mut activity = use_signal(move || default_activity);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let t = st.time.read();
    let activities = if t.activities_enabled {
        t.activities.clone()
    } else {
        Vec::new()
    };
    drop(t);
    let end = start + time::Duration::seconds(secs);
    let minutes = (secs + 30) / 60;
    let log = move |_| {
        let Some(c) = card() else {
            error.set(Some("Pick the work item first.".into()));
            return;
        };
        saving.set(true);
        error.set(None);
        let form = TimeDialogState {
            card: c,
            pick_item: false,
            editing: None,
            can_delete: false,
            from_timer: false,
            timer_minutes: 0,
            date: start.date(),
            minutes,
            duration_text: kanban_core::time::format_hmm(minutes),
            ends_now: false,
            start: format_time_of_day(start.time()),
            activity: activity(),
            comment: String::new(),
        };
        spawn(async move {
            let result = time_tracking::save_entry(st, form).await;
            saving.set(false);
            match result {
                Ok(()) => onclose.call(()),
                Err(e) => error.set(Some(e)),
            }
        });
    };
    rsx! {
        div {
            class: if flip { "tt-pop flip" } else { "tt-pop" },
            style: "top: {top}px",
            role: "dialog",
            aria_label: "Log {format_duration(secs)}",
            onpointerdown: move |e| e.stop_propagation(),
            onkeydown: move |e| if e.key() == Key::Escape {
                e.stop_propagation();
                onclose.call(());
            },
            div { class: "hd",
                div {
                    b { "Log {format_duration(secs)}" }
                    small { "{day_label(start.date())} · {hm(start)}–{hm(end)}" }
                }
                button { class: "x", r#type: "button", aria_label: "Cancel", onclick: move |_| onclose.call(()), Icon { svg: icons::CLOSE } }
            }
            ParentPicker {
                options: loggable_items(st),
                value: card(),
                noun: "work items",
                label: "Work item",
                recent_label: "Logged this week",
                foot: "Done items are hidden".to_owned(),
                autofocus: card().is_none(),
                onpick: move |id| card.set(id),
            }
            if !activities.is_empty() {
                span { class: "select-wrap",
                    select {
                        class: "ghost",
                        aria_label: "Activity",
                        onchange: move |e| activity.set(e.value()),
                        for a in activities.iter() {
                            option { key: "{a.id}", value: "{a.id}", selected: a.id == activity(), "{a.name}" }
                        }
                    }
                    span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
                }
            }
            if let Some(e) = error() {
                div { class: "warnline", role: "alert", Icon { svg: icons::ALERT } span { "{e}" } }
            }
            div { class: "row",
                button {
                    class: "link",
                    r#type: "button",
                    onclick: move |_| {
                        open_time_dialog_at(st, card(), start, secs, Some(activity()));
                        onclose.call(());
                    },
                    "More options"
                }
                button {
                    class: "primary",
                    r#type: "button",
                    disabled: saving() || card().is_none(),
                    onclick: log,
                    if saving() { span { class: "spinner" } }
                    "Log {format_duration(secs)}"
                }
            }
        }
    }
}
