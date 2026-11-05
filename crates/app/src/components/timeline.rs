//! "Timeline" tab: work items (earliest target first, undated last, with its own filters) on the
//! left, each with its tasks below it (collapsed until expanded), and an endless week calendar
//! on the right. Drag the separator to resize the two, drag the
//! calendar background to pan (the wheel only scrolls rows), drag a diamond to change the item's
//! target date. "Today" brings today back into view.

use std::collections::HashSet;
use std::time::Duration;

use dioxus::prelude::*;
use kanban_core::timeline::{TimelineItem, TimelineLayout, WEEK_PX, format_long};
use kanban_core::{Card, CardId, StateCategory};

use crate::components::filters::{ItemFilters, ItemTab, view_prefs};
use crate::components::icons;
use crate::components::item_dialog::{open_edit, open_new};
use crate::components::widgets::{Assignee, Icon, Prio, StatePill, TypeIcon, today};
use crate::services;
use crate::state::use_app;

const DEFAULT_INFO_PX: f64 = 560.0;
const MIN_INFO_PX: f64 = 280.0;
const MIN_TRACK_PX: f64 = 240.0;
const SEP_PX: f64 = 6.0;
/// Below this list width the date column is hidden.
const NARROW_INFO_PX: f64 = 400.0;

/// What a pointer drag on the timeline is doing.
#[derive(Clone, Copy, PartialEq)]
enum PointerMode {
    /// Moving a target-date diamond.
    Marker {
        id: CardId,
        start_client_x: f64,
        start_x: f64,
        dx: f64,
    },
    /// Panning the calendar.
    Pan {
        start_client_x: f64,
        start_scroll: f64,
    },
    /// Moving the separator.
    Split {
        start_client_x: f64,
        start_width: f64,
    },
}

