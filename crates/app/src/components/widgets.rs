//! Small building blocks shared by the tabs: icons, state pills, priority badges, avatars (ADO
//! profile pictures over initials), the "assign to me" disc and dates.

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

#[component]
pub fn Prio(priority: Option<u8>) -> Element {
    let p = priority.unwrap_or(0);
    let label = if p == 0 {
        "–".to_owned()
    } else {
        format!("P{p}")
    };
    let title = if p == 0 {
        "No priority".to_owned()
    } else {
        format!("Priority {p}")
    };
    rsx! {
        span { class: "prio p{p}", "data-tip": "{title}", "{label}" }
    }
}

/// The person's ADO profile picture over their initials, which stay when there is no picture.
#[component]
pub fn Avatar(person: Person, #[props(default)] large: bool) -> Element {
    // The descriptor whose image failed to load.
    let mut failed = use_signal(|| None::<String>);
    let initials = person.initials();
    let class = if large { "av lg" } else { "av" };
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

#[component]
pub fn DateLabel(date: Option<Date>, #[props(default)] open: bool) -> Element {
    let Some(d) = date else {
        return rsx! {};
    };
    let late = open && d < today();
    let class = if late { "date late" } else { "date" };
    let text = short_date(d);
    rsx! {
        span { class: "{class}", "data-tip": if late { "Overdue" } else { "Target date" },
            Icon { svg: icons::CALENDAR }
            span { class: "sr-only", "Target date: " }
            "{text}"
        }
    }
}
