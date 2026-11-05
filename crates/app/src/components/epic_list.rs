//! "List" tab: the full backlog tree in backlog order — epics, then each level below them down to
//! tasks — every row with children collapsible on its own, plus "Expand down to <level>" to set
//! every row at once. Levels come from the team's backlog configuration. Filters keep the
//! ancestors of matching items as dimmed context rows.
//! In backlog order (the default) rows can be dragged to reorder among siblings with the same
//! parent (never reparented), which goes through `workitemsorder`. Sorting by target date or
//! priority (click the Due or Pri header) reorders siblings locally and turns dragging off.
//! Rows share one column grid with a sticky header, so cells line up at every depth. Row style (tint, accent bar, type icon) follows
//! `appearance.list`.

use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;
use kanban_core::config::ListSort;
use kanban_core::filters::{self, ListRow};
use kanban_core::{Card, CardId, StateCategory, Workspace};

use crate::components::filters::{ItemFilters, ItemTab, update_view_prefs, view_prefs};
use crate::components::icons;
use crate::components::item_dialog::{open_edit, open_new, open_new_top_level};
use crate::components::widgets::{Assignee, DateLabel, Icon, Prio, StatePill, TypeIcon};
use crate::services;
use crate::state::{Drag, use_app};

/// Everything one row needs, computed while the workspace is borrowed.
#[derive(Clone, PartialEq)]
struct RowData {
    row: ListRow,
    card: Card,
    category: StateCategory,
    state_color: String,
    type_color: String,
    type_icon: Option<String>,
    /// (done, total) of the descendants, for rows above the task level.
    progress: Option<(usize, usize)>,
    /// Number of direct children (for the badge on a collapsed row).
    kids: usize,
    /// Types offered by the row's "Add …" links.
    add_types: Vec<String>,
    /// Ids sharing the row's parent, in order (drop targets).
    siblings: Vec<CardId>,
}