#[component]
pub fn Timeline() -> Element {
    let mut st = use_app();
    let mut mode = use_signal(|| None::<PointerMode>);
    let mut total_w = use_signal(|| 0.0_f64);
    let mut info_w = use_signal(|| {
        st.settings
            .peek()
            .timeline_info_width
            .map_or(DEFAULT_INFO_PX, f64::from)
    });
    // Axis x at the calendar's left edge; `None` = today at a third of the width.
    let mut scroll_x = use_signal(|| None::<f64>);
    // Parents whose tasks are shown.
    let mut expanded = use_signal(HashSet::<CardId>::new);

    let ws_guard = st.workspace.read();
    let Some(ws) = ws_guard.as_ref() else {
        return rsx! {};
    };
    let hierarchy = ws.hierarchy();
    let titles = services::root_titles(ws);
    let view = view_prefs(st, ws);
    // Filtered items, then the tasks of each (tasks follow their parent, unfiltered).
    let shown = kanban_core::filters::timeline_items(
        ws,
        &hierarchy,
        &view.timeline,
        view.iteration.as_deref(),
    );
    let tasks: Vec<&Card> = shown
        .iter()
        .flat_map(|c| hierarchy.tasks.get(&c.id).into_iter().flatten())
        .filter_map(|id| ws.cards.get(id))
        .collect();
    let items: Vec<(Card, StateCategory, String)> = shown
        .into_iter()
        .chain(tasks)
        .map(|c| (c.clone(), ws.category(c), ws.state_color(c)))
        .collect();
    let new_type = ws
        .kanban_levels()
        .last()
        .and_then(|l| l.default_type.clone())
        .unwrap_or_else(|| "User Story".into());
    let area = crate::components::epic_list::default_area(ws);
    // Per type: colour, ADO icon id and fetched icon, for the row style (View options).
    let type_look: std::collections::HashMap<String, (String, Option<String>, Option<String>)> = ws
        .type_colors
        .keys()
        .map(|t| {
            (
                t.clone(),
                (ws.type_color(t), ws.type_icon(t), ws.type_icon_src(t)),
            )
        })
        .collect();
    let style = st.settings.read().appearance.timeline.clone();
    drop(ws_guard);

    let today = today();
    let layout = TimelineLayout::build(
        today,
        &items
            .iter()
            .map(|(c, cat, _)| TimelineItem {
                id: c.id,
                target: c.target_date,
                open: !cat.is_done(),
                parent: c.parent,
            })
            .collect::<Vec<_>>(),
    );
    let by_id: std::collections::HashMap<CardId, (Card, StateCategory, String)> = items
        .into_iter()
        .map(|(c, cat, color)| (c.id, (c, cat, color)))
        .collect();

    let total = total_w();
    let max_info = (total - MIN_TRACK_PX - SEP_PX).max(MIN_INFO_PX);
    let info = info_w().clamp(MIN_INFO_PX, max_info);
    let track_w = (total - info - SEP_PX).max(0.0);
    let today_x = layout.today_x;
    let home = today_x - track_w / 3.0;
    let sx = scroll_x().unwrap_or(home);
    // Months clipped to the visible window keep their labels in view; weeks get a margin.
    let (months, _) = layout.spans(sx, sx + track_w);
    let (_, weeks) = layout.spans(sx - WEEK_PX, sx + track_w + WEEK_PX);
    let grid_x = -sx.rem_euclid(WEEK_PX);
    let today_px = today_x - sx;
    let today_side = if today_px < 0.0 {
        Some("left")
    } else if today_px > track_w {
        Some("right")
    } else {
        None
    };
    let current = *mode.read();
    let tl_class = format!(
        "tl{}{}",
        if info < NARROW_INFO_PX { " narrow" } else { "" },
        match current {
            Some(PointerMode::Pan { .. }) => " panning",
            Some(PointerMode::Split { .. }) => " splitting",
            Some(PointerMode::Marker { .. }) => " moving",
            None => "",
        }
    );

    // Eases the calendar to `target` over ~250ms.
    let pan_to = move |target: f64| {
        let from = scroll_x.peek().unwrap_or(home);
        spawn(async move {
            const STEPS: u32 = 15;
            for i in 1..=STEPS {
                let t = f64::from(i) / f64::from(STEPS);
                let eased = 1.0 - (1.0 - t).powi(3);
                scroll_x.set(Some(from + (target - from) * eased));
                tokio::time::sleep(Duration::from_millis(16)).await;
            }
        });
    };
    let start_pan = move |e: PointerEvent| {
        e.prevent_default();
        mode.set(Some(PointerMode::Pan {
            start_client_x: e.client_coordinates().x,
            start_scroll: scroll_x.peek().unwrap_or(home),
        }));
    };
    let mut save_width = move || {
        let w = info_w.peek().clamp(MIN_INFO_PX, max_info).round() as u32;
        if st.settings.peek().timeline_info_width != Some(w) {
            st.settings.write().timeline_info_width = Some(w);
            services::save_settings(st);
        }
    };
    let layout_for_up = layout.clone();

    rsx! {
        div { class: "toolbar", ItemFilters { tab: ItemTab::Timeline } }
        div { class: "legend",
            span { span { class: "sw-marker" } "Target date (drag to move)" }
            span { span { class: "sw-bar" } "Time remaining" }
            span { span { class: "sw-late" } "Overdue" }
            span { span { class: "sw-today" } "Today" }
            div { class: "spacer" }
            button {
                class: "primary",
                r#type: "button",
                onclick: move |_| open_new(st, false, new_type.clone(), None, area.clone()),
                Icon { svg: icons::PLUS }
                "New work item"
            }
        }
        section {
            class: "{tl_class}",
            tabindex: "0",
            aria_label: "Work items by target date",
            style: "--info: {info}px; --gx: {grid_x}px",
            onresize: move |e| {
                if let Ok(size) = e.get_content_box_size() {
                    total_w.set(size.width);
                }
            },
            onpointermove: move |e| {
                let cx = e.client_coordinates().x;
                let m = *mode.peek();
                match m {
                    Some(PointerMode::Marker { id, start_client_x, start_x, .. }) => {
                        mode.set(Some(PointerMode::Marker { id, start_client_x, start_x, dx: cx - start_client_x }));
                    }
                    Some(PointerMode::Pan { start_client_x, start_scroll }) => {
                        scroll_x.set(Some(start_scroll - (cx - start_client_x)));
                    }
                    Some(PointerMode::Split { start_client_x, start_width }) => {
                        info_w.set((start_width + cx - start_client_x).clamp(MIN_INFO_PX, max_info));
                    }
                    None => {}
                }
            },
            onpointerup: move |_| {
                match mode.take() {
                    Some(PointerMode::Marker { id, start_x, dx, .. }) => {
                        let new = layout_for_up.date_at(start_x + dx);
                        if new != layout_for_up.date_at(start_x) {
                            services::set_target_date(st, id, Some(new));
                        }
                    }
                    Some(PointerMode::Split { .. }) => save_width(),
                    _ => {}
                }
            },
            onpointerleave: move |_| {
                if let Some(PointerMode::Split { .. }) = mode.take() {
                    save_width();
                }
            },
            div { class: "tl-body",
                div {
                    class: "tl-split",
                    role: "separator",
                    aria_orientation: "vertical",
                    aria_label: "Resize the item list",
                    "data-tip": "Drag to resize",
                    onpointerdown: move |e| {
                        e.prevent_default();
                        mode.set(Some(PointerMode::Split { start_client_x: e.client_coordinates().x, start_width: info }));
                    },
                }
                div { class: "tl-head",
                    div { class: "tl-info", "Work item, earliest target first" }
                    div { class: "tl-gap" }
                    div { class: "tl-track", onpointerdown: start_pan,
                        for m in months.iter() {
                            span { key: "{m.label}", class: "tl-month", style: "left: {m.left - sx}px; width: {m.width}px", "{m.label}" }
                        }
                        for w in weeks.iter() {
                            span {
                                key: "{w.left}",
                                class: if w.current { "tl-week now" } else { "tl-week" },
                                style: "left: {w.left - sx}px; width: {WEEK_PX}px",
                                "{w.label}"
                            }
                        }
                        if today_side.is_none() {
                            span { class: "tl-today-cap", style: "left: {today_px}px", "data-tip": "Today" }
                        }
                        if let Some(side) = today_side {
                            button {
                                class: "ghost tl-today-btn {side}",
                                r#type: "button",
                                "data-tip": "Scroll back to today",
                                onpointerdown: move |e| e.stop_propagation(),
                                onclick: move |_| pan_to(home),
                                if side == "left" { span { class: "ico flip", dangerous_inner_html: icons::CHEVRON_RIGHT } }
                                "Today"
                                if side == "right" { Icon { svg: icons::CHEVRON_RIGHT } }
                            }
                        }
                    }
                }
                for (i, row) in layout.rows.iter().enumerate() {
                    {
                        if row.parent.is_some_and(|p| !expanded.read().contains(&p)) {
                            return rsx! {};
                        }
                        let Some((card, cat, color)) = by_id.get(&row.id).cloned() else { return rsx! {} };
                        let id = card.id;
                        let is_open = expanded.read().contains(&id);
                        let n_tasks = row.children;
                        let epic = hierarchy.root_of.get(&id).and_then(|r| titles.get(r).map(|t| format!("{r} {t}")));
                        let dx = match current {
                            Some(PointerMode::Marker { id: m, dx, .. }) if m == id => Some(dx),
                            _ => None,
                        };
                        let marker_x = row.marker_x.map(|x| x + dx.unwrap_or(0.0));
                        let marker_px = marker_x.map(|x| x - sx);
                        let drag_date = marker_x
                            .filter(|_| dx.is_some_and(|d| d.abs() > 2.0))
                            .map(|x| format_long(layout.date_at(x)));
                        let date_text = card.target_date.map(format_long).unwrap_or_else(|| "No target date".into());
                        let edit = card.clone();
                        let look = type_look
                            .get(&card.work_item_type)
                            .cloned()
                            .unwrap_or_else(|| ("6b7686".to_owned(), None, None));
                        let start_x = row.marker_x.unwrap_or(0.0);
                        let mk_class = format!(
                            "mk{}{}",
                            if !row.open { " fin" } else if row.overdue { " late" } else { "" },
                            if dx.is_some() { " drag" } else { "" },
                        );
                        // An off-screen marker shows as a chevron on that edge.
                        let off = marker_px.and_then(|x| {
                            if x < 0.0 { Some("left") } else if x > track_w { Some("right") } else { None }
                        });
                        let focus_x = row.marker_x.map(|x| x - track_w / 2.0);
                        let bar = row
                            .bar
                            .filter(|_| dx.is_none())
                            .map(|(l, w)| (l - sx, w))
                            .filter(|(l, w)| l + w > 0.0 && *l < track_w);
                        rsx! {
                            div {
                                key: "{id}",
                                class: format!(
                                    "tl-row{}{} {}",
                                    if row.parent.is_some() { " child" } else { "" },
                                    if style.tint { " tint" } else { "" },
                                    style.accent.class()
                                ),
                                style: "--i: {i}; --tc: #{look.0}",
                                div { class: "tl-info",
                                    if n_tasks > 0 {
                                        button {
                                            class: if is_open { "twirl sm open" } else { "twirl sm" },
                                            r#type: "button",
                                            aria_expanded: "{is_open}",
                                            aria_label: if is_open { "Hide the tasks of {card.title}" } else { "Show the tasks of {card.title}" },
                                            "data-tip": if n_tasks == 1 { "1 task".to_owned() } else { format!("{n_tasks} tasks") },
                                            onclick: move |_| {
                                                let mut e = expanded.write();
                                                if !e.remove(&id) { e.insert(id); }
                                            },
                                            Icon { svg: icons::CHEVRON_RIGHT }
                                            span { class: "count", "{n_tasks}" }
                                        }
                                    } else {
                                        span { class: "twirl-space" }
                                    }
                                    span { class: "id", style: "width: 56px", "{id}" }
                                    Prio { priority: card.priority }
                                    if style.icon {
                                        TypeIcon { icon: look.1.clone(), src: look.2.clone(), color: look.0.clone(), label: card.work_item_type.clone() }
                                    }
                                    div { style: "flex: 1 1 0; min-width: 0; display: flex; flex-direction: column",
                                        button {
                                            class: "etitle sm",
                                            r#type: "button",
                                            style: "min-height: 28px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap",
                                            aria_label: "Edit {card.title}",
                                            onclick: move |_| open_edit(st, &edit, false),
                                            "{card.title}"
                                        }
                                        if let Some(e) = epic {
                                            span { style: "color: var(--faint); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap", "{e}" }
                                        }
                                    }
                                    div { class: "tl-date",
                                        span { "{date_text}" }
                                        if row.overdue { span { style: "color: var(--warn); font-size: 12px; font-weight: 600", "Overdue" } }
                                    }
                                    StatePill { state: card.state.clone(), category: cat, color }
                                    Assignee { id, person: card.assigned_to.clone(), title: card.title.clone() }
                                }
                                div { class: "tl-gap" }
                                div { class: "tl-track", onpointerdown: start_pan,
                                    if today_side.is_none() {
                                        span { class: "tl-today", style: "left: {today_px}px" }
                                    }
                                    if let Some((left, width)) = bar {
                                        span { class: if row.overdue { "tl-bar late" } else { "tl-bar" }, style: "left: {left}px; width: {width}px" }
                                    }
                                    match (marker_px, off) {
                                        (Some(x), None) => rsx! {
                                            button {
                                                class: "{mk_class}",
                                                r#type: "button",
                                                style: "left: {x}px",
                                                "data-tip": "Target date: {date_text}. Drag to change.",
                                                aria_label: "Target date of {card.title}: {date_text}",
                                                onpointerdown: move |e| {
                                                    e.prevent_default();
                                                    e.stop_propagation();
                                                    mode.set(Some(PointerMode::Marker { id, start_client_x: e.client_coordinates().x, start_x, dx: 0.0 }));
                                                },
                                            }
                                            if let Some(label) = drag_date {
                                                span { class: "mk-tip", style: "left: {x}px", "{label}" }
                                            }
                                        },
                                        (Some(_), Some(side)) => rsx! {
                                            button {
                                                class: "tl-edge {side}",
                                                r#type: "button",
                                                "data-tip": "Target date: {date_text}. Click to scroll there.",
                                                aria_label: "Scroll to the target date of {card.title}: {date_text}",
                                                onpointerdown: move |e| e.stop_propagation(),
                                                onclick: move |_| if let Some(x) = focus_x { pan_to(x) },
                                                if side == "left" { span { class: "ico flip", dangerous_inner_html: icons::CHEVRON_RIGHT } }
                                                if side == "right" { Icon { svg: icons::CHEVRON_RIGHT } }
                                            }
                                        },
                                        (None, _) => rsx! {
                                            div { class: "tl-undated", span { "No target date" } }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
                if layout.rows.is_empty() {
                    p { class: "empty", "No work items match these filters." }
                }
            }
        }
    }
}
