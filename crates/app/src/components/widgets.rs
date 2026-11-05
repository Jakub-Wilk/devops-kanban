//! Small building blocks shared by the tabs: icons, state pills, priority badges, work item type
//! icons, avatars (ADO profile pictures over initials), the "assign to me" disc and dates.

use dioxus::prelude::*;
use kanban_core::card::Person;
use kanban_core::{CardId, StateCategory};
use time::Date;

use crate::components::avatars::avatar_src;
use crate::components::icons;
use crate::services;
use crate::state::use_app;

#[component]
pub fn Icon(svg: &'static str) -> Element {
    rsx! {
        span { class: "ico", dangerous_inner_html: svg }
    }
}

/// CSS-friendly slug of a state name (`TEST-FAILED` → `test-failed`).
pub fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// A state with its ADO `color` (hex without `#`).
#[component]
pub fn StatePill(state: String, category: StateCategory, color: String) -> Element {
    let class = format!("pill c-{} s-{}", category.as_str(), slug(&state));
    rsx! {
        span { class: "{class}", style: "--sc: #{color}", i {} "{state}" }
    }
}

/// Priority as four bars: P1 lights all four, P4 one; no priority shows four outlines. The
/// number stays in the tooltip and the accessible name.
#[component]
pub fn Prio(priority: Option<u8>) -> Element {
    let p = priority.unwrap_or(0).min(4);
    let title = if p == 0 {
        "No priority".to_owned()
    } else {
        format!("Priority {p}")
    };
    rsx! {
        span { class: "prio p{p}", role: "img", aria_label: "{title}", "data-tip": "{title}",
            b {} b {} b {} b {}
        }
    }
}

#[component]
pub fn Avatar(
    person: Person,
    #[props(default)] large: bool,
    #[props(default)] small: bool,
) -> Element {
    // The descriptor whose image failed to load.
    let mut failed = use_signal(|| None::<String>);
    let initials = person.initials();
    let class = if large {
        "av lg"
    } else if small {
        "av sm"
    } else {
        "av"
    };
    let src = person
        .descriptor
        .clone()
        .filter(|d| failed.read().as_ref() != Some(d));
    rsx! {
        span { class: "{class}", "data-tip": "{person.display_name}", aria_label: "Assigned to {person.display_name}",
            "{initials}"
            if let Some(d) = src {
                img {
                    src: avatar_src(&d),
                    alt: "",
                    draggable: "false",
                    onerror: move |_| failed.set(Some(d.clone())),
                }
            }
        }
    }
}

/// Avatar when assigned, otherwise a dashed disc that assigns the item to the signed-in user.
#[component]
pub fn Assignee(id: CardId, person: Option<Person>, title: String) -> Element {
    let st = use_app();
    match person {
        Some(p) => rsx! { Avatar { person: p } },
        None => {
            rsx! {
                button {
                    class: "assign",
                    r#type: "button",
                    "data-tip": "Assign to me",
                    aria_label: "Assign {title} to me",
                    draggable: "false",
                    onclick: move |e| {
                        e.stop_propagation();
                        services::assign_to_me(st, id);
                    },
                    span { class: "disc", Icon { svg: icons::ASSIGN } }
                }
            }
        }
    }
}

pub fn today() -> Date {
    time::OffsetDateTime::now_local()
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc())
        .date()
}

/// `20 Nov` this year, `20 Nov 2027` otherwise.
pub fn short_date(d: Date) -> String {
    let month = kanban_core::timeline::short_month(d.month());
    if d.year() == today().year() {
        format!("{} {month}", d.day())
    } else {
        format!("{} {month} {}", d.day(), d.year())
    }
}

/// Target date. When it is past and the item is still open, it turns amber and says how late
/// it is ("2 days late"), so the warning doesn't rely on colour.
#[component]
pub fn DateLabel(date: Option<Date>, #[props(default)] open: bool) -> Element {
    let Some(d) = date else {
        return rsx! {};
    };
    let days_late = (today() - d).whole_days();
    let late = open && days_late > 0;
    let class = if late { "date late" } else { "date" };
    let text = short_date(d);
    let late_text = match days_late {
        1 => "1 day late".to_owned(),
        n => format!("{n} days late"),
    };
    rsx! {
        span { class: "{class}", "data-tip": if late { "Overdue" } else { "Target date" },
            Icon { svg: icons::CALENDAR }
            span { class: "sr-only", "Target date: " }
            "{text}"
            if late { span { class: "late-by", "· {late_text}" } }
        }
    }
}

/// A work item type's icon. `src` is Azure DevOps's own icon in the type colour (a `data:` URI,
/// shown as an image so its SVG can't run anything); without it the app draws `icon` (the ADO
/// icon id, `icon_book`, …) itself, and unknown ids show a coloured dot. `color` is the type
/// colour without `#`.
#[component]
pub fn TypeIcon(
    icon: Option<String>,
    src: Option<String>,
    color: String,
    label: String,
) -> Element {
    if let Some(src) = src {
        return rsx! {
            span { class: "ty ado", role: "img", aria_label: "{label}", "data-tip": "{label}",
                img { src: "{src}", alt: "", draggable: "false" }
            }
        };
    }
    let svg = icon.as_deref().and_then(icons::type_icon);
    let class = if svg.is_some() { "ty" } else { "ty dot" };
    rsx! {
        span { class: "{class}", style: "--tc: #{color}", role: "img", aria_label: "{label}", "data-tip": "{label}",
            if let Some(svg) = svg { span { class: "ico", dangerous_inner_html: svg } }
        }
    }
}