#[component]
pub fn EpicList(search_focus: Signal<Option<std::rc::Rc<MountedData>>>) -> Element {
    let st = use_app();
    let mut query = use_signal(String::new);
    let mut open = use_signal(HashSet::<CardId>::new);

    let ws_guard = st.workspace.read();
    let Some(ws) = ws_guard.as_ref() else {
        return rsx! {};
    };
    let hierarchy = ws.hierarchy();
    let root_type = ws
        .root_types()
        .first()
        .cloned()
        .unwrap_or_else(|| "Epic".into());
    // "+ <type>" buttons: the top level, then each kanban level's default type, top-down.
    let new_types: Vec<(String, bool)> = std::iter::once((root_type.clone(), true))
        .chain(ws.kanban_levels().iter().filter_map(|l| {
            l.default_type
                .clone()
                .or_else(|| l.work_item_types.first().cloned())
                .map(|t| (t, false))
        }))
        .collect();
    let q = query.read().trim().to_lowercase();
    let view = view_prefs(st, ws);
    let matches_text = |c: &Card| {
        q.is_empty() || c.title.to_lowercase().contains(&q) || c.id.to_string().contains(&q)
    };
    let rows = filters::list_rows(
        ws,
        &hierarchy,
        &view.list,
        view.iteration.as_deref(),
        matches_text,
    );
    let sort = view.list_sort;
    let rows = match sort {
        ListSort::Backlog => rows,
        // No date / no priority last; ties keep backlog order.
        ListSort::TargetDate => filters::sort_siblings(rows, |r| {
            let d = ws.cards.get(&r.id).and_then(|c| c.target_date);
            (d.is_none(), d)
        }),
        ListSort::Priority => filters::sort_siblings(rows, |r| {
            ws.cards
                .get(&r.id)
                .and_then(|c| c.priority)
                .filter(|p| *p > 0)
                .unwrap_or(u8::MAX)
        }),
    };
    let (project, team) = (ws.project.clone(), ws.team.clone());
    let levels = filters::list_levels(ws);
    let level_defaults = level_default_types(ws, &levels);
    let data = row_data(ws, &hierarchy, &rows, &level_defaults);
    let roots_shown = rows.iter().filter(|r| r.depth == 0).count();
    let roots_hidden = view
        .list
        .is_filtering()
        .then(|| hierarchy.roots.len().saturating_sub(roots_shown))
        .filter(|n| *n > 0);
    let default_area = default_area(ws);
    let style = st.settings.read().appearance.list.clone();
    drop(ws_guard);

    // Rows that have children, by level, for "Expand down to".
    let parents: Vec<(CardId, usize)> = rows
        .iter()
        .filter(|r| r.has_children)
        .map(|r| (r.id, r.rank))
        .collect();
    let level_buttons: Vec<(String, usize, bool, HashSet<CardId>)> = levels
        .iter()
        .enumerate()
        .map(|(rank, (name, _))| {
            let target: HashSet<CardId> = parents
                .iter()
                .filter(|(_, r)| *r < rank)
                .map(|(id, _)| *id)
                .collect();
            let current = open.read();
            let on = parents
                .iter()
                .all(|(id, _)| current.contains(id) == target.contains(id));
            let count = rows.iter().filter(|r| r.rank == rank).count();
            (name.clone(), count, on, target)
        })
        .collect();

    // Hide everything below a collapsed row.
    let visible: Vec<RowData> = {
        let current = open.read();
        let mut out = Vec::new();
        let mut collapsed_at: Option<u8> = None;
        for d in data {
            if collapsed_at.is_some_and(|depth| d.row.depth > depth) {
                continue;
            }
            collapsed_at =
                (d.row.has_children && !current.contains(&d.row.id)).then_some(d.row.depth);
            out.push(d);
        }
        out
    };
    let empty = visible.is_empty();
    // Clicking a sortable header sorts by it; clicking it again goes back to backlog order.
    let on_sort = move |by: ListSort| {
        let next = if sort == by { ListSort::Backlog } else { by };
        update_view_prefs(st, &project, &team, |p| p.list_sort = next);
    };

    rsx! {
        div { class: "toolbar",
            label { class: "search",
                Icon { svg: icons::SEARCH }
                input {
                    class: "field",
                    r#type: "text",
                    placeholder: "Search epics and work items",
                    aria_label: "Search epics and work items",
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                    onmounted: move |e| search_focus.set(Some(e.data())),
                }
                kbd { "/" }
            }
            ItemFilters { tab: ItemTab::List }
            div { class: "spacer" }
            span { class: "toolbar",
                span { class: "label", id: "expand-to", "Expand down to" }
                div { class: "seg", role: "group", aria_labelledby: "expand-to",
                    for (name, count, on, target) in level_buttons {
                        button {
                            key: "{name}",
                            class: if on { "on" } else { "" },
                            r#type: "button",
                            aria_pressed: "{on}",
                            onclick: move |_| open.set(target.clone()),
                            "{name}"
                            span { class: "count", style: "margin-left: 6px", "{count}" }
                        }
                    }
                }
            }
        }
        div { class: "addrow", role: "group", aria_label: "New work item",
            for (t, is_root) in new_types {
                button {
                    key: "{t}",
                    class: "link sm",
                    r#type: "button",
                    "data-tip": "New {t.to_lowercase()}",
                    onclick: {
                        let t = t.clone();
                        let area = default_area.clone();
                        move |_| {
                            if is_root {
                                open_new(st, true, t.clone(), None, area.clone());
                            } else {
                                open_new_top_level(st, t.clone(), area.clone());
                            }
                        }
                    },
                    Icon { svg: icons::PLUS_14 }
                    "{t}"
                }
            }
        }

        section { class: "list lgrid", aria_label: "Work items", role: "table",
            div { class: "lhead", role: "row",
                span {}
                span { role: "columnheader", "ID" }
                SortHead { label: "Pri", this: ListSort::Priority, sort, on_sort: on_sort.clone() }
                span { role: "columnheader", if sort == ListSort::Backlog { "Title · backlog order" } else { "Title" } }
                span { role: "columnheader", "Status" }
                span { role: "columnheader", "Progress" }
                span { role: "columnheader", class: "c-area", "Area" }
                span { role: "columnheader", class: "c-sprint", "Sprint" }
                SortHead { label: "Due", this: ListSort::TargetDate, sort, on_sort: on_sort.clone() }
                span { role: "columnheader", class: "sr-only", "Assigned to" }
            }
            for (i, d) in visible.into_iter().enumerate() {
                ListRowView {
                    key: "{d.row.id}",
                    index: i,
                    open: open.read().contains(&d.row.id),
                    on_toggle: move |id: CardId| {
                        let mut o = open.write();
                        if !o.remove(&id) { o.insert(id); }
                    },
                    can_drag: sort == ListSort::Backlog,
                    tint: style.tint,
                    accent: style.accent.class(),
                    icon: style.icon,
                    d,
                }
            }
            if empty {
                p { class: "empty", "No epics match these filters." }
            }
            if let Some(n) = roots_hidden {
                div { class: "list-foot",
                    if n == 1 { "1 {root_type.to_lowercase()} hidden by the filters." } else { "{n} {root_type.to_lowercase()}s hidden by the filters." }
                }
            }
        }
    }
}

