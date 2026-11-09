//! The Timetable tab's week of 7pace entries: which week a date is in, the hours the grid shows,
//! where entries overlap and how they share a day's column, and the rules that keep drags
//! (create, move, resize) from making new overlaps.
//!
//! Overlaps can still exist (7pace's own UI, imports, the desktop client), so they are detected
//! and laid out side by side for the user to fix, never hidden or rejected. All times are the
//! user's local wall-clock time, as [`TimeEntry::start`] is.

use std::collections::HashMap;

use time::{Date, Duration, PrimitiveDateTime, Time, Weekday};

use crate::time::TimeEntry;

/// Drags snap to this many minutes.
pub const SNAP_MINUTES: i64 = 15;
/// Shortest entry a resize or a drag-to-create leaves.
pub const MIN_SECS: i64 = SNAP_MINUTES * 60;
/// Hours the grid shows when every entry fits in them.
pub const DEFAULT_HOURS: (u8, u8) = (7, 19);

const DAY_SECS: i64 = 24 * 3600;

/// The Monday of `date`'s week.
pub fn week_start(date: Date) -> Date {
    date - Duration::days(date.weekday().number_days_from_monday().into())
}

/// Monday to Sunday of the week starting on `monday`.
pub fn week_days(monday: Date) -> [Date; 7] {
    std::array::from_fn(|i| monday + Duration::days(i as i64))
}

pub fn is_weekend(date: Date) -> bool {
    matches!(date.weekday(), Weekday::Saturday | Weekday::Sunday)
}

pub fn end_of(e: &TimeEntry) -> PrimitiveDateTime {
    e.start + Duration::seconds(e.seconds)
}

/// Seconds since `day`'s midnight (negative before it, over a day after it).
fn secs_into(day: Date, t: PrimitiveDateTime) -> i64 {
    (t - day.midnight()).whole_seconds()
}

fn at(day: Date, secs: i64) -> PrimitiveDateTime {
    day.midnight() + Duration::seconds(secs)
}

/// `secs` rounded to the nearest snap step.
fn snap(secs: i64) -> i64 {
    let step = SNAP_MINUTES * 60;
    (secs + step / 2).div_euclid(step) * step
}

/// The part of an entry that falls on one day, in seconds since that day's midnight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// The entry's worklog id.
    pub id: String,
    pub from: i64,
    pub to: i64,
}

/// The parts of `entries` on `day`, clipped to it, earliest first. An entry over midnight
/// appears on both days.
pub fn day_segments(entries: &[TimeEntry], day: Date) -> Vec<Segment> {
    let mut out: Vec<Segment> = entries
        .iter()
        .filter_map(|e| {
            let from = secs_into(day, e.start).max(0);
            let to = secs_into(day, end_of(e)).min(DAY_SECS);
            (to > from || (e.seconds == 0 && from == secs_into(day, e.start) && from < DAY_SECS))
                .then(|| Segment {
                    id: e.id.clone(),
                    from,
                    to: to.max(from),
                })
        })
        .collect();
    out.sort_by(|a, b| (a.from, a.to, &a.id).cmp(&(b.from, b.to, &b.id)));
    out
}

/// The hours the grid shows: `default`, widened to the earliest start and latest end of
/// `entries` on any of `days`.
pub fn hour_range(entries: &[TimeEntry], days: &[Date], default: (u8, u8)) -> (u8, u8) {
    let (mut lo, mut hi) = (i64::from(default.0) * 3600, i64::from(default.1) * 3600);
    for day in days {
        for s in day_segments(entries, *day) {
            lo = lo.min(s.from);
            hi = hi.max(s.to);
        }
    }
    let lo = (lo / 3600).clamp(0, 23);
    let hi = ((hi + 3599) / 3600).clamp(lo + 1, 24);
    (lo as u8, hi as u8)
}

/// Two entries logged over the same time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlap {
    pub a: String,
    pub b: String,
    pub from: PrimitiveDateTime,
    pub to: PrimitiveDateTime,
}

/// Every pair of `entries` that overlaps (one ending exactly when the next starts doesn't).
pub fn overlaps(entries: &[TimeEntry]) -> Vec<Overlap> {
    let mut sorted: Vec<&TimeEntry> = entries.iter().collect();
    sorted.sort_by_key(|e| (e.start, end_of(e)));
    let mut out = Vec::new();
    for (i, a) in sorted.iter().enumerate() {
        let a_end = end_of(a);
        for b in &sorted[i + 1..] {
            if b.start >= a_end {
                break;
            }
            let to = a_end.min(end_of(b));
            if to > b.start {
                out.push(Overlap {
                    a: a.id.clone(),
                    b: b.id.clone(),
                    from: b.start,
                    to,
                });
            }
        }
    }
    out
}

