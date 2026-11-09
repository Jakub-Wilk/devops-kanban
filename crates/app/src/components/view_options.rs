//! "View options": a button in the top bar, right after the view tabs (the same place on every
//! tab), opening a popover with the current view's appearance settings (`appearance` in
//! `docs/settings.md`): row style for the List and the Timeline, card style for the Kanban.
//! Changes apply to the board behind the popover at once, so it needs no preview.

use dioxus::prelude::*;
use kanban_core::config::{AccentSide, AppearanceSettings};

use crate::components::icons;
use crate::components::widgets::Icon;
use crate::services;
use crate::state::use_app;

/// Which view's options to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTab {
    List,
    Kanban,
    Timeline,
}

#[component]
pub fn ViewOptions(tab: ViewTab) -> Element {
    let mut open = use_signal(|| false);
    // Closing can follow a change that needs the kanban's hover tracking (re)attached.
    let mut close = move || {
        open.set(false);
        crate::components::kanban::attach_hover_tracking();
    };
    rsx! {
        span { class: "popover-anchor",
            button {
                class: if open() { "ghost viewbtn on" } else { "ghost viewbtn" },
                r#type: "button",
                aria_label: "View options",
                aria_haspopup: "dialog",
                aria_expanded: "{open()}",
                onclick: move |_| if open() { close() } else { open.set(true) },
                Icon { svg: icons::SLIDERS }
                "View options"
            }
            if open() {
                div { class: "popover-backdrop", onclick: move |_| close() }
                div {
                    class: "popover left viewopts",
                    role: "dialog",
                    aria_label: "View options",
                    onkeydown: move |e| if e.key() == Key::Escape { close() },
                    match tab {
                        ViewTab::List => rsx! { RowOptions { timeline: false } },
                        ViewTab::Timeline => rsx! { RowOptions { timeline: true } },
                        ViewTab::Kanban => rsx! { KanbanOptions {} },
                    }
                }
            }
        }
    }
}

/// Writes one appearance change and saves the settings.
fn use_set_appearance() -> impl FnMut(&dyn Fn(&mut AppearanceSettings)) + Copy {
    let mut st = use_app();
    move |f: &dyn Fn(&mut AppearanceSettings)| {
        f(&mut st.settings.write().appearance);
        services::save_settings(st);
    }
}

#[component]
fn Toggle(
    label: &'static str,
    help: &'static str,
    on: bool,
    onchange: EventHandler<bool>,
) -> Element {
    rsx! {
        label { class: "vo-row",
            span { class: "vo-text", strong { "{label}" } small { "{help}" } }
            input {
                r#type: "checkbox",
                class: "vo-check",
                checked: on,
                onchange: move |e| onchange.call(e.checked()),
            }
        }
    }
}

#[component]
fn AccentPicker(
    label: &'static str,
    help: &'static str,
    value: AccentSide,
    on_pick: EventHandler<AccentSide>,
) -> Element {
    rsx! {
        div { class: "vo-row",
            span { class: "vo-text", strong { "{label}" } small { "{help}" } }
            div { class: "seg sm", role: "group", aria_label: "{label}",
                for side in AccentSide::ALL {
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
}

/// List and Timeline rows share the options (each view keeps its own values).
#[component]
fn RowOptions(timeline: bool) -> Element {
    let st = use_app();
    let mut set = use_set_appearance();
    let style = {
        let a = &st.settings.read().appearance;
        if timeline {
            a.timeline.clone()
        } else {
            a.list.clone()
        }
    };
    rsx! {
        h3 { class: "vo-title", if timeline { "Timeline rows" } else { "List rows" } }
        Toggle {
            label: "Background tint",
            help: "Lean each row towards its work item type's colour.",
            on: style.tint,
            onchange: move |v| set(&move |a| rows(a, timeline).tint = v),
        }
        AccentPicker {
            label: "Accent bar",
            help: "A bar in the type's colour on one edge of each row.",
            value: style.accent,
            on_pick: move |v| set(&move |a| rows(a, timeline).accent = v),
        }
        Toggle {
            label: "Type icon",
            help: "The type's icon from Azure DevOps, before the title.",
            on: style.icon,
            onchange: move |v| set(&move |a| rows(a, timeline).icon = v),
        }
    }
}

fn rows(a: &mut AppearanceSettings, timeline: bool) -> &mut kanban_core::config::ListStyle {
    if timeline {
        &mut a.timeline
    } else {
        &mut a.list
    }
}

#[component]
fn KanbanOptions() -> Element {
    let st = use_app();
    let mut set = use_set_appearance();
    let k = st.settings.read().appearance.kanban.clone();
    rsx! {
        h3 { class: "vo-title", "Kanban cards" }
        Toggle {
            label: "Background tint",
            help: "Lean each card towards its status colour.",
            on: k.tint,
            onchange: move |v| set(&move |a| a.kanban.tint = v),
        }
        AccentPicker {
            label: "Accent bar",
            help: "A bar in the status colour on one edge of each card.",
            value: k.accent,
            on_pick: move |v| set(&move |a| a.kanban.accent = v),
        }
        Toggle {
            label: "Widen the hovered column",
            help: "The column under the mouse widens, the others narrow. The done column doesn't.",
            on: k.hover_widen,
            onchange: move |v| set(&move |a| a.kanban.hover_widen = v),
        }
        Toggle {
            label: "Open cards on hover",
            help: "Cards open under the mouse; a pin keeps one open at the top of its column.",
            on: k.hover_open,
            onchange: move |v| set(&move |a| a.kanban.hover_open = v),
        }
    }
}
