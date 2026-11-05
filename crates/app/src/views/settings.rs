//! Settings editor (see docs/settings.md), including the 7pace time tracking token.

use dioxus::prelude::*;

use crate::components::icons;
use crate::components::widgets::{Icon, Prio, StatePill, TypeIcon};
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
                            div { strong { "One-off tag" } p { "Tag for one-off items. When set, new and edited items get a checkbox next to Tags that adds it and leaves the parent empty, the only way to save an item without a parent. Empty turns it off; the default is “Ad-Hoc”." } }
                            input {
                                class: "field",
                                r#type: "text",
                                style: "width: 200px",
                                placeholder: "Off",
                                value: "{s.one_off_tag}",
                                onchange: move |e| {
                                    let v = e.value().replace(';', " ").trim().to_owned();
                                    st.settings.write().one_off_tag = v;
                                    services::save_settings(st);
                                },
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
                    AppearanceSection {}
                    TimeTrackingSettings {}
                }
            }
        }
    }
}

/// List row and kanban card style, each with a preview built from sample items, so it works
/// before any board is loaded.
#[component]
fn AppearanceSection() -> Element {
    let mut st = use_app();
    let a = st.settings.read().appearance.clone();
    let mut set = move |f: &dyn Fn(&mut kanban_core::config::AppearanceSettings)| {
        f(&mut st.settings.write().appearance);
        services::save_settings(st);
    };
    let list = a.list.clone();
    let card_accent = a.kanban.accent;
    // (id, depth, type, ADO icon id, type colour, priority, title, state, state colour, owner)
    let rows = [
        (
            104812,
            0,
            "Epic",
            "icon_crown",
            "FF7B00",
            1,
            "Print pipeline rewrite",
            "Active",
            "007acc",
            "JW",
        ),
        (
            104830,
            2,
            "User Story",
            "icon_book",
            "009CCC",
            1,
            "Render regional issue variants in parallel",
            "Active",
            "007acc",
            "MK",
        ),
        (
            104901,
            3,
            "Task",
            "icon_clipboard",
            "F2CB1D",
            2,
            "Split the renderer queue per region",
            "Active",
            "007acc",
            "MK",
        ),
    ];
    let cards = [
        (
            104830,
            1,
            "Render regional issue variants in parallel",
            "007acc",
            "12 Nov",
            false,
        ),
        (
            104841,
            2,
            "Retry failed PDF exports with backoff",
            "cc293d",
            "3 Nov · 2 days late",
            true,
        ),
    ];
    let row_class = format!(
        "ap-row{} {}",
        if list.tint { " tint" } else { "" },
        list.accent.class()
    );
    let card_class = format!("card {}", card_accent.class());
    rsx! {
        h2 { style: "margin: 12px 0 0; font-size: 18px; letter-spacing: -0.01em", "Appearance" }
        div { class: "list", style: "padding: 4px 20px",
            h3 { class: "ap-sub", "List rows" }
            div { class: "row",
                div { strong { "Background tint" } p { "Lean each row's background towards its work item type colour." } }
                input {
                    r#type: "checkbox",
                    style: "width: 18px; height: 18px; accent-color: var(--accent)",
                    aria_label: "Background tint",
                    checked: list.tint,
                    onchange: move |e| set(&|a| a.list.tint = e.checked()),
                }
            }
            div { class: "row",
                div { strong { "Accent bar" } p { "A 4 px bar in the type colour on one edge of each row." } }
                AccentPicker { label: "List accent bar", value: list.accent, on_pick: move |v| set(&|a| a.list.accent = v) }
            }
            div { class: "row",
                div { strong { "Type icon" } p { "The work item type's icon from Azure DevOps, before the title." } }
                input {
                    r#type: "checkbox",
                    style: "width: 18px; height: 18px; accent-color: var(--accent)",
                    aria_label: "Type icon",
                    checked: list.icon,
                    onchange: move |e| set(&|a| a.list.icon = e.checked()),
                }
            }
            div { class: "ap-prev",
                span { class: "cap", "Preview" }
                div { class: "ap-box", aria_hidden: "true",
                    for (id, depth, ty, icon, tc, prio, title, state, sc, who) in rows {
                        div { key: "{id}", class: "{row_class}", style: "--tc: #{tc}",
                            span { class: "id", "{id}" }
                            Prio { priority: Some(prio) }
                            span { class: "ttl", style: "padding-left: {depth * 24}px",
                                if list.icon { TypeIcon { icon: Some(icon.to_owned()), color: tc.to_owned(), label: ty.to_owned() } }
                                span { "{title}" }
                            }
                            StatePill { state: state.to_owned(), category: kanban_core::StateCategory::InProgress, color: sc.to_owned() }
                            span { class: "av", "{who}" }
                        }
                    }
                }
            }
            h3 { class: "ap-sub sep", "Kanban cards" }
            div { class: "row",
                div { strong { "Accent bar" } p { "A 4 px bar in the card's state colour on one edge of the card." } }
                AccentPicker { label: "Kanban accent bar", value: card_accent, on_pick: move |v| set(&|a| a.kanban.accent = v) }
            }
            div { class: "ap-prev",
                span { class: "cap", "Preview" }
                div { class: "ap-cards", aria_hidden: "true",
                    for (id, prio, title, sc, date, late) in cards {
                        article { key: "{id}", class: "{card_class}", style: "--sc: #{sc}",
                            div { class: "top", span { span { class: "id", "{id}" } Prio { priority: Some(prio) } } }
                            span { class: "etitle sm", style: "min-height: 0", "{title}" }
                            div { class: "bottom",
                                span { class: "av", "JW" }
                                span { class: if late { "date late" } else { "date" }, Icon { svg: icons::CALENDAR } "{date}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AccentPicker(
    label: &'static str,
    value: kanban_core::config::AccentSide,
    on_pick: EventHandler<kanban_core::config::AccentSide>,
) -> Element {
    rsx! {
        div { class: "seg", role: "group", aria_label: "{label}",
            for side in kanban_core::config::AccentSide::ALL {
                button {
                    key: "{side.label()}",
                    class: if side == value { "on" } else { "" },
                    r#type: "button",
                    aria_pressed: "{side == value}",
                    onclick: move |_| on_pick.call(side),
                    "{side.label()}"
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
                            rsx! { p { class: if ok { "ok" } else { "bad" }, "{text}" } }
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
