//! Timeline layout: items on an unbounded week grid by target date.
//!
//! Weeks start on Monday and are [`WEEK_PX`] wide; x = 0 is the Monday of today's week and the
//! axis extends without limit both ways, so the view can pan anywhere. [`TimelineLayout::spans`]
//! builds the month and week headers for just the visible window. Items are sorted earliest
//! target first; undated items come last.

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
    /// x = 0: Monday of today's week.
    pub origin: Date,
    pub today_x: f64,
    pub rows: Vec<RowLayout>,
}

impl TimelineLayout {
    pub fn build(today: Date, items: &[TimelineItem]) -> Self {
        let origin = today - Duration::days(today.weekday().number_days_from_monday().into());
        let x = |d: Date| (d - origin).whole_days() as f64 * DAY_PX;

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
            origin,
            today_x,
            rows,
        }
    }

    /// The date under horizontal offset `x` (any x, past or future).
    pub fn date_at(&self, x: f64) -> Date {
        self.origin + Duration::days((x / DAY_PX).floor() as i64)
    }

    /// Centre x of `date`.
    pub fn x_of(&self, date: Date) -> f64 {
        (date - self.origin).whole_days() as f64 * DAY_PX + DAY_PX / 2.0
    }

    /// Months (clipped to the window, so their labels stay visible) and weeks overlapping
    /// `[x0, x1)`. `current` marks the ones containing today.
    pub fn spans(&self, x0: f64, x1: f64) -> (Vec<Span>, Vec<Span>) {
        let today = self.date_at(self.today_x);
        let left = |d: Date| (d - self.origin).whole_days() as f64 * DAY_PX;

        let weeks = ((x0 / WEEK_PX).floor() as i64..(x1 / WEEK_PX).ceil() as i64)
            .map(|i| {
                let ws = self.origin + Duration::weeks(i);
                Span {
                    label: format!("{} {}", ws.day(), short_month(ws.month())),
                    left: i as f64 * WEEK_PX,
                    width: WEEK_PX,
                    current: today >= ws && today < ws + Duration::weeks(1),
                }
            })
            .collect();

        let mut months = Vec::new();
        let first = self.date_at(x0);
        let mut m = Date::from_calendar_date(first.year(), first.month(), 1).unwrap_or(first);
        while left(m) < x1 {
            let next = next_month(m);
            let (l, r) = (left(m).max(x0), left(next).min(x1));
            months.push(Span {
                label: format!("{} {}", short_month(m.month()), m.year()),
                left: l,
                width: r - l,
                current: today >= m && today < next,
            });
            m = next;
        }
        (months, weeks)
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
    fn origin_is_monday_and_undated_sort_last() {
        let today = date!(2026 - 11 - 04); // Wednesday
        let l = TimelineLayout::build(
            today,
            &[
                item(1, None, true),
                item(2, Some(date!(2026 - 12 - 11)), true),
                item(3, Some(date!(2026 - 10 - 16)), false),
            ],
        );
        assert_eq!(l.origin, date!(2026 - 11 - 02));
        assert_eq!(
            l.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
        assert!(l.rows[2].marker_x.is_none());
        assert!(l.rows[0].marker_x.unwrap() < 0.0); // before the origin
        assert_eq!(l.date_at(l.today_x), today);
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
    fn date_at_round_trips_x_of_in_past_and_future() {
        let l = TimelineLayout::build(date!(2026 - 11 - 04), &[]);
        for d in [
            date!(2019 - 02 - 28),
            date!(2026 - 11 - 01),
            date!(2026 - 11 - 04),
            date!(2031 - 07 - 15),
        ] {
            assert_eq!(l.date_at(l.x_of(d)), d);
        }
    }

    #[test]
    fn spans_cover_the_window_across_a_year_boundary() {
        let l = TimelineLayout::build(date!(2026 - 11 - 04), &[]);
        let (x0, x1) = (l.x_of(date!(2026 - 12 - 10)), l.x_of(date!(2027 - 02 - 03)));
        let (months, weeks) = l.spans(x0, x1);
        let total: f64 = months.iter().map(|m| m.width).sum();
        assert!((total - (x1 - x0)).abs() < 0.01);
        assert_eq!(
            months.iter().map(|m| m.label.as_str()).collect::<Vec<_>>(),
            ["Dec 2026", "Jan 2027", "Feb 2027"]
        );
        assert!(weeks.first().unwrap().left <= x0 && weeks.last().unwrap().left + WEEK_PX >= x1);
        assert!(weeks.iter().all(|w| !w.current));
        let (_, now) = l.spans(0.0, WEEK_PX);
        assert!(now[0].current);
    }
}
