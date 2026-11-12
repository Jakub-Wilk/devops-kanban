//! "Kanban" tab: the team's real board for one backlog level (columns, WIP limits, swimlanes
//! and state mappings from ADO), with a Features/Stories switcher, a Columns menu to hide
//! columns, epic, assignee and area filters (all saved per board, each with a checkbox that
//! switches it off without losing the selection), and drag-and-drop
//! between and within columns ("Move back" on the toast afterwards). Column headers show how many
//! cards the filters hide and, when one person's cards are shown, a WIP meter of those cards;
//! under them a "New" button per column opens the item dialog with that column's state. Both
//! rows stick to the top while the board scrolls.
//! The done column is a drop zone that closes items, with "View closed" for recent ones.
//! Cards show the timer and add-time actions in their header, the description's first line, the
//! first few tags and the time total; a chevron opens the full description and all tags, their
//! tasks and the user's 7pace entries (fetched in bulk for all shown cards). Two optional
//! settings: "Widen the hovered column", and "Open cards on hover", where a pin (saved per board)
//! keeps a card open at the top of its column instead of the chevron.

use std::collections::HashSet;

use dioxus::prelude::*;
use kanban_core::filters;
use kanban_core::{Board, Card, CardId, Workspace};

use crate::components::epic_list::default_area;
use crate::components::filters::{
    FilterMenu, FilterOption, area_options, assignee_options, view_prefs,
};
use crate::components::icons;
use crate::components::item_dialog::{open_edit, open_new_in_column};
use crate::components::tasks::{CardTasks, TaskItem, task_items};
use crate::components::time::{CardTimeActions, CardTimeLog, CardTimeTotal, EntryLoader};
use crate::components::widgets::{Assignee, DateLabel, Icon, Prio, StatePill};
use crate::routes::Route;
use crate::services;
use crate::state::{AppState, Drag, DropSlot, NewInColumn, use_app};

/// "Open cards on hover": a hovered card opens at once and closes this long after the mouse leaves.
const HOVER_CLOSE: std::time::Duration = std::time::Duration::from_millis(50);

/// "Widen the hovered column". Remembers the last column the mouse was over as the board's
/// `data-hc` (cells carry `data-kc`) and widens it through the board's `--tpl`; cleared when the mouse leaves the board or reaches the done column (an empty
/// `data-kc`: neutral ground, equal widths). Plain `:hover` would drop to "no column" in every
/// gap (between header rows, between columns, over a lane header) and make the widths jump.
/// Set in the DOM directly, so hovering never re-renders.
///
/// Attaches to the board element once (it marks itself); safe to run again at any time, so
/// [`attach_hover_tracking`] runs it whenever the board may be new or the setting changed.
const TRACK_HOVER_JS: &str = r#"
const el = document.getElementById('kanban-board');
if (!el || el.dataset.hoverWired) return;
el.dataset.hoverWired = '1';
// The hovered column takes 1.8 shares, the others 1 (the board keeps its width). Written as
// the board's --tpl, which main.css uses while the board has the `dyn` class.
const widen = (i) => {
  const n = el.querySelectorAll('.board.heads > .colhead').length;
  const tracks = [];
  for (let k = 0; k < n; k++) tracks.push(k === i ? 'minmax(0, 1.8fr)' : 'minmax(0, 1fr)');
  el.style.setProperty('--tpl', tracks.join(' '));
};
const reset = () => { delete el.dataset.hc; el.style.removeProperty('--tpl'); };
el.addEventListener('mouseover', (e) => {
  const c = e.target.closest ? e.target.closest('[data-kc]') : null;
  if (!c) return;
  if (c.dataset.kc === '') { if (el.dataset.hc !== undefined) reset(); }
  else if (el.dataset.hc !== c.dataset.kc) { el.dataset.hc = c.dataset.kc; widen(Number(c.dataset.kc)); }
});
el.addEventListener('mouseleave', reset);
"#;

