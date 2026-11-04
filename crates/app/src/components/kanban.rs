//! "Kanban" tab: the team's real board for one backlog level (columns, WIP limits, swimlanes
//! and state mappings from ADO), with a Features/Stories switcher, a Columns menu to hide
//! columns, epic, assignee and area filters (all saved per board), each card's tasks as a
//! checklist, and drag-and-drop between and within
//! columns.

use std::collections::HashSet;

use dioxus::prelude::*;
use kanban_core::filters;
use kanban_core::{Board, Card, CardId, Workspace};

use crate::components::epic_list::default_area;
use crate::components::filters::{
    FilterMenu, FilterOption, area_options, assignee_options, view_prefs,
};
use crate::components::icons;
use crate::components::item_dialog::{open_edit, open_new};
use crate::components::tasks::{CardTasks, TaskItem, task_items};
use crate::components::widgets::{Assignee, DateLabel, Icon, Prio, StatePill};
use crate::routes::Route;
use crate::services;
use crate::state::{AppState, Drag, DropSlot, use_app};

#[component]
pub fn Kanban(board: String) -> Element {
    let st = use_app();
    let collapsed_lanes = use_signal(HashSet::<String>::new);

    let ws_guard = st.workspace.read();
    let Some(ws) = ws_guard.as_ref() else {
        return rsx! {};
    };
    let levels: Vec<(String, String)> = ws
        .kanban_levels()
        .iter()
        .map(|l| (l.name.clone(), l.default_type.clone().unwrap_or_default()))
        .collect();
    let (project, team) = (ws.project.clone(), ws.team.clone());
    let switcher = rsx! {
        div { class: "seg", role: "tablist", aria_label: "Board",
            for (name, _) in levels.clone() {
                button {
                    key: "{name}",
                    class: if name == board { "on" } else { "" },
                    r#type: "button",
                    role: "tab",
                    aria_selected: "{name == board}",
                    onclick: {
                        let (project, team, name) = (project.clone(), team.clone(), name.clone());
                        move |_| {
                            navigator().replace(Route::Board { project: project.clone(), team: team.clone(), board: name.clone() });
                        }
                    },
                    "{name}"
                }
            }
        }
    };
    let Some(b) = ws.boards.get(&board) else {
        let msg = ws
            .board_errors
            .get(&board)
            .cloned()
            .unwrap_or_else(|| format!("This team has no {board} board."));
        return rsx! {
            div { class: "toolbar", {switcher} }
            div { class: "note err", Icon { svg: icons::ALERT } "Board unavailable: {msg}" }
        };
    };

    let hierarchy = ws.hierarchy();
    let roots = services::root_titles(ws);
    let me = ws.me.clone();
    let prefs = st.settings.read().board_prefs(&project, &team, &board);
    let hidden = prefs.hidden_columns.clone();
    let default_type = levels
        .iter()
        .find(|(n, _)| *n == board)
        .map(|(_, t)| t.clone())
        .filter(|t| !t.is_empty())
        .or_else(|| b.work_item_types.first().cloned())
        .unwrap_or_default();

    // Filters: epic → iteration → assignees and areas (each menu counts before its own filter).
    let view = view_prefs(st, ws);
    // A saved epic that was closed or moved away no longer filters.
    let epic_sel = prefs.epic.filter(|e| hierarchy.roots.contains(e));
    let base = filters::kanban_cards(ws, &hierarchy, b, epic_sel, view.iteration.as_deref());
    let visible: HashSet<CardId> = base
        .iter()
        .filter(|c| filters::matches_board(c, &prefs, me.as_ref()))
        .map(|c| c.id)
        .collect();
    let in_areas: Vec<&Card> = base
        .iter()
        .copied()
        .filter(|c| filters::matches_areas(c, &prefs.areas))
        .collect();
    let assignee_opts = assignee_options(&in_areas, me.as_ref());
    let of_assignees: Vec<&Card> = base
        .iter()
        .copied()
        .filter(|c| filters::matches_assignees(c, &prefs.assignees, me.as_ref()))
        .collect();
    let area_opts = area_options(&of_assignees, &prefs.areas);
    let on_board: Vec<&Card> = ws.cards.values().filter(|c| b.shows(c)).collect();

    let mut epic_options: Vec<(CardId, String)> = hierarchy
        .roots
        .iter()
        .filter_map(|id| roots.get(id).map(|t| (*id, t.clone())))
        .collect();
    epic_options.retain(|(id, _)| {
        hierarchy
            .children
            .get(id)
            .is_some_and(|k| k.iter().any(|(c, _)| on_board.iter().any(|x| x.id == *c)))
    });

    let columns: Vec<kanban_core::Column> = b.visible_columns(&hidden).cloned().collect();
    let all_columns: Vec<String> = b.columns.iter().map(|c| c.name.clone()).collect();
    let column_opts: Vec<FilterOption> = all_columns
        .iter()
        .map(|c| FilterOption {
            value: c.clone(),
            label: c.clone(),
            count: None,
        })
        .collect();
    let shown_columns: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
    let lanes: Vec<(Option<String>, Option<String>, Option<String>)> = if b.has_lanes() {
        b.lanes
            .iter()
            .map(|l| (Some(l.id.clone()), l.name.clone(), l.color.clone()))
            .collect()
    } else {
        vec![(None, None, None)]
    };
    let n = columns.len().max(1);
    let grid_style = format!(
        "grid-template-columns: repeat({n}, minmax(236px, 1fr)); min-width: {}px",
        n * 248
    );
    let header_cols: Vec<ColumnHead> = columns
        .iter()
        .map(|c| column_head(ws, b, c, &visible))
        .collect();
    let board_clone = b.clone();
    let area = epic_sel
        .and_then(|e| ws.cards.get(&e))
        .map(|e| e.area_path.clone())
        .unwrap_or_else(|| default_area(ws));
    drop(ws_guard);

    let hidden_n = hidden.len();
    let filtering = epic_sel.is_some() || !prefs.assignees.is_empty() || !prefs.areas.is_empty();
    let save = {
        let (project, team, board) = (project.clone(), team.clone(), board.clone());
        move |f: Box<dyn FnOnce(&mut kanban_core::BoardPrefs)>| {
            save_board_prefs(st, &project, &team, &board, f)
        }
    };
    rsx! {
        div { class: "toolbar",
            {switcher}
            label { class: "toolbar",
                span { class: "label", "Epic" }
                span { class: "select-wrap",
                    select {
                        class: "field",
                        style: "min-width: 220px; max-width: 360px",
                        onchange: {
                            let save = save.clone();
                            move |e: FormEvent| {
                                let epic = e.value().parse().ok();
                                save(Box::new(move |p| p.epic = epic))
                            }
                        },
                        option { value: "", selected: epic_sel.is_none(), "All epics" }
                        for (id, title) in epic_options {
                            option { key: "{id}", value: "{id}", selected: epic_sel == Some(id), "{id}  {title}" }
                        }
                    }
                    span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
                }
            }
            FilterMenu {
                label: "Assigned to",
                options: assignee_opts,
                selected: prefs.assignees.clone(),
                on_change: {
                    let save = save.clone();
                    move |v: Vec<String>| save(Box::new(move |p| p.assignees = v))
                },
            }
            FilterMenu {
                label: "Area",
                options: area_opts,
                selected: prefs.areas.clone(),
                on_change: {
                    let save = save.clone();
                    move |v: Vec<String>| save(Box::new(move |p| p.areas = v))
                },
            }
            if filtering {
                button {
                    class: "link",
                    r#type: "button",
                    onclick: {
                        let save = save.clone();
                        move |_| save(Box::new(|p| {
                            p.epic = None;
                            p.assignees.clear();
                            p.areas.clear();
                        }))
                    },
                    Icon { svg: icons::CLOSE }
                    "Clear filters"
                }
            }
            div { class: "spacer" }
            FilterMenu {
                label: "Columns",
                icon: icons::COLUMNS,
                plain: true,
                badge: hidden_n,
                align_right: true,
                options: column_opts,
                selected: shown_columns,
                reset_label: "Show all",
                reset_to: all_columns.clone(),
                on_change: {
                    let (project, team, board) = (project.clone(), team.clone(), board.clone());
                    let all = all_columns.clone();
                    move |shown: Vec<String>| {
                        let hidden: Vec<String> = all.iter().filter(|c| !shown.contains(c)).cloned().collect();
                        save_board_prefs(st, &project, &team, &board, |p| p.hidden_columns = hidden);
                    }
                },
            }
            button {
                class: "primary",
                r#type: "button",
                onclick: {
                    let t = default_type.clone();
                    move |_| open_new(st, false, t.clone(), epic_sel, area.clone())
                },
                Icon { svg: icons::PLUS }
                "New {default_type.to_lowercase()}"
            }
        }

        if columns.is_empty() {
            div { class: "note", Icon { svg: icons::INFO } "All columns are hidden. Use the Columns menu to show some." }
        } else {
            div { class: "board-scroll",
                div { class: "board", style: "{grid_style}",
                    for (i, h) in header_cols.iter().enumerate() {
                        div { key: "h-{h.id}", class: "colhead", style: "--i: {i}",
                            StatePill { state: h.name.clone(), category: h.category, color: h.color.clone() }
                            span { class: "count", "{h.shown}" }
                            if h.limit > 0 {
                                span { class: if h.total > h.limit as usize { "wip over" } else { "wip" }, "data-tip": "WIP limit", "{h.total}/{h.limit}" }
                            }
                        }
                    }
                }
                for (lane_id, lane_name, lane_color) in lanes {
                    Lane {
                        key: "{lane_id:?}",
                        board: board_clone.clone(),
                        board_name: board.clone(),
                        columns: columns.clone(),
                        lane_id,
                        lane_name,
                        lane_color,
                        visible: visible.clone(),
                        roots: hierarchy.root_of.clone(),
                        root_titles: roots.clone(),
                        task_ids: hierarchy.tasks.clone(),
                        grid_style: grid_style.clone(),
                        collapsed: collapsed_lanes,
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct ColumnHead {
    id: String,
    name: String,
    category: kanban_core::StateCategory,
    color: String,
    shown: usize,
    total: usize,
    limit: u32,
}

fn column_head(
    ws: &Workspace,
    b: &Board,
    c: &kanban_core::Column,
    visible: &HashSet<CardId>,
) -> ColumnHead {
    let ids = b.placement.get(&c.id).cloned().unwrap_or_default();
    // The board's first type that this column maps (stable, unlike HashMap order).
    let mapping = b
        .work_item_types
        .iter()
        .find_map(|t| c.state_mappings.get(t).map(|s| (t, s)));
    let category = mapping
        .map(|(t, s)| ws.states.category(t, s))
        .unwrap_or(kanban_core::StateCategory::Unknown);
    let color = mapping
        .map(|(t, s)| ws.states.color(t, s))
        .unwrap_or_else(|| category.default_color().to_owned());
    ColumnHead {
        id: c.id.clone(),
        name: c.name.clone(),
        category,
        color,
        shown: ids.iter().filter(|id| visible.contains(id)).count(),
        total: ids.len(),
        limit: c.item_limit,
    }
}

#[component]
fn Lane(
    board: Board,
    board_name: String,
    columns: Vec<kanban_core::Column>,
    lane_id: Option<String>,
    lane_name: Option<String>,
    lane_color: Option<String>,
    visible: HashSet<CardId>,
    roots: std::collections::HashMap<CardId, CardId>,
    root_titles: std::collections::HashMap<CardId, String>,
    /// Parent → task ids ([`kanban_core::Hierarchy::tasks`]).
    task_ids: std::collections::HashMap<CardId, Vec<CardId>>,
    grid_style: String,
    collapsed: Signal<HashSet<String>>,
) -> Element {
    let st = use_app();
    let key = lane_id.clone().unwrap_or_default();
    let is_collapsed = collapsed.read().contains(&key);
    let ws_guard = st.workspace.read();
    let Some(ws) = ws_guard.as_ref() else {
        return rsx! {};
    };
    // Cards per column for this lane, in board order.
    let cells: Vec<(kanban_core::Column, Vec<Card>, usize)> = columns
        .iter()
        .map(|c| {
            let all = board.placement.get(&c.id).cloned().unwrap_or_default();
            let cards: Vec<Card> = all
                .iter()
                .filter(|id| visible.contains(id))
                .filter_map(|id| ws.cards.get(id))
                .filter(|card| lane_id.as_ref().is_none_or(|l| board.lane_of(card) == *l))
                .cloned()
                .collect();
            (c.clone(), cards, all.len())
        })
        .collect();
    let categories: std::collections::HashMap<CardId, (kanban_core::StateCategory, String)> = cells
        .iter()
        .flat_map(|(_, cards, _)| {
            cards
                .iter()
                .map(|c| (c.id, (ws.category(c), ws.state_color(c))))
        })
        .collect();
    // Each card's tasks, and the task type when it can own tasks (for "Add task").
    let tasks: std::collections::HashMap<CardId, (Vec<TaskItem>, Option<String>)> = cells
        .iter()
        .flat_map(|(_, cards, _)| cards.iter())
        .map(|c| {
            let add = ws
                .owns_tasks(c)
                .then(|| ws.task_type().map(str::to_owned))
                .flatten();
            (c.id, (task_items(ws, task_ids.get(&c.id)), add))
        })
        .collect();
    drop(ws_guard);
    let lane_total: usize = cells.iter().map(|(_, c, _)| c.len()).sum();
    let toggle_key = key.clone();
    rsx! {
        if lane_id.is_some() {
            button {
                class: if is_collapsed { "lanehead closed" } else { "lanehead" },
                r#type: "button",
                aria_expanded: "{!is_collapsed}",
                onclick: move |_| {
                    let mut c = collapsed;
                    let mut set = c.write();
                    if !set.remove(&toggle_key) { set.insert(toggle_key.clone()); }
                },
                Icon { svg: icons::CHEVRON_DOWN }
                if let Some(color) = lane_color.clone() { span { class: "swatch", style: "background: #{color}" } }
                "{lane_name.clone().unwrap_or_else(|| \"Default lane\".into())}"
                span { class: "count", "{lane_total}" }
            }
        }
        if !is_collapsed {
            div { class: "board", style: "{grid_style}; margin-top: 10px",
                for (i, (col, cards, total)) in cells.into_iter().enumerate() {
                    ColumnCell {
                        key: "{col.id}",
                        index: i,
                        board_name: board_name.clone(),
                        column: col,
                        lane_id: lane_id.clone(),
                        cards,
                        total,
                        roots: roots.clone(),
                        root_titles: root_titles.clone(),
                        categories: categories.clone(),
                        tasks: tasks.clone(),
                    }
                }
            }
        }
    }
}

#[component]
fn ColumnCell(
    index: usize,
    board_name: String,
    column: kanban_core::Column,
    lane_id: Option<String>,
    cards: Vec<Card>,
    total: usize,
    roots: std::collections::HashMap<CardId, CardId>,
    root_titles: std::collections::HashMap<CardId, String>,
    categories: std::collections::HashMap<CardId, (kanban_core::StateCategory, String)>,
    tasks: std::collections::HashMap<CardId, (Vec<TaskItem>, Option<String>)>,
) -> Element {
    let mut st = use_app();
    let slot = st.drop_slot.read().clone();
    let dragging = match *st.drag.read() {
        Some(Drag::Card(id)) => Some(id),
        _ => None,
    };
    let here = slot
        .as_ref()
        .filter(|s| s.column_id == column.id && s.lane_id == lane_id && dragging.is_some());
    let incoming = dragging.is_some_and(|d| !cards.iter().any(|c| c.id == d));
    let over_limit = here.is_some()
        && incoming
        && column.item_limit > 0
        && total + 1 > column.item_limit as usize;
    let class = format!(
        "col{}{}",
        if here.is_some() { " over" } else { "" },
        if over_limit { " over-limit" } else { "" }
    );
    let before = here.map(|s| s.before);
    let col_id = column.id.clone();
    let lane = lane_id.clone();
    let col_for_drop = column.id.clone();
    let lane_for_drop = lane_id.clone();
    let col_for_tail = column.id.clone();
    let lane_for_tail = lane_id.clone();
    let board_for_drop = board_name.clone();
    let empty = cards.iter().all(|c| Some(c.id) == dragging);
    rsx! {
        section {
            class: "{class}",
            style: "--i: {index}",
            aria_label: "{column.name}",
            ondragover: move |e| {
                if dragging.is_none() { return; }
                e.prevent_default();
                let same = st.drop_slot.peek().as_ref().is_some_and(|s| s.column_id == col_id && s.lane_id == lane);
                if !same {
                    st.drop_slot.set(Some(DropSlot { column_id: col_id.clone(), lane_id: lane.clone(), before: None }));
                }
            },
            ondrop: move |e| {
                e.prevent_default();
                finish_drop(st, &board_for_drop, &col_for_drop, lane_for_drop.clone());
            },
            for (i, card) in cards.into_iter().enumerate() {
                Fragment { key: "{card.id}",
                if before == Some(Some(card.id)) && dragging != Some(card.id) {
                    div { class: "slot" }
                }
                CardView {
                    index: i,
                    epic: roots.get(&card.id).and_then(|r| root_titles.get(r).map(|t| (*r, t.clone()))),
                    state: categories.get(&card.id).cloned().unwrap_or((kanban_core::StateCategory::Unknown, kanban_core::StateCategory::Unknown.default_color().to_owned())),
                    column_id: column.id.clone(),
                    lane_id: lane_id.clone(),
                    tasks: tasks.get(&card.id).cloned().unwrap_or_default(),
                    card,
                }
                }
            }
            if before == Some(None) {
                div { class: "slot" }
            }
            if empty && before.is_none() {
                p { class: "empty", "Nothing here" }
            }
            div {
                class: "col-tail",
                ondragover: move |e| {
                    if dragging.is_none() { return; }
                    e.prevent_default();
                    e.stop_propagation();
                    let s = Some(DropSlot { column_id: col_for_tail.clone(), lane_id: lane_for_tail.clone(), before: None });
                    if *st.drop_slot.peek() != s { st.drop_slot.set(s); }
                },
            }
        }
    }
}

fn finish_drop(mut st: AppState, board: &str, column_id: &str, lane_id: Option<String>) {
    let drag = st.drag.take();
    let slot = st.drop_slot.take();
    let Some(Drag::Card(id)) = drag else { return };
    let before = slot
        .filter(|s| s.column_id == column_id && s.lane_id == lane_id)
        .and_then(|s| s.before);
    // Index in the full (unfiltered) column, not counting the dragged card.
    let index = st
        .workspace
        .peek()
        .as_ref()
        .and_then(|w| w.boards.get(board))
        .and_then(|b| b.placement.get(column_id))
        .map(|ids| {
            let rest: Vec<CardId> = ids.iter().copied().filter(|&x| x != id).collect();
            before
                .and_then(|b| rest.iter().position(|&x| x == b))
                .unwrap_or(rest.len())
        })
        .unwrap_or(0);
    services::move_card(st, board, id, column_id.to_owned(), lane_id, index);
}

#[component]
fn CardView(
    card: Card,
    index: usize,
    epic: Option<(CardId, String)>,
    /// Category and ADO color of the card's state.
    state: (kanban_core::StateCategory, String),
    column_id: String,
    lane_id: Option<String>,
    /// The card's tasks, and the task type when it can own tasks.
    tasks: (Vec<TaskItem>, Option<String>),
) -> Element {
    let mut st = use_app();
    let id = card.id;
    let dragging = *st.drag.read() == Some(Drag::Card(id));
    let pending = st.pending.read().contains(&id);
    let class = format!(
        "card{}{}",
        if dragging { " dragging" } else { "" },
        if pending { " pending" } else { "" }
    );
    let (category, color) = state;
    let edit = card.clone();
    let parent = card.clone();
    let (task_list, add_task) = tasks;
    let tags: Vec<String> = card.tags.iter().take(3).cloned().collect();
    rsx! {
        article {
            class: "{class}",
            style: "--i: {index}; --sc: #{color}",
            draggable: "true",
            aria_label: "{card.work_item_type} {id}: {card.title}",
            ondragstart: move |e| {
                let _ = e.data_transfer().set_data("text/plain", &id.to_string());
                e.data_transfer().set_effect_allowed("move");
                st.drag.set(Some(Drag::Card(id)));
            },
            ondragend: move |_| {
                st.drag.set(None);
                st.drop_slot.set(None);
            },
            ondragover: move |e| {
                if !matches!(*st.drag.peek(), Some(Drag::Card(_))) { return; }
                e.prevent_default();
                e.stop_propagation();
                if *st.drag.peek() == Some(Drag::Card(id)) { return; }
                let s = Some(DropSlot { column_id: column_id.clone(), lane_id: lane_id.clone(), before: Some(id) });
                if *st.drop_slot.peek() != s { st.drop_slot.set(s); }
            },
            div { class: "top",
                span {
                    span { class: "id", "{id}" }
                    Prio { priority: card.priority }
                }
            }
            if let Some((eid, title)) = epic {
                span { class: "epic-chip", "data-tip": "{eid} {title}", "{eid} {title}" }
            }
            button {
                class: "etitle sm",
                r#type: "button",
                draggable: "false",
                aria_label: "Edit {card.title}",
                onclick: move |_| open_edit(st, &edit, false),
                "{card.title}"
            }
            if !tags.is_empty() {
                div { class: "tags",
                    for t in tags { span { key: "{t}", class: "tag", "{t}" } }
                }
            }
            div { class: "bottom",
                Assignee { id, person: card.assigned_to.clone(), title: card.title.clone() }
                DateLabel { date: card.target_date, open: !category.is_done() }
            }
            if !task_list.is_empty() || add_task.is_some() {
                CardTasks { parent: parent.clone(), tasks: task_list.clone(), task_type: add_task.clone() }
            }
        }
    }
}

fn save_board_prefs(
    mut st: AppState,
    project: &str,
    team: &str,
    board: &str,
    f: impl FnOnce(&mut kanban_core::BoardPrefs),
) {
    {
        let mut settings = st.settings.write();
        let mut prefs = settings.board_prefs(project, team, board);
        f(&mut prefs);
        settings.set_board_prefs(project, team, board, prefs);
    }
    services::save_settings(st);
}