#[component]
fn ListRowView(
    d: RowData,
    index: usize,
    open: bool,
    on_toggle: EventHandler<CardId>,
    /// Backlog order only: another sort can't be written back to ADO's rank.
    can_drag: bool,
    tint: bool,
    accent: &'static str,
    icon: bool,
) -> Element {
    let mut st = use_app();
    let mut over = use_signal(|| false);
    let row = d.row;
    let id = row.id;
    let parent = row.parent;
    let card = d.card.clone();
    let root = row.depth == 0;
    let dragging = *st.drag.read() == Some(Drag::Row { id, parent });
    let droppable =
        matches!(*st.drag.read(), Some(Drag::Row { id: x, parent: p }) if p == parent && x != id);
    let pending = st.pending.read().contains(&id);
    let class = format!(
        "lrow {}{}{}{}{}{} {accent}",
        if root { "root" } else { "child" },
        if row.task { " task" } else { "" },
        if row.context { " context" } else { "" },
        if tint { " tint" } else { "" },
        if dragging { " dragging" } else { "" },
        if *over.read() && droppable {
            " drop-before"
        } else {
            ""
        },
    );
    let style = format!(
        "--tc: #{}; --i: {}; --d: {}",
        d.type_color,
        index.min(14),
        row.depth
    );
    let edit = card.clone();
    let siblings = d.siblings.clone();
    let open_state = !d.category.is_done();
    let title_class = if root { "etitle" } else { "etitle sm" };
    rsx! {
        div {
            class: "{class}",
            style: "{style}",
            role: "row",
            ondragover: move |e| {
                if matches!(*st.drag.peek(), Some(Drag::Row { parent: p, .. }) if p == parent) {
                    e.prevent_default();
                    if !*over.peek() { over.set(true); }
                }
            },
            ondragleave: move |_| over.set(false),
            ondrop: move |e| {
                e.prevent_default();
                over.set(false);
                if let Some(Drag::Row { id: dragged, parent: p }) = *st.drag.peek()
                    && p == parent
                    && dragged != id
                {
                    let rest: Vec<CardId> = siblings.iter().copied().filter(|&x| x != dragged).collect();
                    if let Some(pos) = rest.iter().position(|&x| x == id) {
                        let prev = pos.checked_sub(1).map(|i| rest[i]);
                        services::reorder_item(st, dragged, prev, Some(id));
                    }
                }
                st.drag.set(None);
            },
            // Every row fills every column, so cells line up at any depth.
            span { class: "c-grip",
                if !(row.task || row.context || !can_drag) {
                    span {
                        class: "grip",
                        draggable: "true",
                        "data-tip": "Drag to reorder",
                        ondragstart: move |e| {
                            let _ = e.data_transfer().set_data("text/plain", &id.to_string());
                            e.data_transfer().set_effect_allowed("move");
                            st.drag.set(Some(Drag::Row { id, parent }));
                        },
                        ondragend: move |_| {
                            st.drag.set(None);
                            over.set(false);
                        },
                        Icon { svg: icons::GRIP }
                    }
                }
            }
            span { class: "id", role: "cell", "{id}" }
            span { role: "cell", Prio { priority: card.priority } }
            span { class: "title-cell", role: "cell", style: "padding-left: {row.depth as usize * 24}px",
                if row.has_children {
                    button {
                        class: if open { "twirl sm open" } else { "twirl sm" },
                        r#type: "button",
                        aria_expanded: "{open}",
                        aria_label: if open { "Collapse {card.title}" } else { "Expand {card.title}" },
                        onclick: move |_| on_toggle.call(id),
                        Icon { svg: icons::CHEVRON_RIGHT }
                    }
                } else {
                    span { class: "twirl-space" }
                }
                if icon {
                    TypeIcon { icon: d.type_icon.clone(), color: d.type_color.clone(), label: card.work_item_type.clone() }
                } else if !root {
                    span { class: "type", "{card.work_item_type}" }
                }
                button {
                    class: "{title_class}",
                    r#type: "button",
                    aria_label: "Edit {card.title}",
                    onclick: move |_| open_edit(st, &edit, root),
                    "{card.title}"
                }
                if row.has_children && !open {
                    span { class: "kids", "data-tip": "Hidden below", "{d.kids}" }
                }
                for t in d.add_types.clone() {
                    button {
                        key: "{t}",
                        class: "link sm add-child",
                        r#type: "button",
                        "data-tip": "Add a {t.to_lowercase()}",
                        onclick: {
                            let t = t.clone();
                            let area = card.area_path.clone();
                            move |e: MouseEvent| {
                                e.stop_propagation();
                                open_new(st, false, t.clone(), Some(id), area.clone());
                            }
                        },
                        Icon { svg: icons::PLUS_14 }
                        "{t}"
                    }
                }
                if pending { span { class: "spinner", style: "color: var(--accent)" } }
            }
            span { role: "cell", StatePill { state: card.state.clone(), category: d.category, color: d.state_color.clone() } }
            span { role: "cell",
                if let Some((done, total)) = d.progress {
                    {
                        let pct = (done * 100).checked_div(total).unwrap_or(0);
                        rsx! {
                            div { class: "progress", "data-tip": "{done} of {total} work items done",
                                div { class: "bar-track", div { class: "bar-fill", style: "width: {pct}%" } }
                                span { class: "id", style: "width: 44px; text-align: right", "{done}/{total}" }
                            }
                        }
                    }
                }
            }
            span { class: "c-area area", role: "cell", "data-tip": "Area: {card.area_path}", "{card.area_leaf()}" }
            span { class: "c-sprint iter", role: "cell", "data-tip": "{card.iteration_path}", "{card.iteration_leaf()}" }
            span { role: "cell", DateLabel { date: card.target_date, open: open_state } }
            span { class: "c-own", role: "cell", Assignee { id, person: card.assigned_to.clone(), title: card.title.clone() } }
        }
    }
}