/// (Re)attaches the hovered-column tracking to the kanban board, if one is shown. Called when
/// the board mounts, after "Widen the hovered column" changes, and when View options closes.
pub fn attach_hover_tracking() {
    let _ = document::eval(TRACK_HOVER_JS);
}

/// Tags a collapsed card shows before "+N".
const COLLAPSED_TAGS: usize = 3;

#[component]
pub fn Kanban(board: String) -> Element {
    let st = use_app();
    let collapsed_lanes = use_signal(HashSet::<String>::new);
    // Hooks before any early return. Effects run after the DOM is updated, so this attaches the
    // hovered-column tracking once the (possibly new) board is there, and again whenever
    // "Widen the hovered column" changes.
    let widen_setting = st.settings.read().appearance.kanban.hover_widen;
    use_effect(use_reactive!(|widen_setting| {
        let _ = widen_setting;
        attach_hover_tracking();
    }));

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
    // Menus show the selection (`prefs`); filters switched off don't apply (`applied`).
    let applied = prefs.applied();
    // A saved epic that was closed or moved away no longer filters.
    let epic_pick = prefs.epic.filter(|e| hierarchy.roots.contains(e));
    let epic_sel = epic_pick.filter(|_| !prefs.epic_off);
    let base = filters::kanban_cards(ws, &hierarchy, b, epic_sel, view.iteration.as_deref());
    let visible: HashSet<CardId> = base
        .iter()
        .filter(|c| filters::matches_board(c, &applied, me.as_ref()))
        .map(|c| c.id)
        .collect();
    let kanban_style = st.settings.read().appearance.kanban.clone();
    let (hover_widen, hover_open) = (kanban_style.hover_widen, kanban_style.hover_open);
    let pinned: HashSet<CardId> = if hover_open {
        prefs.pinned.iter().copied().collect()
    } else {
        HashSet::new()
    };
    // 7pace entries for every shown card (and the tracked one), fetched in bulk: one call per
    // 100 cards, once per card until Reload, so opening or hovering a card costs nothing.
    let timer_card = st.time.read().timer.as_ref().map(|t| t.work_item);
    let mut shown_ids: Vec<CardId> = visible.iter().copied().chain(timer_card).collect();
    shown_ids.sort_unstable();
    shown_ids.dedup();
    let in_areas: Vec<&Card> = base
        .iter()
        .copied()
        .filter(|c| filters::matches_areas(c, &applied.areas))
        .collect();
    let assignee_opts = assignee_options(&in_areas, me.as_ref());
    let of_assignees: Vec<&Card> = base
        .iter()
        .copied()
        .filter(|c| filters::matches_assignees(c, &applied.assignees, me.as_ref()))
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
    // Column templates as variables on the board; main.css picks one by the `dyn` class, so
    // turning "Widen the hovered column" on or off only toggles that class. While widening, the
    // hover script overrides `--tpl` (see TRACK_HOVER_JS).
    // `--tpl-eq` is spelled out per column, like the hover script's `--tpl`: a `repeat()`
    // doesn't animate to or from a plain list, so going back to even widths would snap.
    let board_vars = format!(
        "--tpl-fixed: repeat({n}, minmax(236px, 1fr)); --tpl-eq: {}",
        vec!["minmax(0, 1fr)"; n].join(" ")
    );
    let grid_style = format!("min-width: {}px", n * 248);
    let header_cols: Vec<ColumnHead> = columns
        .iter()
        .map(|c| column_head(ws, b, c, &visible))
        .collect();
    // The WIP meter counts only the cards shown (like ADO), so it only means something when
    // they are one person's: filtered to one person, or every shown card is the same assignee's.
    let wip_on = filters::filtered_to_one_person(&applied)
        || filters::single_assignee(visible.iter().filter_map(|id| ws.cards.get(id))).is_some();
    // "New" per column: the column's state for the board's default type, preset in the dialog.
    // None for the done column (dropping there closes) and columns that don't map the type.
    // The done column is neutral ground for widening: it never widens, and hovering it
    // returns the board to equal widths (its cells carry an empty `data-kc`).
    let closing_cols: Vec<bool> = columns
        .iter()
        .map(|c| c.kind == kanban_core::ColumnKind::Outgoing)
        .collect();
    let new_in: Vec<Option<NewInColumn>> = columns
        .iter()
        .map(|c| {
            (c.kind != kanban_core::ColumnKind::Outgoing)
                .then(|| c.state_mappings.get(&default_type))
                .flatten()
                .map(|state| NewInColumn {
                    field: b.fields.column.clone(),
                    name: c.name.clone(),
                    state: state.clone(),
                })
        })
        .collect();
    let col_shown: std::collections::HashMap<String, usize> = header_cols
        .iter()
        .map(|h| (h.id.clone(), h.shown))
        .collect();
    let board_clone = b.clone();
    let area = epic_sel
        .and_then(|e| ws.cards.get(&e))
        .map(|e| e.area_path.clone())
        .unwrap_or_else(|| default_area(ws));
    drop(ws_guard);

    let hidden_n = hidden.len();
    let filtering = epic_pick.is_some() || !prefs.assignees.is_empty() || !prefs.areas.is_empty();
    let (epic_off, assignees_off, areas_off) =
        (prefs.epic_off, prefs.assignees_off, prefs.areas_off);
    // Room for the checkbox before the "Epic" label once an epic is picked.
    let epic_pad = if epic_pick.is_some() { 76 } else { 52 };
    let save = {
        let (project, team, board) = (project.clone(), team.clone(), board.clone());
        move |f: Box<dyn FnOnce(&mut kanban_core::BoardPrefs)>| {
            save_board_prefs(st, &project, &team, &board, f)
        }
    };
    rsx! {
        div { class: "toolbar",
            {switcher}
            // The label sits inside the picker, like the iteration picker's icon.
            span { class: if epic_pick.is_some() && epic_off { "select-wrap has-toggle paused" } else if epic_pick.is_some() { "select-wrap has-toggle" } else { "select-wrap" },
                if epic_pick.is_some() {
                    input {
                        class: "filter-toggle",
                        r#type: "checkbox",
                        checked: !epic_off,
                        aria_label: "Apply the Epic filter",
                        "data-tip": if epic_off { "Filter off: click to apply it again" } else { "Switch the filter off, keeping the epic" },
                        onchange: {
                            let save = save.clone();
                            move |e: FormEvent| {
                                let off = !e.checked();
                                save(Box::new(move |p| p.epic_off = off))
                            }
                        },
                    }
                }
                span { class: "lead text", aria_hidden: "true", "Epic" }
                select {
                        class: "field",
                        aria_label: "Epic",
                        style: "min-width: 220px; max-width: 360px; padding-left: {epic_pad}px",
                        onchange: {
                            let save = save.clone();
                            move |e: FormEvent| {
                                let epic = e.value().parse().ok();
                                save(Box::new(move |p| p.epic = epic))
                            }
                        },
                        option { value: "", selected: epic_pick.is_none(), "All epics" }
                        for (id, title) in epic_options {
                            option { key: "{id}", value: "{id}", selected: epic_pick == Some(id), "{id}  {title}" }
                        }
                    }
                    span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
            }
            FilterMenu {
                label: "Assigned to",
                options: assignee_opts,
                selected: prefs.assignees.clone(),
                on_change: {
                    let save = save.clone();
                    move |v: Vec<String>| save(Box::new(move |p| p.assignees = v))
                },
                off: assignees_off,
                on_toggle: {
                    let save = save.clone();
                    move |on: bool| save(Box::new(move |p| p.assignees_off = !on))
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
                off: areas_off,
                on_toggle: {
                    let save = save.clone();
                    move |on: bool| save(Box::new(move |p| p.areas_off = !on))
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
                            p.epic_off = false;
                            p.assignees_off = false;
                            p.areas_off = false;
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
        }

        EntryLoader { ids: shown_ids }
        if columns.is_empty() {
            div { class: "note", Icon { svg: icons::INFO } "All columns are hidden. Use the Columns menu to show some." }
        } else {
            div {
                id: "kanban-board",
                class: if hover_widen { "board-scroll dyn" } else { "board-scroll" },
                style: "{board_vars}",
                onmounted: move |_| attach_hover_tracking(),
                div { class: "board heads", style: "{grid_style}",
                    for (i, (h, neutral)) in header_cols.iter().zip(closing_cols.iter().copied()).enumerate() {
                        div { key: "h-{h.id}", class: "colhead", "data-kc": if neutral { String::new() } else { i.to_string() }, style: "--i: {i}",
                            StatePill { state: h.name.clone(), category: h.category, color: h.color.clone() }
                            span { class: "count", "{h.shown}" }
                            if h.total > h.shown {
                                span { class: "hidden-n", "data-tip": "Hidden by the filters", "{h.total - h.shown} hidden" }
                            }
                            if h.limit > 0 && wip_on {
                                span {
                                    class: if h.shown > h.limit as usize { "wip over" } else { "wip" },
                                    "data-tip": "Work in progress of the shown person against the column's WIP limit",
                                    "{h.shown}/{h.limit} WIP"
                                }
                            }
                        }
                    }
                    // Second header row: a "New" button per column.
                    for (i, (h, preset)) in header_cols.iter().zip(new_in).enumerate() {
                        match preset {
                            Some(c) => {
                                let t = default_type.clone();
                                let area = area.clone();
                                let tip = format!("New {} in {}", default_type.to_lowercase(), h.name);
                                rsx! {
                                    button {
                                        key: "n-{h.id}",
                                        class: "col-add",
                                        "data-kc": "{i}",
                                        r#type: "button",
                                        "data-tip": "{tip}",
                                        aria_label: "{tip}",
                                        onclick: move |_| open_new_in_column(st, t.clone(), epic_sel, area.clone(), c.clone()),
                                        Icon { svg: icons::PLUS_14 }
                                        "New"
                                    }
                                }
                            }
                            None => rsx! { span { key: "n-{h.id}", "data-kc": if closing_cols[i] { String::new() } else { i.to_string() } } },
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
                        pinned: pinned.clone(),
                        hover_open,
                        collapsed: collapsed_lanes,
                        wip_on,
                        col_shown: col_shown.clone(),
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
    /// "Open cards on hover": pinned cards go first in their column.
    pinned: HashSet<CardId>,
    hover_open: bool,
    collapsed: Signal<HashSet<String>>,
    /// The WIP meter applies (one person's cards are shown).
    wip_on: bool,
    /// Column id → cards shown in it across lanes.
    col_shown: std::collections::HashMap<String, usize>,
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
            let mut cards: Vec<Card> = all
                .iter()
                .filter(|id| visible.contains(id))
                .filter_map(|id| ws.cards.get(id))
                .filter(|card| lane_id.as_ref().is_none_or(|l| board.lane_of(card) == *l))
                .cloned()
                .collect();
            // Stable: pinned cards first, each group in board order.
            cards.sort_by_key(|c| !pinned.contains(&c.id));
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
                        shown_in_column: col_shown.get(&col.id).copied().unwrap_or(0),
                        column: col,
                        lane_id: lane_id.clone(),
                        cards,
                        total,
                        roots: roots.clone(),
                        root_titles: root_titles.clone(),
                        categories: categories.clone(),
                        tasks: tasks.clone(),
                        wip_on,
                        pinned: pinned.clone(),
                        hover_open,
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
    wip_on: bool,
    shown_in_column: usize,
    pinned: HashSet<CardId>,
    hover_open: bool,
) -> Element {
    let mut st = use_app();
    let mut show_closed = use_signal(|| false);
    // Memos, so a moving drop slot re-renders only the column it leaves and the one it enters
    // (reading the signals directly re-rendered every column of every lane on each change).
    let dragging = use_memo(move || match *st.drag.read() {
        Some(Drag::Card(id)) => Some(id),
        _ => None,
    })();
    // `Some(before)` when the drop slot is in this cell.
    let slot_here = use_memo(use_reactive(
        (&column.id, &lane_id),
        move |(column_id, lane_id)| {
            st.drop_slot
                .read()
                .as_ref()
                .filter(|s| s.column_id == column_id && s.lane_id == lane_id)
                .map(|s| s.before)
        },
    ))();
    let before = slot_here.filter(|_| dragging.is_some());
    let here = before.is_some();
    let incoming = dragging.is_some_and(|d| !cards.iter().any(|c| c.id == d));
    let over_limit = here
        && incoming
        && wip_on
        && column.item_limit > 0
        && shown_in_column + 1 > column.item_limit as usize;
    let limit = column.item_limit;
    let closed_cards: Vec<(CardId, Card)> = cards.iter().map(|c| (c.id, c.clone())).collect();
    let class = format!(
        "col{}{}",
        if here { " over" } else { "" },
        if over_limit { " over-limit" } else { "" }
    );
    let col_id = column.id.clone();
    let lane = lane_id.clone();
    let col_for_drop = column.id.clone();
    let lane_for_drop = lane_id.clone();
    let col_for_tail = column.id.clone();
    let lane_for_tail = lane_id.clone();
    let board_for_drop = board_name.clone();
    let empty = cards.iter().all(|c| Some(c.id) == dragging);
    // The done (Outgoing) column doesn't list its items: dropping on it closes one.
    let closing = column.kind == kanban_core::ColumnKind::Outgoing;
    let zone_class = if here && incoming {
        "close-zone over"
    } else {
        "close-zone"
    };
    let window = kanban_core::queries::COMPLETED_WINDOW_DAYS;
    rsx! {
        section {
            class: "{class}",
            style: "--i: {index}",
            "data-kc": if closing { String::new() } else { index.to_string() },
            aria_label: "{column.name}",
            ondragover: move |e| {
                if dragging.is_none() { return; }
                e.prevent_default();
                // Handled: the page's handler clears the slot only outside every column.
                e.stop_propagation();
                let same = st.drop_slot.peek().as_ref().is_some_and(|s| s.column_id == col_id && s.lane_id == lane);
                if !same {
                    st.drop_slot.set(Some(DropSlot { column_id: col_id.clone(), lane_id: lane.clone(), before: None }));
                }
            },
            ondrop: move |e| {
                e.prevent_default();
                finish_drop(st, &board_for_drop, &col_for_drop, lane_for_drop.clone());
            },
            if closing {
                div { class: "{zone_class}",
                    div { class: "close-zone-inner",
                        Icon { svg: icons::CHECK }
                        strong { if here && incoming { "Release to close" } else { "Drop here to close" } }
                        span { "data-tip": "Closed items stay listed on the List and Timeline", "{total} closed in the last {window} days" }
                        if !closed_cards.is_empty() {
                            button {
                                class: "link sm",
                                r#type: "button",
                                aria_expanded: "{show_closed()}",
                                onclick: move |_| show_closed.toggle(),
                                if show_closed() { "Hide closed" } else { "View closed" }
                            }
                        }
                        if show_closed() {
                            ul { class: "closed-list",
                                for (cid, c) in closed_cards.clone() {
                                    li { key: "{cid}",
                                        button {
                                            class: "task-title",
                                            r#type: "button",
                                            onclick: move |_| open_edit(st, &c, false),
                                            span { class: "id", "{cid}" }
                                            " {c.title}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for (i, card) in cards.into_iter().enumerate().filter(|_| !closing) {
                Fragment { key: "{card.id}",
                if before == Some(Some(card.id)) && dragging != Some(card.id) {
                    div { class: "slot" }
                    if over_limit { p { class: "wip-warn", "Over the WIP limit: this makes {shown_in_column + 1} of {limit}" } }
                }
                CardView {
                    index: i,
                    epic: roots.get(&card.id).and_then(|r| root_titles.get(r).map(|t| (*r, t.clone()))),
                    state: categories.get(&card.id).cloned().unwrap_or((kanban_core::StateCategory::Unknown, kanban_core::StateCategory::Unknown.default_color().to_owned())),
                    column_id: column.id.clone(),
                    lane_id: lane_id.clone(),
                    tasks: tasks.get(&card.id).cloned().unwrap_or_default(),
                    pin: hover_open.then(|| pinned.contains(&card.id)),
                    board_name: board_name.clone(),
                    card,
                }
                }
            }
            if before == Some(None) {
                div { class: "slot" }
                if over_limit { p { class: "wip-warn", "Over the WIP limit: this makes {shown_in_column + 1} of {limit}" } }
            }
            if empty && before.is_none() && !closing {
                p { class: "empty", "Nothing here" }
            }
            // The close zone already fills the done column.
            if !closing {
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
    /// "Open cards on hover": whether the card is pinned. `None` = classic (expand chevron).
    pin: Option<bool>,
    board_name: String,
) -> Element {
    let mut st = use_app();
    // Hover mode: open as soon as the mouse is on the card, close a moment after it leaves.
    let mut hovering = use_signal(|| false);
    let mut hover_open = use_signal(|| false);
    let pinned = pin == Some(true);
    let id = card.id;
    // A memo: a drag start or end re-renders only the dragged card.
    let dragging = use_memo(move || *st.drag.read() == Some(Drag::Card(id)))();
    let pending = st.pending.read().contains(&id);
    let tracking = st
        .time
        .read()
        .timer
        .as_ref()
        .is_some_and(|t| t.work_item == id);
    let accent = st.settings.read().appearance.kanban.accent.class();
    let tint = if st.settings.read().appearance.kanban.tint {
        " tint"
    } else {
        ""
    };
    let (category, color) = state;
    let edit = card.clone();
    let parent = card.clone();
    let (task_list, add_task) = tasks;
    let expanded = match pin {
        Some(p) => p || hover_open(),
        None => st.card_open.read().contains(&id),
    };
    let class = format!(
        "card {accent}{tint}{}{}{}{}{}",
        if expanded { " open" } else { "" },
        if pinned { " pinned" } else { "" },
        if dragging { " dragging" } else { "" },
        if pending { " pending" } else { "" },
        if tracking { " tracking" } else { "" }
    );
    // Collapsed: the first few tags and the description's first line; expanded: all of both.
    // Both are always rendered and fold into each other with a CSS transition (`.fold` /
    // `.unfold`), so opening and closing animate both ways.
    let tags: Vec<String> = card.tags.iter().take(COLLAPSED_TAGS).cloned().collect();
    let all_tags = card.tags.clone();
    let more_tags = card.tags.len() - tags.len();
    let more_tip = card.tags[tags.len()..].join(", ");
    let desc_line = kanban_core::description::first_line(&card.description);
    let desc_html = desc_line
        .as_ref()
        .map(|_| kanban_core::description::sanitize_for_display(&card.description));
    // Only a description or hidden tags make the summary and the full view differ.
    let swaps = desc_line.is_some() || more_tags > 0;
    let tasks_done = task_list.iter().filter(|t| t.category.is_done()).count();
    let has_details = !task_list.is_empty()
        || add_task.is_some()
        || crate::time_tracking::connected(st)
        || desc_line.is_some()
        || more_tags > 0;
    // The epic line uses the epic's type colour.
    let epic_color = epic.as_ref().and_then(|(eid, _)| {
        st.workspace
            .read()
            .as_ref()
            .and_then(|w| w.cards.get(eid).map(|e| w.type_color(&e.work_item_type)))
    });
    rsx! {
        article {
            class: "{class}",
            style: "--i: {index}; --sc: #{color}",
            draggable: "true",
            aria_label: "{card.work_item_type} {id}: {card.title}",
            onmouseenter: move |_| {
                if pin.is_none() { return; }
                hovering.set(true);
                if !*hover_open.peek() {
                    hover_open.set(true);
                }
            },
            onmouseleave: move |_| {
                if pin.is_none() { return; }
                hovering.set(false);
                // Close only if the mouse stays away, so brushing past the edge (or a card
                // shifting under the mouse as it closes) doesn't snap it shut.
                spawn(async move {
                    tokio::time::sleep(HOVER_CLOSE).await;
                    if !*hovering.peek() && *hover_open.peek() {
                        hover_open.set(false);
                    }
                });
            },
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
                    if !task_list.is_empty() {
                        span { class: "task-sum", "data-tip": "Tasks done", Icon { svg: icons::CHECK_14 } "{tasks_done}/{task_list.len()}" }
                    }
                }
                div { class: "quick",
                    CardTimeActions { card: parent.clone() }
                }
                if let Some(on) = pin {
                    button {
                        class: if on { "pin on" } else { "pin" },
                        r#type: "button",
                        draggable: "false",
                        aria_pressed: "{on}",
                        aria_label: if on { "Unpin" } else { "Pin open at the top" },
                        "data-tip": if on { "Unpin" } else { "Pin open at the top" },
                        onclick: {
                            let board_name = board_name.clone();
                            move |e: MouseEvent| {
                                e.stop_propagation();
                                let Some((project, team)) = st.workspace.peek().as_ref().map(|w| (w.project.clone(), w.team.clone())) else { return; };
                                save_board_prefs(st, &project, &team, &board_name, |p| {
                                    if on {
                                        p.pinned.retain(|x| *x != id);
                                    } else if !p.pinned.contains(&id) {
                                        p.pinned.push(id);
                                    }
                                });
                            }
                        },
                        Icon { svg: icons::PIN }
                    }
                } else if has_details {
                    button {
                        class: if expanded { "chev open" } else { "chev" },
                        r#type: "button",
                        draggable: "false",
                        aria_expanded: "{expanded}",
                        aria_label: if expanded { "Hide details" } else { "Show details" },
                        "data-tip": if expanded { "Hide details" } else { "Show details" },
                        onclick: move |e| {
                            e.stop_propagation();
                            let mut open = st.card_open.write();
                            if !open.remove(&id) { open.insert(id); }
                        },
                        Icon { svg: icons::CHEVRON_DOWN }
                    }
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
            if let Some((eid, title)) = epic {
                span { class: "card-epic", "data-tip": "{eid} {title}",
                    i { style: "background: #{epic_color.clone().unwrap_or_default()}" }
                    span { "{title}" }
                }
            }
            if swaps {
                // Collapsed summary: folds away as the card opens.
                div { class: "fold", "inert": if expanded { Some("true") } else { None },
                    div { class: "fold-in",
                        if let Some(line) = desc_line.clone() { p { class: "desc-line", "{line}" } }
                        if !tags.is_empty() {
                            div { class: "tags one-row",
                                for t in tags.clone() { span { key: "{t}", class: "tag", "{t}" } }
                                if more_tags > 0 {
                                    span { class: "tag more", "data-tip": "{more_tip}", "+{more_tags}" }
                                }
                            }
                        }
                    }
                }
                // Full description and every tag: unfold as the card opens.
                div { class: "unfold", "inert": if expanded { None } else { Some("true") },
                    div { class: "fold-in",
                        if let Some(html) = desc_html { div { class: "card-desc", dangerous_inner_html: "{html}" } }
                        if !all_tags.is_empty() {
                            div { class: "tags",
                                for t in all_tags { span { key: "{t}", class: "tag", "{t}" } }
                            }
                        }
                    }
                }
            } else if !tags.is_empty() {
                div { class: "tags",
                    for t in tags { span { key: "{t}", class: "tag", "{t}" } }
                }
            }
            div { class: "bottom",
                Assignee { id, person: card.assigned_to.clone(), title: card.title.clone() }
                div { class: "spacer" }
                DateLabel { date: card.target_date, open: !category.is_done(), compact: true }
                CardTimeTotal { completed_work: card.completed_work }
            }
            if has_details {
                div { class: "unfold", "inert": if expanded { None } else { Some("true") },
                    div { class: "fold-in",
                        div { class: "card-more",
                            if !task_list.is_empty() || add_task.is_some() {
                                CardTasks { parent: parent.clone(), tasks: task_list.clone(), task_type: add_task.clone() }
                            }
                            CardTimeLog { card: parent.clone() }
                        }
                    }
                }
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
