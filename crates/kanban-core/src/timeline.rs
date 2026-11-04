//! Timeline layout: items on a week grid by target date.
//!
//! Weeks start on Monday and are [`WEEK_PX`] wide. The range covers every dated item plus today,
//! with two weeks of padding after the latest date and at least two before today. Items are
//! sorted earliest target first; undated items come last.

use serde::{Deserialize, Serialize};
use time::{Date, Duration, Month};

use crate::card::CardId;

pub const WEEK_PX: f64 = 96.0;
const DAY_PX: f64 = WEEK_PX / 7.0;

/// Input row.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineItem {
    pub id: CardId,
    pub target: Option<Date>,
    /// Not Resolved/Completed.
    pub open: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub label: String,
    pub left: f64,
    pub width: f64,
    /// Contains today.
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RowLayout {
    pub id: CardId,
    /// Centre of the target marker.
    pub marker_x: Option<f64>,
    /// Bar between today and the target, for open items.
    pub bar: Option<(f64, f64)>,
    pub overdue: bool,
    pub open: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineLayout {
    pub start: Date,
    pub width: f64,
    pub today_x: f64,
    pub months: Vec<Span>,
    pub weeks: Vec<Span>,
    pub rows: Vec<RowLayout>,
}

impl TimelineLayout {
    pub fn build(today: Date, items: &[TimelineItem]) -> Self {
        let dated: Vec<Date> = items.iter().filter_map(|i| i.target).collect();
        let earliest = dated
            .iter()
            .copied()
            .chain([today - Duration::weeks(2)])
            .min()
            .unwrap_or(today);
        let latest = dated.iter().copied().chain([today]).max().unwrap_or(today);
        let start = earliest - Duration::days(earliest.weekday().number_days_from_monday().into());
        let n_weeks = ((latest + Duration::weeks(2) - start).whole_days() as f64 / 7.0)
            .ceil()
            .max(1.0) as i64;
        let end = start + Duration::weeks(n_weeks);
        let width = n_weeks as f64 * WEEK_PX;
        let x = |d: Date| (d - start).whole_days() as f64 * DAY_PX;

        let weeks = (0..n_weeks)
            .map(|i| {
                let ws = start + Duration::weeks(i);
                Span {
                    label: format!("{} {}", ws.day(), short_month(ws.month())),
                    left: i as f64 * WEEK_PX,
                    width: WEEK_PX,
                    current: today >= ws && today < ws + Duration::weeks(1),
                }
            })
            .collect();

        let mut months = Vec::new();
        let mut m = Date::from_calendar_date(start.year(), start.month(), 1).unwrap_or(start);
        while m < end {
            let next = next_month(m);
            let left = x(m).max(0.0);
            let right = x(next).min(width);
            months.push(Span {
                label: format!("{} {}", short_month(m.month()), m.year()),
                left,
                width: right - left,
                current: today >= m && today < next,
            });
            m = next;
        }

        let mut sorted: Vec<&TimelineItem> = items.iter().collect();
        sorted.sort_by(|a, b| match (a.target, b.target) {
            (Some(x), Some(y)) => x.cmp(&y).then(a.id.cmp(&b.id)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.id.cmp(&b.id),
        });
        let today_x = x(today) + DAY_PX / 2.0;
        let rows = sorted
            .into_iter()
            .map(|i| {
                let marker_x = i.target.map(|d| x(d) + DAY_PX / 2.0);
                let overdue = i.open && i.target.is_some_and(|d| d < today);
                let bar = match marker_x {
                    Some(mx) if i.open && i.target != Some(today) => {
                        Some((mx.min(today_x), (mx - today_x).abs()))
                    }
                    _ => None,
                };
                RowLayout {
                    id: i.id,
                    marker_x,
                    bar,
                    overdue,
                    open: i.open,
                }
            })
            .collect();

        Self {
            start,
            width,
            today_x,
            months,
            weeks,
            rows,
        }
    }

    /// The date under horizontal offset `x` (for dragging a marker), clamped to the range.
    pub fn date_at(&self, x: f64) -> Date {
        let max_days = (self.width / DAY_PX).floor() as i64 - 1;
        let days = ((x / DAY_PX).floor() as i64).clamp(0, max_days.max(0));
        self.start + Duration::days(days)
    }

    /// Centre x of `date`.
    pub fn x_of(&self, date: Date) -> f64 {
        (date - self.start).whole_days() as f64 * DAY_PX + DAY_PX / 2.0
    }
}

pub fn short_month(m: Month) -> &'static str {
    match m {
        Month::January => "Jan",
        Month::February => "Feb",
        Month::March => "Mar",
        Month::April => "Apr",
        Month::May => "May",
        Month::June => "Jun",
        Month::July => "Jul",
        Month::August => "Aug",
        Month::September => "Sep",
        Month::October => "Oct",
        Month::November => "Nov",
        Month::December => "Dec",
    }
}

/// `20 Nov 2026`.
pub fn format_long(d: Date) -> String {
    format!("{} {} {}", d.day(), short_month(d.month()), d.year())
}

fn next_month(d: Date) -> Date {
    let (y, m) = if d.month() == Month::December {
        (d.year() + 1, Month::January)
    } else {
        (d.year(), d.month().next())
    };
    Date::from_calendar_date(y, m, 1).unwrap_or(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    fn item(id: CardId, target: Option<Date>, open: bool) -> TimelineItem {
        TimelineItem { id, target, open }
    }

    #[test]
    fn starts_on_monday_and_sorts_undated_last() {
        let today = date!(2026 - 11 - 04); // Wednesday
        let l = TimelineLayout::build(
            today,
            &[
                item(1, None, true),
                item(2, Some(date!(2026 - 12 - 11)), true),
                item(3, Some(date!(2026 - 10 - 16)), false),
            ],
        );
        assert_eq!(l.start.weekday(), time::Weekday::Monday);
        assert!(l.start <= date!(2026 - 10 - 16));
        assert_eq!(
            l.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
        assert!(l.rows[2].marker_x.is_none());
        assert!(l.weeks.iter().filter(|w| w.current).count() == 1);
        assert!(l.width >= l.x_of(date!(2026 - 12 - 11)) + WEEK_PX);
    }

    #[test]
    fn overdue_only_when_open_and_past() {
        let today = date!(2026 - 11 - 04);
        let l = TimelineLayout::build(
            today,
            &[
                item(1, Some(date!(2026 - 11 - 01)), true),
                item(2, Some(date!(2026 - 11 - 01)), false),
            ],
        );
        let r1 = l.rows.iter().find(|r| r.id == 1).unwrap();
        let r2 = l.rows.iter().find(|r| r.id == 2).unwrap();
        assert!(r1.overdue && r1.bar.is_some());
        assert!(!r2.overdue && r2.bar.is_none());
    }

    #[test]
    fn date_at_round_trips_x_of() {
        let l = TimelineLayout::build(
            date!(2026 - 11 - 04),
            &[item(1, Some(date!(2027 - 01 - 29)), true)],
        );
        for d in [
            date!(2026 - 11 - 04),
            date!(2026 - 12 - 25),
            date!(2027 - 01 - 29),
        ] {
            assert_eq!(l.date_at(l.x_of(d)), d);
        }
        assert_eq!(l.date_at(-500.0), l.start);
    }

    #[test]
    fn months_cover_the_track() {
        let l = TimelineLayout::build(
            date!(2026 - 11 - 04),
            &[item(1, Some(date!(2027 - 03 - 26)), true)],
        );
        let total: f64 = l.months.iter().map(|m| m.width).sum();
        assert!((total - l.width).abs() < 0.01);
        assert_eq!(l.months.first().unwrap().label, "Oct 2026");
    }
}