/// Where a segment sits in its day's column: `lane` of `lanes` side-by-side columns. Entries
/// that don't overlap anything get `(0, 1)`, the full width.
pub fn lanes(segments: &[Segment]) -> HashMap<String, (usize, usize)> {
    let mut out = HashMap::new();
    // Clusters of transitively overlapping segments share a lane count.
    let mut cluster: Vec<(&Segment, usize)> = Vec::new();
    let mut lane_ends: Vec<i64> = Vec::new();
    let mut cluster_end = i64::MIN;
    let flush = |cluster: &mut Vec<(&Segment, usize)>,
                 lane_ends: &mut Vec<i64>,
                 out: &mut HashMap<String, (usize, usize)>| {
        let n = lane_ends.len().max(1);
        for (s, lane) in cluster.drain(..) {
            out.insert(s.id.clone(), (lane, n));
        }
        lane_ends.clear();
    };
    for s in segments {
        if s.from >= cluster_end && !cluster.is_empty() {
            flush(&mut cluster, &mut lane_ends, &mut out);
        }
        let lane = match lane_ends.iter().position(|end| *end <= s.from) {
            Some(i) => {
                lane_ends[i] = s.to;
                i
            }
            None => {
                lane_ends.push(s.to);
                lane_ends.len() - 1
            }
        };
        cluster.push((s, lane));
        cluster_end = if cluster.len() == 1 {
            s.to
        } else {
            cluster_end.max(s.to)
        };
    }
    flush(&mut cluster, &mut lane_ends, &mut out);
    out
}

/// Busy spans on `day` (seconds since midnight), merged, leaving out the entry `skip`.
fn busy(others: &[TimeEntry], day: Date, skip: Option<&str>) -> Vec<(i64, i64)> {
    let mut spans: Vec<(i64, i64)> = day_segments(others, day)
        .into_iter()
        .filter(|s| Some(s.id.as_str()) != skip && s.to > s.from)
        .map(|s| (s.from, s.to))
        .collect();
    spans.sort();
    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (a, b) in spans {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    merged
}

/// Free spans on `day`, the complement of [`busy`].
fn gaps(busy: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let mut cursor = 0;
    for (a, b) in busy {
        if *a > cursor {
            out.push((cursor, *a));
        }
        cursor = cursor.max(*b);
    }
    if cursor < DAY_SECS {
        out.push((cursor, DAY_SECS));
    }
    out
}

/// Where `entry` lands when dragged to start at `wanted` (on `wanted`'s day): snapped to
/// [`SNAP_MINUTES`], then pushed to the nearest place on that day where it fits without
/// overlapping another entry (flush against a neighbour if need be). `None` when it fits nowhere
/// on that day.
pub fn clamp_move(
    entry: &TimeEntry,
    wanted: PrimitiveDateTime,
    entries: &[TimeEntry],
) -> Option<PrimitiveDateTime> {
    let day = wanted.date();
    let len = entry.seconds.max(1);
    if len > DAY_SECS {
        return None;
    }
    let want = snap(secs_into(day, wanted)).clamp(0, DAY_SECS - len);
    gaps(&busy(entries, day, Some(&entry.id)))
        .into_iter()
        .filter(|(a, b)| b - a >= len)
        .map(|(a, b)| want.clamp(a, b - len))
        .min_by_key(|s| (s - want).abs())
        .map(|s| at(day, s))
}

/// Which edge of an entry a resize drags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Start,
    End,
}

