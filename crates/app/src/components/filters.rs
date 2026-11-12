//! The filter bar: `FilterMenu` (a button opening a checkbox popover, also used for the kanban
//! Columns menu; a filter's own checkbox switches it off without losing the selection), `ItemFilters` (epic status, assignee, area and work item status, for the List and the
//! Timeline, each with its own selection) and helpers that read and save the team's [`ViewPrefs`].

use dioxus::prelude::*;
use kanban_core::card::Person;
use kanban_core::filters::{self, ME, UNASSIGNED};
use kanban_core::{Card, TabFilters, ViewPrefs, Workspace};

use crate::components::icons;
use crate::components::widgets::Icon;
use crate::services;
use crate::state::{AppState, use_app};

/// The tabs with an [`ItemFilters`] bar; each keeps its own filters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ItemTab {
    List,
    Timeline,
}

impl ItemTab {
    pub fn of(self, view: &ViewPrefs) -> &TabFilters {
        match self {
            Self::List => &view.list,
            Self::Timeline => &view.timeline,
        }
    }

    pub fn of_mut(self, view: &mut ViewPrefs) -> &mut TabFilters {
        match self {
            Self::List => &mut view.list,
            Self::Timeline => &mut view.timeline,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilterOption {
    pub value: String,
    pub label: String,
    pub count: Option<usize>,
}

/// The loaded team's filters.
pub fn view_prefs(st: AppState, ws: &Workspace) -> ViewPrefs {
    st.settings.read().view_prefs(&ws.project, &ws.team)
}

/// Changes the team's filters and saves the settings.
pub fn update_view_prefs(
    mut st: AppState,
    project: &str,
    team: &str,
    f: impl FnOnce(&mut ViewPrefs),
) {
    {
        let mut settings = st.settings.write();
        let mut prefs = settings.view_prefs(project, team);
        f(&mut prefs);
        settings.set_view_prefs(project, team, prefs);
    }
    services::save_settings(st);
}

/// Me, everyone else assigned in `cards`, then Unassigned; counted over `cards`.
pub fn assignee_options(cards: &[&Card], me: Option<&Person>) -> Vec<FilterOption> {
    let count = |token: &str| {
        Some(
            cards
                .iter()
                .filter(|c| filters::matches_assignee(c, token, me))
                .count(),
        )
    };
    let mut out = Vec::new();
    if me.is_some() {
        out.push(FilterOption {
            value: ME.into(),
            label: "Me".into(),
            count: count(ME),
        });
    }
    for p in kanban_core::card::people(cards.iter().copied())
        .into_iter()
        .filter(|p| me.is_none_or(|m| m.unique_name != p.unique_name))
    {
        out.push(FilterOption {
            count: count(&p.unique_name),
            value: p.unique_name,
            label: p.display_name,
        });
    }
    out.push(FilterOption {
        value: UNASSIGNED.into(),
        label: "Unassigned".into(),
        count: count(UNASSIGNED),
    });
    out
}

/// Every area path of `cards` (plus the `selected` ones, so they can be unticked), labelled
/// without the project prefix and counted over `cards` including sub-areas.
pub fn area_options(cards: &[&Card], selected: &[String]) -> Vec<FilterOption> {
    let mut paths: Vec<String> = cards
        .iter()
        .map(|c| c.area_path.clone())
        .chain(selected.iter().cloned())
        .collect();
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .map(|path| {
            let one = [path.clone()];
            FilterOption {
                label: path
                    .split_once('\\')
                    .map_or(path.as_str(), |(_, rest)| rest)
                    .to_owned(),
                count: Some(
                    cards
                        .iter()
                        .filter(|c| filters::matches_areas(c, &one))
                        .count(),
                ),
                value: path,
            }
        })
        .collect()
}

/// One option per state, counted over `cards`.
pub fn state_options(states: &[String], cards: &[&Card]) -> Vec<FilterOption> {
    states
        .iter()
        .map(|s| FilterOption {
            value: s.clone(),
            label: s.clone(),
            count: Some(cards.iter().filter(|c| c.state == *s).count()),
        })
        .collect()
}

/// A button summarising the selection (`Label`, `Label: Me`, `Label: Me +2`) that opens a
/// checkbox popover. With `plain`, the button shows only the label and `badge`. With `on_toggle`,
/// a set filter gets a checkbox in its button that switches it on and off (`off`), keeping the
/// selection.
#[component]
pub fn FilterMenu(
    label: String,
    options: Vec<FilterOption>,
    selected: Vec<String>,
    on_change: EventHandler<Vec<String>>,
    #[props(default)] icon: Option<&'static str>,
    #[props(default)] plain: bool,
    #[props(default)] off: bool,
    #[props(default)] on_toggle: Option<EventHandler<bool>>,
    #[props(default)] badge: Option<usize>,
    #[props(default)] align_right: bool,
    #[props(default = "Clear".to_owned())] reset_label: String,
    #[props(default)] reset_to: Vec<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let active = !plain && !selected.is_empty();
    let toggle = on_toggle.filter(|_| active);
    let class = match (active, off, toggle.is_some()) {
        (false, ..) => "ghost filter",
        (true, true, _) => "ghost filter has-toggle paused",
        (true, false, true) => "ghost filter has-toggle on",
        (true, false, false) => "ghost filter on",
    };
    let names: Vec<&str> = options
        .iter()
        .filter(|o| selected.contains(&o.value))
        .map(|o| o.label.as_str())
        .collect();
    let summary = match names.as_slice() {
        _ if plain => None,
        [] => None,
        [one] => Some((*one).to_owned()),
        [first, rest @ ..] => Some(format!("{first} +{}", rest.len())),
    };
    let popover_class = if align_right {
        "popover"
    } else {
        "popover left"
    };
    rsx! {
        div { class: "popover-anchor",
            if let Some(toggle) = toggle {
                input {
                    class: "filter-toggle",
                    r#type: "checkbox",
                    checked: !off,
                    aria_label: "Apply the {label} filter",
                    "data-tip": if off { "Filter off: click to apply it again" } else { "Switch the filter off, keeping its selection" },
                    onchange: move |e: FormEvent| toggle.call(e.checked()),
                }
            }
            button {
                class,
                r#type: "button",
                aria_expanded: "{open}",
                aria_haspopup: "dialog",
                onclick: move |_| open.toggle(),
                if let Some(svg) = icon { Icon { svg } }
                span { class: "dim", "{label}" }
                if let Some(s) = summary { span { class: "sum", "{s}" } }
                if let Some(n) = badge.filter(|n| *n > 0) { span { class: "hidden-count", "{n}" } }
                Icon { svg: icons::CHEVRON_DOWN }
            }
            if open() {
                div { class: "popover-backdrop", onclick: move |_| open.set(false) }
                div {
                    class: "{popover_class}",
                    role: "dialog",
                    aria_label: "{label}",
                    onkeydown: move |e| if e.key() == Key::Escape { open.set(false) },
                    h3 { "{label}" }
                    div { class: "opts",
                        for o in options {
                            label { key: "{o.value}", class: "check",
                                input {
                                    r#type: "checkbox",
                                    checked: selected.contains(&o.value),
                                    onchange: {
                                        let value = o.value.clone();
                                        let selected = selected.clone();
                                        move |e: FormEvent| {
                                            let mut s = selected.clone();
                                            if e.checked() {
                                                if !s.contains(&value) { s.push(value.clone()); }
                                            } else {
                                                s.retain(|x| *x != value);
                                            }
                                            on_change.call(s);
                                        }
                                    },
                                }
                                span { class: "grow", "{o.label}" }
                                if let Some(n) = o.count { span { class: "count", "{n}" } }
                            }
                        }
                    }
                    div { class: "foot",
                        button {
                            class: "link",
                            r#type: "button",
                            onclick: {
                                let reset = reset_to.clone();
                                move |_| on_change.call(reset.clone())
                            },
                            "{reset_label}"
                        }
                        button { class: "link", r#type: "button", style: "float: right", onclick: move |_| open.set(false), "Done" }
                    }
                }
            }
        }
    }
}

/// Epic status, Assigned to, Area and Status menus (each can be switched off) plus "Clear filters",
/// for the List or Timeline.
#[component]
pub fn ItemFilters(tab: ItemTab) -> Element {
    let st = use_app();
    let ws_guard = st.workspace.read();
    let Some(ws) = ws_guard.as_ref() else {
        return rsx! {};
    };
    let view = view_prefs(st, ws);
    let prefs = tab.of(&view).clone();
    let iteration = view.iteration.as_deref();
    let hierarchy = ws.hierarchy();
    let me = ws.me.as_ref();

    let mut epic_states: Vec<String> = ws
        .root_types()
        .iter()
        .flat_map(|t| ws.states.states(t))
        .collect();
    epic_states.dedup();
    let epics: Vec<&Card> = hierarchy
        .roots
        .iter()
        .filter_map(|id| ws.cards.get(id))
        .collect();
    let epic_opts = state_options(&epic_states, &epics);

    // Counts (of work items) per menu ignore that menu's own selection and switched-off filters.
    let applied = prefs.applied();
    let without_assignees = TabFilters {
        assignees: Vec::new(),
        ..applied.clone()
    };
    let assignee_opts = assignee_options(
        &filters::timeline_items(ws, &hierarchy, &without_assignees, iteration),
        me,
    );
    let without_areas = TabFilters {
        areas: Vec::new(),
        ..applied.clone()
    };
    let mut area_pool = filters::timeline_items(ws, &hierarchy, &without_areas, iteration);
    if tab == ItemTab::List {
        // Epics match the area filter themselves on the List.
        area_pool.extend(epics.iter().copied());
    }
    let area_opts = area_options(&area_pool, &prefs.areas);
    let without_states = TabFilters {
        item_states: Vec::new(),
        ..applied.clone()
    };
    let item_states = filters::kanban_states(ws, &st.settings.read());
    let state_opts = state_options(
        &item_states,
        &filters::timeline_items(ws, &hierarchy, &without_states, iteration),
    );
    let (project, team) = (ws.project.clone(), ws.team.clone());
    drop(ws_guard);

    let save = move |f: Box<dyn FnOnce(&mut TabFilters)>| {
        update_view_prefs(st, &project, &team, |v| f(tab.of_mut(v)))
    };
    rsx! {
        FilterMenu {
            label: "Epic status",
            options: epic_opts,
            selected: prefs.epic_states.clone(),
            on_change: {
                let save = save.clone();
                move |v: Vec<String>| save(Box::new(move |p| p.epic_states = v))
            },
            off: prefs.epic_states_off,
            on_toggle: {
                let save = save.clone();
                move |on: bool| save(Box::new(move |p| p.epic_states_off = !on))
            },
        }
        FilterMenu {
            label: "Assigned to",
            options: assignee_opts,
            selected: prefs.assignees.clone(),
            on_change: {
                let save = save.clone();
                move |v: Vec<String>| save(Box::new(move |p| p.assignees = v))
            },
            off: prefs.assignees_off,
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
            off: prefs.areas_off,
            on_toggle: {
                let save = save.clone();
                move |on: bool| save(Box::new(move |p| p.areas_off = !on))
            },
        }
        FilterMenu {
            label: "Status",
            options: state_opts,
            selected: prefs.item_states.clone(),
            on_change: {
                let save = save.clone();
                move |v: Vec<String>| save(Box::new(move |p| p.item_states = v))
            },
            off: prefs.item_states_off,
            on_toggle: {
                let save = save.clone();
                move |on: bool| save(Box::new(move |p| p.item_states_off = !on))
            },
        }
        if prefs.is_filtering() {
            button {
                class: "link",
                r#type: "button",
                onclick: move |_| save(Box::new(|p| *p = TabFilters::default())),
                Icon { svg: icons::CLOSE }
                "Clear filters"
            }
        }
    }
}
