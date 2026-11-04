//! Settings editor (see docs/settings.md), including the 7pace time tracking token.

use dioxus::prelude::*;

use crate::components::icons;
use crate::components::widgets::Icon;
use crate::routes::Route;
use crate::services;
use crate::state::use_app;
use crate::time_tracking;

#[component]
pub fn Settings() -> Element {
    let mut st = use_app();
    let s = st.settings.read().clone();
    let hidden_boards = s
        .boards
        .values()
        .filter(|b| !b.hidden_columns.is_empty())
        .count();
    let back = match (s.default_project.clone(), s.default_team.clone()) {
        (Some(project), Some(team)) => Route::Board {
            project,
            team,
            board: s.default_board.clone().unwrap_or_else(|| "Stories".into()),
        },
        _ => Route::Setup {},
    };
    rsx! {
        div { class: "app",
            header { class: "topbar",
                div { class: "wrap",
                    div { class: "spacer" }
                    Link { class: "ghost", to: back, Icon { svg: icons::BACK } "Back to board" }
                }
            }
            div { class: "scroll",
                main { class: "wrap panel settings",
                    h1 { style: "margin: 0; font-size: 26px; letter-spacing: -0.02em", "Settings" }
                    div { class: "list", style: "padding: 4px 20px",
                        div { class: "row",
                            div { strong { "Organization" } p { "{s.organization}" } }
                            button {
                                class: "ghost",
                                r#type: "button",
                                onclick: move |_| {
                                    let org = st.settings.peek().organization.clone();
                                    let _ = kanban_core::config::delete_pat(&org);
                                    services::stop_sync(st);
                                    st.client.set(None);
                                    st.pat.set(None);
                                    st.workspace.set(None);
                                    navigator().push(Route::Setup {});
                                },
                                "Sign out"
                            }
                        }
                        div { class: "row",
                            div { strong { "Sync interval" } p { "How often to check Azure DevOps for changes (seconds, at least 10). Applies to the next board you open." } }
                            input {
                                class: "field",
                                r#type: "number",
                                min: "10",
                                style: "width: 110px",
                                value: "{s.poll_interval_secs}",
                                onchange: move |e| {
                                    if let Ok(v) = e.value().parse::<u64>() {
                                        st.settings.write().poll_interval_secs = v.max(10);
                                        services::save_settings(st);
                                    }
                                },
                            }
                        }
                        div { class: "row",
                            div { strong { "Cache boards" } p { "Show the last loaded board immediately on startup, then refresh." } }
                            input {
                                r#type: "checkbox",
                                style: "width: 18px; height: 18px; accent-color: var(--accent)",
                                checked: s.cache_enabled,
                                onchange: move |e| {
                                    st.settings.write().cache_enabled = e.checked();
                                    services::save_settings(st);
                                },
                            }
                        }
                        div { class: "row",
                            div { strong { "Hidden columns" } p { "{hidden_boards} board(s) have hidden columns." } }
                            button {
                                class: "ghost",
                                r#type: "button",
                                disabled: hidden_boards == 0,
                                onclick: move |_| {
                                    st.settings.write().boards.clear();
                                    services::save_settings(st);
                                },
                                "Show all columns everywhere"
                            }
                        }
                        div { class: "row",
                            div { strong { "Log level" } p { "tracing filter, e.g. info or kanban_core=debug. RUST_LOG overrides it; applies on restart." } }
                            input {
                                class: "field",
                                r#type: "text",
                                style: "width: 200px",
                                value: "{s.log_level}",
                                onchange: move |e| {
                                    st.settings.write().log_level = e.value();
                                    services::save_settings(st);
                                },
                            }
                        }
                    }
                    TimeTrackingSettings {}
                }
            }
        }
    }
}

/// Enable 7pace and manage its API token (keyring account `7pace:<organization>`).
#[component]
fn TimeTrackingSettings() -> Element {
    let mut st = use_app();
    let mut token = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut message = use_signal(|| None::<Result<String, String>>);
    let enabled = st.settings.read().time_tracking.enabled;
    let t = st.time.read();
    let status = match (&t.me, &t.error) {
        (Some(me), _) => format!("Connected as {me}."),
        (None, Some(e)) => e.clone(),
        (None, None) if enabled => "Connecting…".to_owned(),
        (None, None) => "Off.".to_owned(),
    };
    let connected = t.me.is_some();
    drop(t);
    rsx! {
        h2 { style: "margin: 12px 0 0; font-size: 18px; letter-spacing: -0.01em", "7pace time tracking" }
        div { class: "list", style: "padding: 4px 20px",
            div { class: "row",
                div {
                    strong { "Track time on the kanban" }
                    p { "Start/stop timer, add time and your time entries on each card. {status}" }
                }
                input {
                    r#type: "checkbox",
                    style: "width: 18px; height: 18px; accent-color: var(--accent)",
                    checked: enabled,
                    onchange: move |e| {
                        st.settings.write().time_tracking.enabled = e.checked();
                        services::save_settings(st);
                        time_tracking::connect(st);
                    },
                }
            }
            div { class: "row",
                div {
                    strong { "7pace API token" }
                    p { "Create one in 7pace Timetracker → Settings → Reporting & API. It is not your Azure DevOps PAT. Stored in your OS keyring." }
                    if let Some(m) = message() {
                        {
                            let (ok, text) = match m { Ok(s) => (true, s), Err(s) => (false, s) };
                            rsx! { p { style: if ok { "color: #A6E6BE" } else { "color: var(--danger)" }, "{text}" } }
                        }
                    }
                }
                form {
                    style: "display: flex; gap: 8px; flex: none",
                    onsubmit: move |e| {
                        e.prevent_default();
                        let value = token();
                        saving.set(true);
                        message.set(None);
                        spawn(async move {
                            let result = time_tracking::save_token(st, value).await;
                            saving.set(false);
                            if result.is_ok() {
                                token.set(String::new());
                            }
                            message.set(Some(result.map(|me| format!("Saved. Connected as {me}."))));
                        });
                    },
                    input {
                        class: "field",
                        r#type: "password",
                        style: "width: 220px",
                        placeholder: if connected { "Paste a new token" } else { "Paste the token" },
                        autocomplete: "off",
                        value: "{token}",
                        oninput: move |e| token.set(e.value()),
                    }
                    button { class: "primary", r#type: "submit", disabled: saving() || token().trim().is_empty(),
                        if saving() { span { class: "spinner" } }
                        "Save"
                    }
                    button {
                        class: "ghost",
                        r#type: "button",
                        disabled: saving(),
                        onclick: move |_| {
                            time_tracking::remove_token(st);
                            message.set(Some(Ok("Token removed.".into())));
                        },
                        "Remove"
                    }
                }
            }
        }
    }
}
