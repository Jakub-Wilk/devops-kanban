//! Connection setup (organization + PAT) and the project → team picker.

use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::components::icons;
use crate::components::widgets::Icon;
use crate::routes::Route;
use crate::services;
use crate::state::use_app;

/// Jump straight to the default board only once per run, so "switch project" can come back here.
static AUTO_OPENED: AtomicBool = AtomicBool::new(false);

#[component]
pub fn Setup() -> Element {
    let st = use_app();
    let connected = st.client.read().is_some();

    use_effect(move || {
        let s = st.settings.peek().clone();
        if st.client.read().is_some()
            && !AUTO_OPENED.swap(true, Ordering::Relaxed)
            && let (Some(project), Some(team)) = (s.default_project, s.default_team)
        {
            navigator().replace(Route::Board {
                project,
                team,
                board: s.default_board.unwrap_or_else(|| "Stories".into()),
            });
        }
    });

    rsx! {
        div { class: "center",
            div { class: "setup",
                div { class: "steps", i { class: "on" } i { class: if connected { "on" } else { "" } } }
                if connected { Picker {} } else { Connect {} }
            }
        }
    }
}

#[component]
fn Connect() -> Element {
    let st = use_app();
    let mut org = use_signal(|| st.settings.peek().organization.clone());
    let mut pat = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| st.auth_error.peek().clone());
    let disabled = busy() || org.read().trim().is_empty() || pat.read().trim().is_empty();
    rsx! {
        form {
            class: "card-lg",
            onsubmit: move |e| {
                e.prevent_default();
                busy.set(true);
                error.set(None);
                spawn(async move {
                    if let Err(msg) = services::connect(st, org(), pat()).await {
                        error.set(Some(msg));
                    }
                    busy.set(false);
                });
            },
            h2 { "Connect to Azure DevOps" }
            p { "Create a personal access token in Azure DevOps (User settings → Personal access tokens) with the Work Items: Read & write scope (add Graph: Read to see profile pictures). It is stored in your OS keyring, never in a file." }
            label { class: "f",
                span { class: "lbl", "Organization" }
                input { class: "field", r#type: "text", placeholder: "media-press", value: "{org}", oninput: move |e| org.set(e.value()) }
            }
            label { class: "f",
                span { class: "lbl", "Personal access token" }
                input { class: "field", r#type: "password", autocomplete: "off", value: "{pat}", oninput: move |e| pat.set(e.value()) }
            }
            if let Some(msg) = error() {
                div { class: "note err", Icon { svg: icons::ALERT } "{msg}" }
            }
            div { style: "display: flex; justify-content: flex-end",
                button { class: "primary", r#type: "submit", disabled,
                    if busy() { span { class: "spinner" } }
                    "Connect"
                }
            }
        }
    }
}

#[component]
fn Picker() -> Element {
    let mut st = use_app();
    let defaults = st.settings.peek().clone();
    let mut project = use_signal(|| defaults.default_project.clone().unwrap_or_default());
    let mut team = use_signal(|| defaults.default_team.clone().unwrap_or_default());

    let projects = use_resource(move || async move {
        let client = st.client.read().clone()?;
        Some(client.projects().await.map_err(|e| e.to_string()))
    });
    let teams = use_resource(move || async move {
        let p = project();
        if p.is_empty() {
            return None;
        }
        let client = st.client.peek().clone()?;
        Some(client.teams(&p).await.map_err(|e| e.to_string()))
    });

    let mut project_list = match &*projects.read() {
        Some(Some(Ok(list))) => list.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    project_list.sort_by_key(|p| p.to_lowercase());
    let project_err = match &*projects.read() {
        Some(Some(Err(e))) => Some(e.clone()),
        _ => None,
    };
    let mut team_list = match &*teams.read() {
        Some(Some(Ok(list))) => list.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    team_list.sort_by_key(|t| t.to_lowercase());
    // Pick the project's default team when the current choice isn't in the list.
    if !team_list.is_empty() && !team_list.contains(&team.peek()) {
        let p = project.peek().clone();
        let guess = team_list
            .iter()
            .find(|t| **t == format!("{p} Team"))
            .unwrap_or(&team_list[0])
            .clone();
        team.set(guess);
    }
    let loading_projects = projects.read().is_none();
    let no_teams = team_list.is_empty();
    let can_open = !project.read().is_empty() && team_list.contains(&team.read());

    rsx! {
        div { class: "card-lg",
            h2 { "Choose a team" }
            p { "Signed in to {st.settings.read().organization}. Pick the project and team whose boards you want to work on." }
            label { class: "f",
                span { class: "lbl", "Project" }
                span { class: "select-wrap",
                    select {
                        class: "field",
                        disabled: loading_projects,
                        onchange: move |e| project.set(e.value()),
                        option { value: "", selected: project.read().is_empty(), if loading_projects { "Loading projects…" } else { "Select a project" } }
                        for p in project_list {
                            option { key: "{p}", value: "{p}", selected: *project.read() == p, "{p}" }
                        }
                    }
                    span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
                }
            }
            label { class: "f",
                span { class: "lbl", "Team" }
                span { class: "select-wrap",
                    select {
                        class: "field",
                        disabled: no_teams,
                        onchange: move |e| team.set(e.value()),
                        for t in team_list {
                            option { key: "{t}", value: "{t}", selected: *team.read() == t, "{t}" }
                        }
                    }
                    span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
                }
            }
            if let Some(e) = project_err {
                div { class: "note err", Icon { svg: icons::ALERT } "{e}" }
            }
            div { style: "display: flex; justify-content: space-between; align-items: center; gap: 12px",
                button {
                    class: "link",
                    r#type: "button",
                    onclick: move |_| {
                        let org = st.settings.peek().organization.clone();
                        let _ = kanban_core::config::delete_pat(&org);
                        services::stop_sync(st);
                        st.client.set(None);
                        st.pat.set(None);
                        st.workspace.set(None);
                    },
                    "Use a different token"
                }
                button {
                    class: "primary",
                    r#type: "button",
                    disabled: !can_open,
                    onclick: move |_| {
                        let (p, t) = (project(), team());
                        let board = {
                            let mut s = st.settings.write();
                            s.default_project = Some(p.clone());
                            s.default_team = Some(t.clone());
                            s.default_board.clone().unwrap_or_else(|| "Stories".into())
                        };
                        services::save_settings(st);
                        AUTO_OPENED.store(true, Ordering::Relaxed);
                        navigator().push(Route::Board { project: p, team: t, board });
                    },
                    "Open boards"
                }
            }
        }
    }
}
