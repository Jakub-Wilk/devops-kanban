//! Settings editor (see docs/settings.md).

use dioxus::prelude::*;

use crate::components::icons;
use crate::components::widgets::Icon;
use crate::routes::Route;
use crate::services;
use crate::state::use_app;

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
                }
            }
        }
    }
}