/// `entry`'s new start and length when its `edge` is dragged to `wanted`: snapped, at least
/// [`MIN_SECS`] long, within its day, and stopped at the neighbouring entry. An entry that
/// already overlaps its neighbour can shrink out of the overlap but not grow further into it.
pub fn clamp_resize(
    entry: &TimeEntry,
    edge: Edge,
    wanted: PrimitiveDateTime,
    entries: &[TimeEntry],
) -> (PrimitiveDateTime, i64) {
    let day = entry.start.date();
    let start = secs_into(day, entry.start);
    let end = start + entry.seconds;
    let others = busy(entries, day, Some(&entry.id));
    let want = snap(secs_into(day, wanted));
    match edge {
        Edge::End => {
            // The first other entry starting at or after this one's start.
            let next = others
                .iter()
                .filter(|(a, _)| *a >= start)
                .map(|(a, _)| *a)
                .min()
                .unwrap_or(DAY_SECS);
            let limit = if next < end { end } else { next }.min(DAY_SECS.max(end));
            let floor = (start + MIN_SECS).min(end);
            let new_end = want.clamp(floor, limit.max(floor));
            (entry.start, new_end - start)
        }
        Edge::Start => {
            // The last other entry ending at or before this one's end.
            let prev = others
                .iter()
                .filter(|(_, b)| *b <= end)
                .map(|(_, b)| *b)
                .max()
                .unwrap_or(0);
            let limit = if prev > start { start } else { prev };
            let ceil = (end - MIN_SECS).max(start);
            let new_start = want.clamp(limit.min(ceil), ceil);
            (at(day, new_start), end - new_start)
        }
    }
}

