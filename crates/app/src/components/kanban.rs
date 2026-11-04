//! "Kanban" tab: the team's real board for one backlog level (columns, WIP limits, swimlanes
//! and state mappings from ADO), with a Features/Stories switcher, a Columns menu to hide
//! columns (saved per board), filters, and drag-and-drop between and within columns.

use std::collections::HashSet;

use dioxus::prelude::*;
use kanban_core::card::Person;
use kanban_core::{Board, Card, CardId, Workspace};

use crate::components::epic_list::{default_area, in_iteration};
use crate::components::icons;
use crate::components::item_dialog::{open_edit, open_new};
use crate::components::widgets::{Assignee, DateLabel, Icon, Prio, StatePill};
use crate::routes::Route;
use crate::services;
use crate::state::{AppState, Drag, DropSlot, use_app};

#[derive(Clone, PartialEq)]
enum PersonFilter {
    Everyone,
    Me,
    Unassigned,
    Person(String),
}

#[component]
pub fn Kanban(board: String, iteration: Option<String>) -> Element {
    let st = use_app();
    let mut epic_filter = use_signal(|| None::<CardId>);
    let mut person = use_signal(|| PersonFilter::Everyone);
    let mut menu_open = use_signal(|| false);
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

    // Filters: epic → iteration → person (person counts are taken before the person filter).
    let on_board: Vec<&Card> = ws.cards.values().filter(|c| b.shows(c)).collect();
    let epic_sel = *epic_filter.read();
    let base: Vec<&Card> = on_board
        .iter()
        .copied()
        .filter(|c| epic_sel.is_none_or(|e| hierarchy.root_of.get(&c.id) == Some(&e)))
        .filter(|c| in_iteration(c, &iteration))
        .collect();
    let pf = person.read().clone();
    let matches_person = |c: &Card, f: &PersonFilter| match f {
        PersonFilter::Everyone => true,
        PersonFilter::Me => me.as_ref().is_some_and(|m| {
            c.assigned_to
                .as_ref()
                .is_some_and(|a| a.unique_name == m.unique_name)
        }),
        PersonFilter::Unassigned => c.assigned_to.is_none(),
        PersonFilter::Person(u) => c.assigned_to.as_ref().is_some_and(|a| a.unique_name == *u),
    };
    let visible: HashSet<CardId> = base
        .iter()
        .filter(|c| matches_person(c, &pf))
        .map(|c| c.id)
        .collect();
    let people: Vec<Person> = kanban_core::card::people(base.iter().copied())
        .into_iter()
        .filter(|p| me.as_ref().is_none_or(|m| m.unique_name != p.unique_name))
        .collect();
    let mut person_chips: Vec<(PersonFilter, String, String, usize)> = vec![(
        PersonFilter::Everyone,
        "Everyone".into(),
        "Everyone".into(),
        base.len(),
    )];
    if me.is_some() {
        let n = base
            .iter()
            .filter(|c| matches_person(c, &PersonFilter::Me))
            .count();
        person_chips.push((PersonFilter::Me, "Me".into(), "Assigned to me".into(), n));
    }
    for p in &people {
        let f = PersonFilter::Person(p.unique_name.clone());
        let n = base.iter().filter(|c| matches_person(c, &f)).count();
        person_chips.push((f, p.initials(), p.display_name.clone(), n));
    }
    let unassigned = base.iter().filter(|c| c.assigned_to.is_none()).count();
    person_chips.push((
        PersonFilter::Unassigned,
        "Unassigned".into(),
        "Unassigned".into(),
        unassigned,
    ));

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
    rsx! {
        div { class: "toolbar",
            {switcher}
            label { class: "toolbar",
                span { class: "label", "Epic" }
                span { class: "select-wrap",
                    select {
                        class: "field",
                        style: "min-width: 260px; max-width: 360px",
                        onchange: move |e| epic_filter.set(e.value().parse().ok()),
                        option { value: "", selected: epic_sel.is_none(), "All epics" }
                        for (id, title) in epic_options {
                            option { key: "{id}", value: "{id}", selected: epic_sel == Some(id), "{id}  {title}" }
                        }
                    }
                    span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
                }
            }
            div { class: "spacer" }
            div { class: "popover-anchor",
                button {
                    class: "ghost",
                    r#type: "button",
                    aria_expanded: "{menu_open}",
                    onclick: move |_| menu_open.toggle(),
                    Icon { svg: icons::COLUMNS }
                    "Columns"
                    if hidden_n > 0 { span { class: "hidden-count", title: "{hidden_n} hidden", "{hidden_n}" } }
                }
                if menu_open() {
                    ColumnsMenu {
                        project: project.clone(),
                        team: team.clone(),
                        board: board.clone(),
                        columns: all_columns,
                        hidden: hidden.clone(),
                        on_close: move |_| menu_open.set(false),
                    }
                }
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
        div { class: "toolbar", role: "group", aria_label: "Filter by assignee",
            span { class: "label", "Assigned to" }
            for (f, label, title, count) in person_chips {
                button {
                    key: "{title}",
                    class: if pf == f { "chip on" } else { "chip" },
                    r#type: "button",
                    title: "{title}",
                    aria_pressed: "{pf == f}",
                    onclick: {
                        let f = f.clone();
                        move |_| person.set(f.clone())
                    },
                    "{label}"
                    span { class: "count", "{count}" }
                }
            }
        }

        if columns.is_empty() {
            div { class: "note", Icon { svg: icons::INFO } "All columns are hidden. Use the Columns menu to show some." }
        } else {
            div { class: "board-scroll",
                div { class: "board", style: "{grid_style}",
                    for (i, h) in header_cols.iter().enumerate() {
                        div { key: "h-{h.id}", class: "colhead", style: "--i: {i}",
                            StatePill { state: h.name.clone(), category: h.category }
                            span { class: "count", "{h.shown}" }
                            if h.limit > 0 {
                                span { class: if h.total > h.limit as usize { "wip over" } else { "wip" }, title: "WIP limit", "{h.total}/{h.limit}" }
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
    let category = c
        .state_mappings
        .iter()
        .next()
        .map(|(t, s)| ws.states.category(t, s))
        .unwrap_or(kanban_core::StateCategory::Unknown);
    ColumnHead {
        id: c.id.clone(),
        name: c.name.clone(),
        category,
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
    let categories: std::collections::HashMap<CardId, kanban_core::StateCategory> = cells
        .iter()
        .flat_map(|(_, cards, _)| cards.iter().map(|c| (c.id, ws.category(c))))
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
    categories: std::collections::HashMap<CardId, kanban_core::StateCategory>,
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
                    category: categories.get(&card.id).copied().unwrap_or(kanban_core::StateCategory::Unknown),
                    column_id: column.id.clone(),
                    lane_id: lane_id.clone(),
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
    category: kanban_core::StateCategory,
    column_id: String,
    lane_id: Option<String>,
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
    let edit = card.clone();
    let tags: Vec<String> = card.tags.iter().take(3).cloned().collect();
    rsx! {
        article {
            class: "{class}",
            style: "--i: {index}",
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
                if let Some((eid, title)) = epic {
                    span { class: "epic-chip", title: "{eid} {title}", "{eid} {title}" }
                }
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
                Assignee { id, person: card.assigned_to.clone(), title: card.title.clone(), tip_left: true }
                DateLabel { date: card.target_date, open: !category.is_done() }
            }
        }
    }
}

#[component]
fn ColumnsMenu(
    project: String,
    team: String,
    board: String,
    columns: Vec<String>,
    hidden: Vec<String>,
    on_close: EventHandler<()>,
) -> Element {
    let st = use_app();
    rsx! {
        div {
            class: "popover",
            role: "dialog",
            aria_label: "Show columns",
            onkeydown: move |e| if e.key() == Key::Escape { on_close.call(()) },
            h3 { "Show columns" }
            for name in columns {
                label { key: "{name}", class: "check",
                    input {
                        r#type: "checkbox",
                        checked: !hidden.contains(&name),
                        onchange: {
                            let name = name.clone();
                            let hidden = hidden.clone();
                            let (project, team, board) = (project.clone(), team.clone(), board.clone());
                            move |e: FormEvent| {
                                let mut h = hidden.clone();
                                if e.checked() { h.retain(|x| *x != name); } else if !h.contains(&name) { h.push(name.clone()); }
                                save_hidden(st, &project, &team, &board, h);
                            }
                        },
                    }
                    "{name}"
                }
            }
            div { class: "foot",
                button {
                    class: "link",
                    r#type: "button",
                    onclick: {
                        let (project, team, board) = (project.clone(), team.clone(), board.clone());
                        move |_| {
                            save_hidden(st, &project, &team, &board, Vec::new());
                            on_close.call(());
                        }
                    },
                    "Show all"
                }
                button { class: "link", r#type: "button", style: "float: right", onclick: move |_| on_close.call(()), "Done" }
            }
        }
    }
}

fn save_hidden(mut st: AppState, project: &str, team: &str, board: &str, hidden: Vec<String>) {
    {
        let mut settings = st.settings.write();
        let mut prefs = settings.board_prefs(project, team, board);
        prefs.hidden_columns = hidden;
        settings.set_board_prefs(project, team, board, prefs);
    }
    services::save_settings(st);
}
