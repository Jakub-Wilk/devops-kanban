//! "Timeline" tab: work items on a week grid by target date (earliest first, undated last).
//! Drag a diamond sideways to change the item's target date.

use dioxus::prelude::*;
use kanban_core::timeline::{TimelineItem, TimelineLayout, WEEK_PX, format_long};
use kanban_core::{Card, CardId, StateCategory};

use crate::components::epic_list::in_iteration;
use crate::components::icons;
use crate::components::item_dialog::{open_edit, open_new};
use crate::components::widgets::{Assignee, Icon, Prio, StatePill, today};
use crate::services;
use crate::state::use_app;

const INFO_PX: f64 = 560.0;

/// A marker being dragged.
#[derive(Clone, Copy, PartialEq)]
struct MarkerDrag {
    id: CardId,
    start_client_x: f64,
    start_x: f64,
    dx: f64,
}

#[component]
pub fn Timeline(iteration: Option<String>) -> Element {
    let st = use_app();
    let mut drag = use_signal(|| None::<MarkerDrag>);

    let ws_guard = st.workspace.read();
    let Some(ws) = ws_guard.as_ref() else {
        return rsx! {};
    };
    let roots = ws.root_types();
    let hierarchy = ws.hierarchy();
    let titles = services::root_titles(ws);
    let items: Vec<(Card, StateCategory)> = ws
        .cards
        .values()
        .filter(|c| !roots.contains(&c.work_item_type))
        .filter(|c| in_iteration(c, &iteration))
        .map(|c| (c.clone(), ws.category(c)))
        .collect();
    let new_type = ws
        .kanban_levels()
        .last()
        .and_then(|l| l.default_type.clone())
        .unwrap_or_else(|| "User Story".into());
    let area = crate::components::epic_list::default_area(ws);
    drop(ws_guard);

    let today = today();
    let layout = TimelineLayout::build(
        today,
        &items
            .iter()
            .map(|(c, cat)| TimelineItem {
                id: c.id,
                target: c.target_date,
                open: !cat.is_done(),
            })
            .collect::<Vec<_>>(),
    );
    let by_id: std::collections::HashMap<CardId, (Card, StateCategory)> =
        items.into_iter().map(|(c, cat)| (c.id, (c, cat))).collect();
    let track = layout.width;
    let total = INFO_PX + track;
    let today_x = layout.today_x;
    let layout_for_up = layout.clone();
    let current = *drag.read();

    rsx! {
        div { class: "legend",
            span { span { style: "width: 11px; height: 11px; border-radius: 2px; transform: rotate(45deg); background: var(--accent)" } "Target date (drag to move)" }
            span { span { style: "width: 28px; height: 10px; border-radius: 5px; border: 1px solid var(--accent); background: rgba(124,155,255,.33)" } "Time remaining" }
            span { span { style: "width: 28px; height: 10px; border-radius: 5px; border: 1px solid var(--warn); background: repeating-linear-gradient(135deg, var(--warn) 0 3px, var(--warn-deep) 3px 6px)" } "Overdue" }
            span { span { style: "width: 2px; height: 14px; background: var(--accent)" } "Today" }
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
            class: "tl",
            tabindex: "0",
            aria_label: "Work items by target date",
            onpointermove: move |e| {
                let cur = *drag.peek();
                if let Some(mut d) = cur {
                    d.dx = e.client_coordinates().x - d.start_client_x;
                    drag.set(Some(d));
                }
            },
            onpointerup: move |_| {
                if let Some(d) = drag.take() {
                    let new = layout_for_up.date_at(d.start_x + d.dx);
                    let old = layout_for_up.date_at(d.start_x);
                    if new != old {
                        services::set_target_date(st, d.id, Some(new));
                    }
                }
            },
            onpointerleave: move |_| drag.set(None),
            div { style: "width: {total}px",
                div { class: "tl-head", style: "width: {total}px",
                    div { class: "tl-info", "Work item, earliest target first" }
                    div { class: "tl-track", style: "width: {track}px; height: 56px",
                        for m in layout.months.iter() {
                            span { key: "{m.label}", class: "tl-month", style: "left: {m.left}px; width: {m.width}px", "{m.label}" }
                        }
                        for w in layout.weeks.iter() {
                            span {
                                key: "{w.left}",
                                class: if w.current { "tl-week now" } else { "tl-week" },
                                style: "left: {w.left}px; width: {WEEK_PX}px",
                                "{w.label}"
                            }
                        }
                        span { class: "tl-today-cap", style: "left: {today_x}px", title: "Today" }
                    }
                }
                for (i, row) in layout.rows.iter().enumerate() {
                    {
                        let Some((card, cat)) = by_id.get(&row.id).cloned() else { return rsx! {} };
                        let id = card.id;
                        let epic = hierarchy.root_of.get(&id).and_then(|r| titles.get(r).map(|t| format!("{r} {t}")));
                        let dragging = current.filter(|d| d.id == id);
                        let marker_x = row.marker_x.map(|x| x + dragging.map_or(0.0, |d| d.dx));
                        let drag_date = dragging.and_then(|_| marker_x.map(|x| format_long(layout.date_at(x)))).filter(|_| dragging.is_some_and(|d| d.dx.abs() > 2.0));
                        let date_text = card.target_date.map(format_long).unwrap_or_else(|| "No target date".into());
                        let edit = card.clone();
                        let start_x = row.marker_x.unwrap_or(0.0);
                        let mk_class = format!(
                            "mk{}{}{}",
                            if !row.open { " fin" } else if row.overdue { " late" } else { "" },
                            if dragging.is_some() { " drag" } else { "" },
                            "",
                        );
                        rsx! {
                            div { key: "{id}", class: "tl-row", style: "width: {total}px; --i: {i}",
                                div { class: "tl-info",
                                    span { class: "id", style: "width: 56px", "{id}" }
                                    Prio { priority: card.priority }
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
                                    div { style: "width: 96px; flex: none; display: flex; flex-direction: column; font-size: 13px; color: var(--text-2)",
                                        span { "{date_text}" }
                                        if row.overdue { span { style: "color: var(--warn); font-size: 12px; font-weight: 600", "Overdue" } }
                                    }
                                    StatePill { state: card.state.clone(), category: cat }
                                    Assignee { id, person: card.assigned_to.clone(), title: card.title.clone(), tip_left: true }
                                }
                                div { class: "tl-track", style: "width: {track}px; min-height: 60px",
                                    for w in layout.weeks.iter() {
                                        span { key: "{w.left}", class: "tl-grid", style: "left: {w.left}px" }
                                    }
                                    span { class: "tl-today", style: "left: {today_x}px" }
                                    if let (Some((left, width)), None) = (row.bar, dragging) {
                                        span { class: if row.overdue { "tl-bar late" } else { "tl-bar" }, style: "left: {left}px; width: {width}px" }
                                    }
                                    if let Some(x) = marker_x {
                                        button {
                                            class: "{mk_class}",
                                            r#type: "button",
                                            style: "left: {x}px",
                                            title: "Target date: {date_text}. Drag to change.",
                                            aria_label: "Target date of {card.title}: {date_text}",
                                            onpointerdown: move |e| {
                                                e.prevent_default();
                                                drag.set(Some(MarkerDrag { id, start_client_x: e.client_coordinates().x, start_x, dx: 0.0 }));
                                            },
                                        }
                                        if let Some(label) = drag_date {
                                            span { class: "mk-tip", style: "left: {x}px", "{label}" }
                                        }
                                    } else {
                                        div { class: "tl-undated", span { "No target date" } }
                                    }
                                }
                            }
                        }
                    }
                }
                if layout.rows.is_empty() {
                    p { class: "empty", "No work items to show." }
                }
            }
        }
    }
}
