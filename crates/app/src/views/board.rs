//! One team's workspace: top bar, page head with the List / Kanban / Timeline tabs, and the
//! active tab. `board` is the backlog level shown on the Kanban tab.

use dioxus::prelude::*;

use crate::components::epic_list::{EpicList, in_iteration};
use crate::components::icons;
use crate::components::kanban::Kanban;
use crate::components::timeline::Timeline;
use crate::components::widgets::{Avatar, Icon};
use crate::routes::Route;
use crate::services;
use crate::state::{Status, use_app};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    List,
    Kanban,
    Timeline,
}

impl Tab {
    fn parse(s: Option<&str>) -> Self {
        match s {
            Some("kanban") => Self::Kanban,
            Some("timeline") => Self::Timeline,
            _ => Self::List,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Kanban => "kanban",
            Self::Timeline => "timeline",
        }
    }
}

#[component]
pub fn Board(project: String, team: String, board: String) -> Element {
    let mut st = use_app();
    let mut tab = use_signal(|| Tab::parse(st.settings.peek().last_view.as_deref()));
    let mut iteration = use_signal(|| None::<String>);
    let search_focus = use_signal(|| None::<std::rc::Rc<MountedData>>);

    use_effect(move || {
        if st.client.read().is_none() {
            navigator().replace(Route::Setup {});
        }
    });
    use_effect(use_reactive!(|project, team| {
        services::open_workspace(st, project, team, false);
    }));
    use_effect(use_reactive!(|board| {
        let mut s = st.settings.write();
        if s.default_board.as_deref() != Some(board.as_str()) {
            s.default_board = Some(board);
            drop(s);
            services::save_settings(st);
        }
    }));

    let mut set_tab = move |t: Tab| {
        tab.set(t);
        st.settings.write().last_view = Some(t.key().into());
        services::save_settings(st);
    };

    let status = st.status.read().clone();
    let ws_guard = st.workspace.read();
    let loaded = ws_guard
        .as_ref()
        .filter(|w| w.project == project && w.team == team);
    let (counts, iterations, me) = match loaded {
        Some(ws) => {
            let roots = ws.root_types();
            let it = iteration.read().clone();
            let list = ws
                .cards
                .values()
                .filter(|c| roots.contains(&c.work_item_type))
                .count();
            let kanban = ws.boards.get(&board).map_or(0, |b| {
                ws.cards
                    .values()
                    .filter(|c| b.shows(c) && in_iteration(c, &it))
                    .count()
            });
            let timeline = ws
                .cards
                .values()
                .filter(|c| !roots.contains(&c.work_item_type) && in_iteration(c, &it))
                .count();
            (
                Some((list, kanban, timeline)),
                ws.iterations.clone(),
                ws.me.clone(),
            )
        }
        None => (None, Vec::new(), None),
    };
    let has_ws = loaded.is_some();
    drop(ws_guard);
    let last_sync = st.last_sync.read().and_then(|t| {
        t.to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC))
            .format(time::macros::format_description!("[hour]:[minute]"))
            .ok()
    });

    let current = tab();
    let (title, hint) = match current {
        Tab::List => (
            "Epic list",
            "Plan and prioritise epics. Expand one to see its work items, drag rows to reorder.",
        ),
        Tab::Kanban => (
            "Kanban board",
            "The team's board, columns and all. Drag cards between columns to move them.",
        ),
        Tab::Timeline => (
            "Timeline",
            "Work items by target date, earliest first. Drag a diamond to reschedule.",
        ),
    };
    let it_sel = iteration.read().clone();

    rsx! {
        div {
            class: "app",
            tabindex: "-1",
            onkeydown: move |e| {
                if e.key() == Key::Character("/".into()) && st.dialog.peek().is_none() {
                    e.prevent_default();
                    set_tab(Tab::List);
                    if let Some(m) = search_focus.peek().clone() {
                        spawn(async move { let _ = m.set_focus(true).await; });
                    }
                }
            },
            header { class: "topbar",
                div { class: "wrap",
                    div { class: "brand", span { class: "ico", dangerous_inner_html: icons::LOGO } "Board Manager" }
                    Link { class: "ghost", to: Route::Setup {}, aria_label: "Switch project or team",
                        span { class: "dim", "Project" }
                        span { style: "font-weight: 600", "{project} / {team}" }
                        Icon { svg: icons::CHEVRON_DOWN }
                    }
                    div { class: "spacer" }
                    span { class: "syncinfo",
                        span { class: if matches!(status, Status::Loading | Status::Refreshing) { "syncdot busy" } else { "syncdot" } }
                        match (&status, &last_sync) {
                            (Status::Loading, _) | (Status::Refreshing, _) => "Refreshing…".to_owned(),
                            (_, Some(t)) => format!("Synced {t}"),
                            _ => String::new(),
                        }
                    }
                    button {
                        class: "ghost icon-only",
                        r#type: "button",
                        aria_label: "Reload from Azure DevOps",
                        title: "Reload",
                        onclick: {
                            let (p, t) = (project.clone(), team.clone());
                            move |_| services::open_workspace(st, p.clone(), t.clone(), true)
                        },
                        Icon { svg: icons::REFRESH }
                    }
                    span { class: "select-wrap",
                        span { class: "lead", Icon { svg: icons::CALENDAR_16 } }
                        select {
                            class: "ghost",
                            aria_label: "Iteration",
                            style: "padding-left: 36px",
                            onchange: move |e| {
                                let v = e.value();
                                iteration.set((!v.is_empty()).then_some(v));
                            },
                            option { value: "", selected: it_sel.is_none(), "All iterations" }
                            for i in iterations {
                                option {
                                    key: "{i.path}",
                                    value: "{i.path}",
                                    selected: it_sel.as_deref() == Some(i.path.as_str()),
                                    if i.current { "{i.name} (current)" } else { "{i.name}" }
                                }
                            }
                        }
                        span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
                    }
                    Link { class: "ghost icon-only", to: Route::Settings {}, aria_label: "Settings", Icon { svg: icons::GEAR } }
                    if let Some(me) = me { Avatar { person: me, large: true } }
                }
            }
            div { class: "pagehead",
                div { class: "wrap",
                    div { class: "titlerow", h1 { "{title}" } }
                    p { "{hint}" }
                    div { class: "tabs", role: "tablist", aria_label: "Views",
                        TabButton { label: "List", icon: icons::LIST, on: current == Tab::List, count: counts.map(|c| c.0), onclick: move |_| set_tab(Tab::List) }
                        TabButton { label: "Kanban", icon: icons::KANBAN, on: current == Tab::Kanban, count: counts.map(|c| c.1), onclick: move |_| set_tab(Tab::Kanban) }
                        TabButton { label: "Timeline", icon: icons::TIMELINE, on: current == Tab::Timeline, count: counts.map(|c| c.2), onclick: move |_| set_tab(Tab::Timeline) }
                    }
                }
            }
            div { class: "scroll",
                if let Status::Error(msg) = &status {
                    div { class: "wrap", style: "padding-top: 24px",
                        div { class: "note err",
                            Icon { svg: icons::ALERT }
                            span { style: "flex: 1", "Couldn't load {team}: {msg}" }
                            button {
                                class: "ghost",
                                r#type: "button",
                                onclick: {
                                    let (p, t) = (project.clone(), team.clone());
                                    move |_| services::open_workspace(st, p.clone(), t.clone(), true)
                                },
                                "Retry"
                            }
                        }
                    }
                }
                if !has_ws {
                    if status == Status::Loading { Skeleton {} }
                } else {
                    main { key: "{current.key()}", class: "wrap panel",
                        match current {
                            Tab::List => rsx! { EpicList { iteration: it_sel.clone(), search_focus } },
                            Tab::Kanban => rsx! { Kanban { board: board.clone(), iteration: it_sel.clone() } },
                            Tab::Timeline => rsx! { Timeline { iteration: it_sel.clone() } },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TabButton(
    label: &'static str,
    icon: &'static str,
    on: bool,
    count: Option<usize>,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: if on { "tab on" } else { "tab" },
            role: "tab",
            r#type: "button",
            aria_selected: "{on}",
            onclick: move |e| onclick.call(e),
            Icon { svg: icon }
            "{label}"
            if let Some(n) = count { span { class: "count", "{n}" } }
        }
    }
}

#[component]
fn Skeleton() -> Element {
    rsx! {
        div { class: "wrap panel", aria_busy: "true", aria_label: "Loading",
            div { class: "toolbar",
                div { class: "skel", style: "width: 320px; height: 40px" }
                div { class: "skel", style: "width: 60px; height: 40px" }
                div { class: "skel", style: "width: 80px; height: 40px" }
                div { class: "skel", style: "width: 70px; height: 40px" }
            }
            div { class: "list",
                for i in 0..7 {
                    div { key: "{i}", class: "ehead", style: "padding: 16px 20px; gap: 16px",
                        div { class: "skel", style: "width: 56px; height: 14px" }
                        div { class: "skel", style: "width: 30px; height: 20px" }
                        div { class: "skel", style: "flex: 1; height: 16px; max-width: {360 - i * 24}px" }
                        div { class: "spacer" }
                        div { class: "skel", style: "width: 70px; height: 22px; border-radius: 999px" }
                        div { class: "skel", style: "width: 150px; height: 8px" }
                        div { class: "skel", style: "width: 28px; height: 28px; border-radius: 50%" }
                    }
                }
            }
        }
    }
}
