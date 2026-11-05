//! Pickers shared by the item and time dialogs: a month calendar in a popover, and date labels.

use dioxus::prelude::*;
use kanban_core::timeline::short_month;
use time::{Date, Month, Weekday};

use crate::components::icons;
use crate::components::widgets::{Icon, today};

const WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

fn weekday_short(d: Weekday) -> &'static str {
    match d {
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
        Weekday::Sunday => "Sun",
    }
}

/// `Thu 5 Nov 2026`.
pub fn long_date(d: Date) -> String {
    format!(
        "{} {} {} {}",
        weekday_short(d.weekday()),
        d.day(),
        short_month(d.month()),
        d.year()
    )
}

/// `Thu 5 Nov`.
pub fn day_label(d: Date) -> String {
    format!(
        "{} {} {}",
        weekday_short(d.weekday()),
        d.day(),
        short_month(d.month())
    )
}

/// `Today`, `Yesterday`, `3 days ago`, `in 2 days`.
pub fn relative_day(d: Date) -> String {
    match (today() - d).whole_days() {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        -1 => "Tomorrow".into(),
        n if n > 0 => format!("{n} days ago"),
        n => format!("in {} days", -n),
    }
}

fn month_name(m: Month) -> &'static str {
    match m {
        Month::January => "January",
        Month::February => "February",
        Month::March => "March",
        Month::April => "April",
        Month::May => "May",
        Month::June => "June",
        Month::July => "July",
        Month::August => "August",
        Month::September => "September",
        Month::October => "October",
        Month::November => "November",
        Month::December => "December",
    }
}

/// A month grid. Days after `max` can't be picked.
#[component]
pub fn Calendar(value: Option<Date>, max: Option<Date>, onpick: EventHandler<Date>) -> Element {
    let shown = value.unwrap_or_else(today);
    let mut month = use_signal(|| (shown.year(), shown.month()));
    let (year, m) = month();
    let Ok(first) = Date::from_calendar_date(year, m, 1) else {
        return rsx! {};
    };
    let blanks = first.weekday().number_days_from_monday();
    let days = m.length(year);
    let prev = if m == Month::January {
        (year - 1, Month::December)
    } else {
        (year, m.previous())
    };
    let next = if m == Month::December {
        (year + 1, Month::January)
    } else {
        (year, m.next())
    };
    let next_first = Date::from_calendar_date(next.0, next.1, 1).ok();
    let next_disabled = max.zip(next_first).is_some_and(|(mx, n)| n > mx);
    let now = today();
    rsx! {
        div { class: "cal", role: "dialog", aria_label: "Pick a date",
            div { class: "cal-h",
                button { r#type: "button", aria_label: "Previous month", onclick: move |_| month.set(prev), Icon { svg: icons::BACK } }
                "{month_name(m)} {year}"
                button { r#type: "button", aria_label: "Next month", disabled: next_disabled, onclick: move |_| month.set(next), Icon { svg: icons::CHEVRON_RIGHT } }
            }
            div { class: "cal-g",
                for d in WEEKDAYS { span { key: "{d}", class: "dow", "{d}" } }
                for i in 0..blanks { span { key: "b{i}" } }
                for n in 1..=days {
                    {
                        let date = Date::from_calendar_date(year, m, n).unwrap_or(first);
                        let on = value == Some(date);
                        let mut class = String::new();
                        if on { class.push_str("on"); }
                        if date == now { class.push_str(" today"); }
                        rsx! {
                            button {
                                key: "{n}",
                                class: "{class}",
                                r#type: "button",
                                aria_label: "{long_date(date)}",
                                aria_pressed: "{on}",
                                disabled: max.is_some_and(|mx| date > mx),
                                onclick: move |_| onpick.call(date),
                                "{n}"
                            }
                        }
                    }
                }
            }
        }
    }
}
