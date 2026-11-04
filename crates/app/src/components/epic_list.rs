//! "List" tab: epics in backlog order, each expandable to its features and stories.
//! Rows can be dragged to reorder (siblings only), which goes through `workitemsorder`. Each row
//! has a stripe in its work item type's ADO color.

use std::collections::HashSet;

use dioxus::prelude::*;
use kanban_core::{Card, CardId, Workspace};

use crate::components::filters::{ItemFilters, ItemTab, view_prefs};
use crate::components::icons;
use crate::components::item_dialog::{open_edit, open_new};
use crate::components::widgets::{Assignee, DateLabel, Icon, Prio, StatePill};
use crate::services;
use crate::state::{Drag, use_app};

#[component]
pub fn EpicList(search_focus: Signal<Option<std::rc::Rc<MountedData>>>) -> Element {
    let mut st = use_app();
    let mut query = use_signal(String::new);
    let mut open = use_signal(HashSet::<CardId>::new);
    let mut over = use_signal(|| None::<CardId>);

    let ws_guard = st.workspace.read();
    let Some(ws) = ws_guard.as_ref() else {
        return rsx! {};
    };
    let hierarchy = ws.hierarchy();
    let root_types = ws.root_types();
    let child_types: Vec<String> = ws
        .kanban_levels()
        .iter()
        .filter_map(|l| {
            l.default_type
                .clone()
                .or_else(|| l.work_item_types.first().cloned())
        })
        .collect();
    let q = query.read().trim().to_lowercase();
    let view = view_prefs(st, ws);

    let matches_text = |c: &Card| {
        q.is_empty() || c.title.to_lowercase().contains(&q) || c.id.to_string().contains(&q)
    };
    let rows: Vec<(Card, Vec<(Card, u8)>)> = kanban_core::filters::epic_rows(
        ws,
        &hierarchy,
        &view.list,
        view.iteration.as_deref(),
        matches_text,
    )
    .into_iter()
    .map(|(e, kids)| {
        (
            e.clone(),
            kids.into_iter().map(|(c, d)| (c.clone(), d)).collect(),
        )
    })
    .collect();

    let visible_roots: Vec<CardId> = rows.iter().map(|(e, _)| e.id).collect();
    let (all_open, none_open) = {
        let o = open.read();
        (
            visible_roots.iter().all(|id| o.contains(id)),
            !visible_roots.iter().any(|id| o.contains(id)),
        )
    };
    let progress: Vec<(usize, usize)> = rows
        .iter()
        .map(|(e, _)| hierarchy.progress(e.id, &ws.cards, &ws.states))
        .collect();
    let categories: Vec<_> = rows
        .iter()
        .map(|(e, kids)| {
            (
                (
                    ws.category(e),
                    ws.state_color(e),
                    ws.type_color(&e.work_item_type),
                ),
                kids.iter()
                    .map(|(c, _)| {
                        (
                            ws.category(c),
                            ws.state_color(c),
                            ws.type_color(&c.work_item_type),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let root_type = root_types.first().cloned().unwrap_or_else(|| "Epic".into());
    let default_area = default_area(ws);
    let pending = st.pending.read().clone();
    let dragging = *st.drag.read();
    drop(ws_guard);

    let drop_on = move |target: CardId, siblings: Vec<CardId>| {
        let Some(Drag::Row { id, .. }) = *st.drag.peek() else {
            return;
        };
        if id == target {
            return;
        }
        let rest: Vec<CardId> = siblings.into_iter().filter(|&x| x != id).collect();
        let Some(pos) = rest.iter().position(|&x| x == target) else {
            return;
        };
        let prev = pos.checked_sub(1).map(|i| rest[i]);
        services::reorder_item(st, id, prev, Some(target));
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
            div { class: "seg-icons", role: "group", aria_label: "Expand or collapse epics",
                button {
                    class: "ghost icon-only",
                    r#type: "button",
                    title: "Expand all",
                    aria_label: "Expand all",
                    disabled: all_open,
                    onclick: {
                        let ids = visible_roots.clone();
                        move |_| open.set(ids.iter().copied().collect())
                    },
                    Icon { svg: icons::PLUS }
                }
                button {
                    class: "ghost icon-only",
                    r#type: "button",
                    title: "Collapse all",
                    aria_label: "Collapse all",
                    disabled: none_open,
                    onclick: move |_| open.write().clear(),
                    Icon { svg: icons::MINUS }
                }
            }
            ItemFilters { tab: ItemTab::List }
            div { class: "spacer" }
            button {
                class: "primary",
                r#type: "button",
                onclick: {
                    let root_type = root_type.clone();
                    let area = default_area.clone();
                    move |_| open_new(st, true, root_type.clone(), None, area.clone())
                },
                Icon { svg: icons::PLUS }
                "New {root_type.to_lowercase()}"
            }
        }

        section { class: "list", aria_label: "Epics",
            for (i, ((epic, kids), ((done, total), ((cat, color, type_color), kid_cats)))) in rows.into_iter().zip(progress.into_iter().zip(categories)).enumerate() {
                {
                    let id = epic.id;
                    let expanded = open.read().contains(&id);
                    let pct = (done * 100).checked_div(total).unwrap_or(0);
                    let row_class = format!(
                        "erow{}{}",
                        if dragging == Some(Drag::Row { id, root: None }) { " dragging" } else { "" },
                        if *over.read() == Some(id) && dragging.is_some_and(|d| matches!(d, Drag::Row { root: None, id: x } if x != id)) { " drop-before" } else { "" },
                    );
                    let siblings = visible_roots.clone();
                    let kid_ids: Vec<CardId> = kids.iter().map(|(c, _)| c.id).collect();
                    let epic_area = epic.area_path.clone();
                    let epic_for_edit = epic.clone();
                    let open_state = !cat.is_done();
                    rsx! {
                        div {
                            key: "{id}",
                            class: "{row_class}",
                            style: "--i: {i}",
                            ondragover: move |e| {
                                if matches!(*st.drag.peek(), Some(Drag::Row { root: None, .. })) {
                                    e.prevent_default();
                                    if *over.peek() != Some(id) { over.set(Some(id)); }
                                }
                            },
                            ondrop: move |e| {
                                e.prevent_default();
                                drop_on(id, siblings.clone());
                                over.set(None);
                                st.drag.set(None);
                            },
                            div { class: "ehead", style: "--tc: #{type_color}",
                                span {
                                    class: "grip",
                                    draggable: "true",
                                    title: "Drag to reorder",
                                    ondragstart: move |e| {
                                        let _ = e.data_transfer().set_data("text/plain", &id.to_string());
                                        e.data_transfer().set_effect_allowed("move");
                                        st.drag.set(Some(Drag::Row { id, root: None }));
                                    },
                                    ondragend: move |_| {
                                        st.drag.set(None);
                                        over.set(None);
                                    },
                                    Icon { svg: icons::GRIP }
                                }
                                button {
                                    class: if expanded { "twirl open" } else { "twirl" },
                                    r#type: "button",
                                    aria_expanded: "{expanded}",
                                    aria_label: if expanded { "Collapse {epic.title}" } else { "Expand {epic.title}" },
                                    onclick: move |_| {
                                        let mut o = open.write();
                                        if !o.remove(&id) { o.insert(id); }
                                    },
                                    Icon { svg: icons::CHEVRON_RIGHT }
                                }
                                span { class: "id", style: "width: 56px", "{id}" }
                                Prio { priority: epic.priority }
                                span { style: "flex: 1 1 260px; min-width: 0",
                                    button {
                                        class: "etitle",
                                        r#type: "button",
                                        aria_label: "Edit {epic.title}",
                                        onclick: move |_| open_edit(st, &epic_for_edit, true),
                                        "{epic.title}"
                                    }
                                }
                                span { class: "area", title: "Area: {epic.area_path}",
                                    Icon { svg: icons::FOLDER }
                                    span { "{epic.area_leaf()}" }
                                }
                                StatePill { state: epic.state.clone(), category: cat, color }
                                div { class: "progress", title: "{done} of {total} work items done",
                                    div { class: "bar-track",
                                        div { class: "bar-fill", style: "width: {pct}%" }
                                    }
                                    span { class: "id", style: "width: 44px; text-align: right", "{done}/{total}" }
                                }
                                span { class: "iter", title: "{epic.iteration_path}", "{epic.iteration_leaf()}" }
                                span { style: "width: 104px", DateLabel { date: epic.target_date, open: open_state } }
                                Assignee { id, person: epic.assigned_to.clone(), title: epic.title.clone() }
                                if pending.contains(&id) { span { class: "spinner", style: "color: var(--accent)" } }
                            }
                            div { class: if expanded { "expand open" } else { "expand" },
                                div { class: "inner",
                                    div { class: "children",
                                        for ((child, depth), (ccat, ccolor, ctype)) in kids.into_iter().zip(kid_cats) {
                                            ChildRow {
                                                key: "{child.id}",
                                                card: child,
                                                depth,
                                                category: ccat,
                                                color: ccolor,
                                                type_color: ctype,
                                                root: id,
                                                siblings: kid_ids.clone(),
                                            }
                                        }
                                        div { class: "addrow",
                                            for t in child_types.clone() {
                                                button {
                                                    key: "{t}",
                                                    class: "link",
                                                    r#type: "button",
                                                    onclick: {
                                                        let t = t.clone();
                                                        let area = epic_area.clone();
                                                        move |_| open_new(st, false, t.clone(), Some(id), area.clone())
                                                    },
                                                    Icon { svg: icons::PLUS_14 }
                                                    "Add {t.to_lowercase()}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if visible_roots.is_empty() {
                p { class: "empty", "No epics match these filters." }
            }
        }
    }
}

#[component]
fn ChildRow(
    card: Card,
    depth: u8,
    category: kanban_core::StateCategory,
    color: String,
    /// The work item type's ADO color, for the stripe.
    type_color: String,
    root: CardId,
    siblings: Vec<CardId>,
) -> Element {
    let mut st = use_app();
    let mut over = use_signal(|| false);
    let id = card.id;
    let dragging = *st.drag.read()
        == Some(Drag::Row {
            id,
            root: Some(root),
        });
    let droppable =
        matches!(*st.drag.read(), Some(Drag::Row { root: Some(r), id: x }) if r == root && x != id);
    let class = format!(
        "crow depth-{}{}{}",
        depth.min(2),
        if dragging { " dragging" } else { "" },
        if *over.read() && droppable {
            " drop-before"
        } else {
            ""
        },
    );
    let pending = st.pending.read().contains(&id);
    let edit_card = card.clone();
    rsx! {
        div {
            class: "{class}",
            style: "--tc: #{type_color}",
            ondragover: move |e| {
                if matches!(*st.drag.peek(), Some(Drag::Row { root: Some(r), .. }) if r == root) {
                    e.prevent_default();
                    e.stop_propagation();
                    if !*over.peek() { over.set(true); }
                }
            },
            ondragleave: move |_| over.set(false),
            ondrop: move |e| {
                e.prevent_default();
                e.stop_propagation();
                over.set(false);
                if let Some(Drag::Row { id: dragged, root: Some(r) }) = *st.drag.peek()
                    && r == root
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
            span {
                class: "grip",
                draggable: "true",
                title: "Drag to reorder",
                ondragstart: move |e| {
                    let _ = e.data_transfer().set_data("text/plain", &id.to_string());
                    st.drag.set(Some(Drag::Row { id, root: Some(root) }));
                },
                ondragend: move |_| st.drag.set(None),
                Icon { svg: icons::GRIP }
            }
            span { class: "id", style: "width: 56px", "{id}" }
            Prio { priority: card.priority }
            span { class: "type", "{card.work_item_type}" }
            span { style: "flex: 1 1 240px; min-width: 0",
                button {
                    class: "etitle sm",
                    r#type: "button",
                    aria_label: "Edit {card.title}",
                    onclick: move |_| open_edit(st, &edit_card, false),
                    "{card.title}"
                }
            }
            span { style: "width: 104px", DateLabel { date: card.target_date, open: !category.is_done() } }
            StatePill { state: card.state.clone(), category, color }
            Assignee { id, person: card.assigned_to.clone(), title: card.title.clone() }
            if pending { span { class: "spinner", style: "color: var(--accent)" } }
        }
    }
}

/// The first team area, used for new items without a parent.
pub fn default_area(ws: &Workspace) -> String {
    ws.areas
        .first()
        .cloned()
        .unwrap_or_else(|| ws.project.clone())
}