/// A drag-to-create selection on `day` from `anchor` to `current`: snapped, at least
/// [`MIN_SECS`], and clipped to the free gap the anchor is in. `None` when the anchor is inside
/// an entry or the gap is shorter than [`MIN_SECS`].
pub fn free_slot(
    day: Date,
    anchor: Time,
    current: Time,
    entries: &[TimeEntry],
) -> Option<(PrimitiveDateTime, i64)> {
    let secs = |t: Time| i64::from(t.hour()) * 3600 + i64::from(t.minute()) * 60;
    let a = secs(anchor);
    let (gap_a, gap_b) = gaps(&busy(entries, day, None))
        .into_iter()
        .find(|(ga, gb)| a >= *ga && a < *gb)?;
    if gap_b - gap_a < MIN_SECS {
        return None;
    }
    let step = SNAP_MINUTES * 60;
    let c = secs(current);
    let (mut from, mut to) = if c >= a {
        (a.div_euclid(step) * step, snap(c))
    } else {
        (snap(c), (a.div_euclid(step) + 1) * step)
    };
    if to - from < MIN_SECS {
        to = from + MIN_SECS;
    }
    from = from.max(gap_a);
    to = to.min(gap_b);
    if to - from < MIN_SECS {
        // Pushed against a neighbour: keep the minimum length inside the gap.
        if from == gap_a {
            to = (from + MIN_SECS).min(gap_b);
        } else {
            from = (to - MIN_SECS).max(gap_a);
        }
    }
    Some((at(day, from), to - from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{date, datetime, time};

    fn entry(id: &str, start: PrimitiveDateTime, minutes: i64) -> TimeEntry {
        TimeEntry {
            id: id.into(),
            work_item: Some(1),
            start,
            seconds: minutes * 60,
            comment: None,
            activity: None,
            can_edit: true,
            can_delete: true,
        }
    }

    #[test]
    fn weeks_start_on_monday() {
        assert_eq!(week_start(date!(2026 - 11 - 12)), date!(2026 - 11 - 09));
        assert_eq!(week_start(date!(2026 - 11 - 09)), date!(2026 - 11 - 09));
        assert_eq!(week_start(date!(2026 - 11 - 15)), date!(2026 - 11 - 09));
        assert_eq!(week_days(date!(2026 - 11 - 09))[6], date!(2026 - 11 - 15));
        assert!(is_weekend(date!(2026 - 11 - 14)) && !is_weekend(date!(2026 - 11 - 13)));
    }

    #[test]
    fn hours_widen_to_fit() {
        let days = week_days(date!(2026 - 11 - 09));
        let es = [entry("a", datetime!(2026-11-10 09:00), 60)];
        assert_eq!(hour_range(&es, &days, DEFAULT_HOURS), (7, 19));
        let es = [
            entry("a", datetime!(2026-11-10 06:30), 60),
            entry("b", datetime!(2026-11-11 19:15), 30),
        ];
        assert_eq!(hour_range(&es, &days, DEFAULT_HOURS), (6, 20));
        // Over midnight: the grid runs to the end of the day and from its start.
        let es = [entry("a", datetime!(2026-11-10 23:00), 120)];
        assert_eq!(hour_range(&es, &days, DEFAULT_HOURS), (0, 24));
        // Outside the week: ignored.
        let es = [entry("a", datetime!(2026-11-20 05:00), 60)];
        assert_eq!(hour_range(&es, &days, DEFAULT_HOURS), (7, 19));
    }

    #[test]
    fn overlaps_and_touching() {
        let es = [
            entry("a", datetime!(2026-11-10 13:00), 90),
            entry("b", datetime!(2026-11-10 14:15), 120),
            entry("c", datetime!(2026-11-10 16:15), 30),
        ];
        let o = overlaps(&es);
        assert_eq!(
            o,
            vec![Overlap {
                a: "a".into(),
                b: "b".into(),
                from: datetime!(2026-11-10 14:15),
                to: datetime!(2026-11-10 14:30),
            }]
        );
        // b ends exactly when c starts: not an overlap.
        assert!(!o.iter().any(|o| o.b == "c"));
    }

    #[test]
    fn overlapping_entries_share_the_column() {
        let day = date!(2026 - 11 - 10);
        let es = [
            entry("a", datetime!(2026-11-10 13:00), 90),
            entry("b", datetime!(2026-11-10 14:15), 120),
            entry("c", datetime!(2026-11-10 16:30), 30),
            entry("d", datetime!(2026-11-10 14:20), 10),
        ];
        let l = lanes(&day_segments(&es, day));
        assert_eq!(l["a"], (0, 3));
        assert_eq!(l["b"], (1, 3));
        assert_eq!(l["d"], (2, 3));
        assert_eq!(l["c"], (0, 1));
    }

    #[test]
    fn segments_split_at_midnight() {
        let es = [entry("a", datetime!(2026-11-10 23:00), 120)];
        assert_eq!(
            day_segments(&es, date!(2026 - 11 - 10)),
            vec![Segment {
                id: "a".into(),
                from: 23 * 3600,
                to: DAY_SECS
            }]
        );
        assert_eq!(
            day_segments(&es, date!(2026 - 11 - 11)),
            vec![Segment {
                id: "a".into(),
                from: 0,
                to: 3600
            }]
        );
    }

    #[test]
    fn moves_stop_at_neighbours() {
        let es = [
            entry("a", datetime!(2026-11-10 09:00), 60),
            entry("b", datetime!(2026-11-10 11:00), 60),
            entry("m", datetime!(2026-11-09 15:15), 60),
        ];
        let m = &es[2];
        // Free spot: snapped.
        assert_eq!(
            clamp_move(m, datetime!(2026-11-10 13:07), &es),
            Some(datetime!(2026-11-10 13:00))
        );
        // Into a: pushed flush against it, whichever side is closer.
        assert_eq!(
            clamp_move(m, datetime!(2026-11-10 08:45), &es),
            Some(datetime!(2026-11-10 08:00))
        );
        assert_eq!(
            clamp_move(m, datetime!(2026-11-10 09:45), &es),
            Some(datetime!(2026-11-10 10:00))
        );
        // The gap between a and b fits exactly one hour.
        assert_eq!(
            clamp_move(m, datetime!(2026-11-10 10:30), &es),
            Some(datetime!(2026-11-10 10:00))
        );
        // Moving within its own day ignores itself.
        assert_eq!(
            clamp_move(m, datetime!(2026-11-09 15:30), &es),
            Some(datetime!(2026-11-09 15:30))
        );
        // Day end.
        assert_eq!(
            clamp_move(m, datetime!(2026-11-12 23:30), &es),
            Some(datetime!(2026-11-12 23:00))
        );
        // A 2h entry into a 1h gap: goes to the nearest place it fits instead.
        let long = entry("l", datetime!(2026-11-09 08:00), 120);
        assert_eq!(
            clamp_move(&long, datetime!(2026-11-10 10:00), &es),
            Some(datetime!(2026-11-10 12:00))
        );
    }

    #[test]
    fn resizes_stop_at_neighbours() {
        let es = [
            entry("a", datetime!(2026-11-10 09:00), 60),
            entry("b", datetime!(2026-11-10 11:00), 60),
            entry("c", datetime!(2026-11-10 07:00), 60),
        ];
        let a = &es[0];
        assert_eq!(
            clamp_resize(a, Edge::End, datetime!(2026-11-10 10:30), &es),
            (datetime!(2026-11-10 09:00), 90 * 60)
        );
        assert_eq!(
            clamp_resize(a, Edge::End, datetime!(2026-11-10 12:00), &es),
            (datetime!(2026-11-10 09:00), 120 * 60)
        );
        // Never shorter than the snap step.
        assert_eq!(
            clamp_resize(a, Edge::End, datetime!(2026-11-10 08:00), &es),
            (datetime!(2026-11-10 09:00), MIN_SECS)
        );
        assert_eq!(
            clamp_resize(a, Edge::Start, datetime!(2026-11-10 07:30), &es),
            (datetime!(2026-11-10 08:00), 120 * 60)
        );
        assert_eq!(
            clamp_resize(a, Edge::Start, datetime!(2026-11-10 09:40), &es),
            (datetime!(2026-11-10 09:45), MIN_SECS)
        );
    }

    #[test]
    fn existing_overlaps_can_only_shrink() {
        let es = [
            entry("a", datetime!(2026-11-10 13:00), 90),
            entry("b", datetime!(2026-11-10 14:15), 120),
        ];
        // a overlaps b by 15 minutes: it can shrink out of it, not grow.
        assert_eq!(
            clamp_resize(&es[0], Edge::End, datetime!(2026-11-10 15:00), &es),
            (datetime!(2026-11-10 13:00), 90 * 60)
        );
        assert_eq!(
            clamp_resize(&es[0], Edge::End, datetime!(2026-11-10 14:15), &es),
            (datetime!(2026-11-10 13:00), 75 * 60)
        );
        assert_eq!(
            clamp_resize(&es[1], Edge::Start, datetime!(2026-11-10 13:30), &es),
            (datetime!(2026-11-10 14:15), 120 * 60)
        );
    }

    #[test]
    fn drag_to_create_fills_the_free_gap() {
        let day = date!(2026 - 11 - 11);
        let es = [
            entry("a", datetime!(2026-11-11 13:00), 105),
            entry("b", datetime!(2026-11-11 17:00), 60),
        ];
        assert_eq!(
            free_slot(day, time!(15:05), time!(16:28), &es),
            Some((datetime!(2026-11-11 15:00), 90 * 60))
        );
        // Dragged upwards.
        assert_eq!(
            free_slot(day, time!(16:20), time!(15:00), &es),
            Some((datetime!(2026-11-11 15:00), 90 * 60))
        );
        // Clipped at both neighbours.
        assert_eq!(
            free_slot(day, time!(15:00), time!(18:00), &es),
            Some((datetime!(2026-11-11 15:00), 120 * 60))
        );
        assert_eq!(
            free_slot(day, time!(15:00), time!(12:00), &es),
            Some((datetime!(2026-11-11 14:45), 30 * 60))
        );
        // A click: the minimum length.
        assert_eq!(
            free_slot(day, time!(09:10), time!(09:10), &es),
            Some((datetime!(2026-11-11 09:00), MIN_SECS))
        );
        // Inside an entry.
        assert_eq!(free_slot(day, time!(13:30), time!(14:00), &es), None);
    }
}

/// Every drag the grid can produce, including over midnight and against odd-length
/// neighbours: the rules must never panic.
#[cfg(test)]
mod sweep {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn drags_never_panic() {
        let mk = |id: &str, start: PrimitiveDateTime, secs: i64| TimeEntry {
            id: id.into(),
            work_item: Some(1),
            start,
            seconds: secs,
            comment: None,
            activity: None,
            can_edit: true,
            can_delete: true,
        };
        let base = datetime!(2026-11-10 00:00);
        let starts = [
            0i64,
            47,
            3600 * 8 + 47 * 60,
            3600 * 13,
            3600 * 23,
            3600 * 23 + 50 * 60,
        ];
        let lens = [
            0i64,
            30,
            7 * 60,
            15 * 60,
            47 * 60,
            3600,
            3 * 3600,
            25 * 3600,
        ];
        for &s in &starts {
            for &l in &lens {
                let e = mk("e", base + time::Duration::seconds(s), l);
                for other in [
                    None,
                    Some(mk("o", base + time::Duration::seconds(s + l / 2), 3600)),
                    Some(mk("p", base + time::Duration::seconds(s - 1800), 1800)),
                ] {
                    let mut es = vec![e.clone()];
                    es.extend(other);
                    for w in (-2 * 86400i64..3 * 86400).step_by(437) {
                        let wanted = base + time::Duration::seconds(w);
                        for edge in [Edge::Start, Edge::End] {
                            let r =
                                std::panic::catch_unwind(|| clamp_resize(&e, edge, wanted, &es));
                            assert!(
                                r.is_ok(),
                                "resize s={s} l={l} w={w} {edge:?} n={}",
                                es.len()
                            );
                        }
                        let r = std::panic::catch_unwind(|| clamp_move(&e, wanted, &es));
                        assert!(r.is_ok(), "move s={s} l={l} w={w}");
                    }
                }
            }
        }
    }
}