/// A column header that sorts the List by `this`; active shows an arrow.
#[component]
fn SortHead(
    label: &'static str,
    this: ListSort,
    sort: ListSort,
    on_sort: EventHandler<ListSort>,
) -> Element {
    let on = sort == this;
    rsx! {
        button {
            class: if on { "hsort on" } else { "hsort" },
            r#type: "button",
            role: "columnheader",
            aria_sort: if on { "ascending" } else { "none" },
            "data-tip": if on { "Back to backlog order" } else { "Sort by {label}" },
            onclick: move |_| on_sort.call(this),
            "{label}"
            if on { Icon { svg: icons::ARROW_UP } }
        }
    }
}

/// Default work item type per list level (what "Add …" creates at that level).
fn level_default_types(ws: &Workspace, levels: &[(String, Vec<String>)]) -> Vec<Option<String>> {
    levels
        .iter()
        .map(|(name, types)| {
            ws.levels
                .iter()
                .find(|l| &l.name == name)
                .and_then(|l| l.default_type.clone())
                .or_else(|| {
                    if ws.task_types == *types {
                        ws.task_type().map(str::to_owned)
                    } else {
                        types.first().cloned()
                    }
                })
        })
        .collect()
}

fn row_data(
    ws: &Workspace,
    hierarchy: &kanban_core::hierarchy::Hierarchy,
    rows: &[ListRow],
    level_defaults: &[Option<String>],
) -> Vec<RowData> {
    let task_level = level_defaults.len().saturating_sub(1);
    let mut by_parent: HashMap<Option<CardId>, Vec<CardId>> = HashMap::new();
    for r in rows {
        by_parent.entry(r.parent).or_default().push(r.id);
    }
    let mut kids: HashMap<CardId, usize> = HashMap::new();
    for r in rows {
        if let Some(p) = r.parent {
            *kids.entry(p).or_default() += 1;
        }
    }
    rows.iter()
        .filter_map(|r| {
            let card = ws.cards.get(&r.id)?;
            let add_types: Vec<String> = if r.task || r.context {
                Vec::new()
            } else if r.depth == 0 {
                // Roots may own any level below them (stories can hang directly off an epic).
                level_defaults
                    .get(1..task_level.max(1))
                    .unwrap_or_default()
                    .iter()
                    .flatten()
                    .cloned()
                    .collect()
            } else if r.rank + 1 == task_level {
                ws.owns_tasks(card)
                    .then(|| ws.task_type().map(str::to_owned))
                    .flatten()
                    .into_iter()
                    .collect()
            } else {
                level_defaults
                    .get(r.rank + 1)
                    .cloned()
                    .flatten()
                    .into_iter()
                    .collect()
            };
            let progress = (!r.task)
                .then(|| hierarchy.progress_of(r.id, &ws.cards, &ws.states))
                .filter(|(_, total)| *total > 0);
            Some(RowData {
                row: *r,
                card: card.clone(),
                category: ws.category(card),
                state_color: ws.state_color(card),
                type_color: ws.type_color(&card.work_item_type),
                type_icon: ws.type_icon(&card.work_item_type),
                progress,
                kids: kids.get(&r.id).copied().unwrap_or(0),
                add_types,
                siblings: by_parent.get(&r.parent).cloned().unwrap_or_default(),
            })
        })
        .collect()
}

/// The first team area, used for new items without a parent.
pub fn default_area(ws: &Workspace) -> String {
    ws.areas
        .first()
        .cloned()
        .unwrap_or_else(|| ws.project.clone())
}
