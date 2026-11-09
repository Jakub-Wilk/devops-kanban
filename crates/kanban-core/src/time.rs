//! Time tracking via 7pace.
//!
//! Two read paths, by cost:
//! - **Totals on cards:** 7pace syncs total tracked hours into `Microsoft.VSTS.Scheduling.CompletedWork`
//!   (configured per project). Fetch it with the other card fields — no 7pace call. Excludes time
//!   tracked before the sync was enabled.
//! - **Entries, logging and the timer:** `sevenpace-client` — list a work item's worklogs, create /
//!   edit / delete them, start / stop tracking. After a write, refetch the card so `CompletedWork`
//!   reflects the new total.
//!
//! This module is the boundary: durations are seconds on the wire, and `timeStamp` is the start in
//! the user's **local** time (no offset), which [`TimeEntry`] keeps as a `PrimitiveDateTime`.
//! Also: free-text duration parsing for the time dialog, 24-hour time-of-day input (never
//! AM/PM) and the running [`TimerState`].

use sevenpace_client::models::{TrackingStateDto, WorkLogDto};
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{OffsetDateTime, PrimitiveDateTime, UtcOffset};

use crate::card::CardId;

/// Longest entry the dialog accepts.
pub const MAX_ENTRY_SECS: i64 = 24 * 3600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub id: String,
    pub name: String,
    /// `RRGGBB`.
    pub color: Option<String>,
}

/// One of the user's own worklogs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeEntry {
    /// 7pace worklog uuid.
    pub id: String,
    pub work_item: Option<CardId>,
    /// Start, user-local.
    pub start: PrimitiveDateTime,
    pub seconds: i64,
    pub comment: Option<String>,
    pub activity: Option<Activity>,
    /// 7pace said so (`isCanEdit`); assumed when it didn't say.
    pub can_edit: bool,
    pub can_delete: bool,
}

impl TimeEntry {
    /// `None` when the timestamp can't be read. `local` converts timestamps that carry an offset.
    pub fn from_dto(dto: &WorkLogDto, local: UtcOffset) -> Option<Self> {
        Some(Self {
            id: dto.id.clone(),
            work_item: dto.work_item_id,
            start: parse_local_timestamp(dto.timestamp.as_deref()?, local)?,
            seconds: dto.length.max(0),
            comment: dto.comment.clone().filter(|c| !c.trim().is_empty()),
            activity: dto.activity_type.as_ref().map(|a| Activity {
                id: a.id.clone(),
                name: a.name.clone(),
                color: a.color.clone(),
            }),
            can_edit: dto.is_can_edit.unwrap_or(true),
            can_delete: dto.is_can_delete.unwrap_or(true),
        })
    }
}

/// A 7pace timestamp as local wall-clock time: naive (`2026-11-04T09:00:00`, optional
/// fractions) is taken as local already; one with `Z` or an offset is converted to `local`.
pub fn parse_local_timestamp(s: &str, local: UtcOffset) -> Option<PrimitiveDateTime> {
    let s = s.trim();
    let time_part = s.split_once('T').map_or("", |(_, t)| t);
    if s.ends_with('Z') || s.ends_with('z') || time_part.contains(['+', '-']) {
        let t = OffsetDateTime::parse(s, &Rfc3339).ok()?.to_offset(local);
        return Some(PrimitiveDateTime::new(t.date(), t.time()));
    }
    let naive = s.split_once('.').map_or(s, |(head, _)| head);
    PrimitiveDateTime::parse(
        naive,
        format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]"),
    )
    .ok()
}

/// `2026-11-04T09:00:00`: local time without offset, as 7pace's `timeStamp` wants it.
pub fn wire_timestamp(t: PrimitiveDateTime) -> String {
    t.format(format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second]"
    ))
    .unwrap_or_default()
}

/// Seconds from what a person types: `1:30`, `1h 30m`, `1h30`, `90m`, `1.5h`, `1,5` or `2`
/// (a bare number is hours). `None` for anything else, zero, or more than a day.
pub fn parse_duration(input: &str) -> Option<i64> {
    let s = input.trim().to_lowercase().replace(',', ".");
    if s.is_empty() {
        return None;
    }
    let secs = if let Some((h, m)) = s.split_once(':') {
        let (h, m): (i64, i64) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
        if !(0..60).contains(&m) {
            return None;
        }
        h * 3600 + m * 60
    } else if let Ok(hours) = s.parse::<f64>() {
        (hours * 3600.0).round() as i64
    } else {
        // Number + unit groups: "1h 30m", "1h30", "90min", "1.5h".
        let mut total = 0.0;
        let mut rest = s.as_str();
        let mut last_unit = None;
        while !rest.trim_start().is_empty() {
            rest = rest.trim_start();
            let end = rest
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .unwrap_or(rest.len());
            let n: f64 = rest[..end].parse().ok()?;
            rest = rest[end..].trim_start();
            let unit_end = rest
                .find(|c: char| !c.is_ascii_alphabetic())
                .unwrap_or(rest.len());
            let unit = match &rest[..unit_end] {
                "h" | "hr" | "hrs" | "hour" | "hours" => 3600.0,
                "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
                // "1h30": minutes implied after hours.
                "" if last_unit == Some(3600.0) => 60.0,
                _ => return None,
            };
            total += n * unit;
            last_unit = Some(unit);
            rest = &rest[unit_end..];
        }
        total.round() as i64
    };
    (secs > 0 && secs <= MAX_ENTRY_SECS).then_some(secs)
}

/// `1h 30m`, `45m`, `2h` (rounded to minutes).
pub fn format_duration(secs: i64) -> String {
    let mins = (secs.max(0) + 30) / 60;
    match (mins / 60, mins % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// Longest entry the time dialog takes: a day.
pub const MAX_ENTRY_MINUTES: i64 = 24 * 60;

/// The time dialog's rigid `H:MM` duration field. Digits fill in from the right, so typing
/// `1`, `3`, `0` reads `0:01`, `0:13`, `1:30`; anything but digits is dropped. Empty for zero.
pub fn hmm_from_input(input: &str) -> String {
    let digits: String = input.chars().filter(char::is_ascii_digit).collect();
    let digits = &digits[digits.len().saturating_sub(4)..];
    match digits.parse::<u32>() {
        Ok(0) | Err(_) => String::new(),
        Ok(n) => format!("{}:{:02}", n / 100, n % 100),
    }
}

/// Minutes in an `H:MM` value (`""` is zero). `None` for 60 or more minutes, or over a day.
pub fn parse_hmm(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return Some(0);
    }
    let (h, m) = s.split_once(':')?;
    let (h, m): (i64, i64) = (h.parse().ok()?, m.parse().ok()?);
    let total = h * 60 + m;
    ((0..60).contains(&m) && h >= 0 && total <= MAX_ENTRY_MINUTES).then_some(total)
}

/// `1:05`; empty for zero.
pub fn format_hmm(minutes: i64) -> String {
    if minutes <= 0 {
        String::new()
    } else {
        format!("{}:{:02}", minutes / 60, minutes % 60)
    }
}

/// A time-of-day field as typed: digits and at most one `:`, no more than 5 characters. Never
/// AM/PM: every time in the app is 24-hour.
pub fn clock_input(input: &str) -> String {
    let mut colon = false;
    input
        .chars()
        .filter(|c| {
            if *c == ':' && !colon {
                colon = true;
                return true;
            }
            c.is_ascii_digit()
        })
        .take(5)
        .collect()
}

/// A 24-hour time of day from what a person types: `9`, `09`, `930`, `0930`, `9:30`, `17`,
/// `1730`, `17:30`. `None` for anything else, an hour over 23 or minutes over 59.
pub fn parse_time_of_day(input: &str) -> Option<time::Time> {
    let s = input.trim();
    let (h, m) = match s.split_once(':') {
        Some((h, m)) => (h, m),
        None if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) => match s.len() {
            1 | 2 => (s, "0"),
            3 => s.split_at(1),
            4 => s.split_at(2),
            _ => return None,
        },
        None => return None,
    };
    if h.is_empty() || h.len() > 2 || m.is_empty() || m.len() > 2 {
        return None;
    }
    let (h, m): (u8, u8) = (h.parse().ok()?, m.parse().ok()?);
    time::Time::from_hms(h, m, 0).ok()
}

/// `09:05`.
pub fn format_time_of_day(t: time::Time) -> String {
    format!("{:02}:{:02}", t.hour(), t.minute())
}

/// `0:42:10`, for a running timer.
pub fn format_clock(secs: i64) -> String {
    let s = secs.max(0);
    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// The UTC offset in minutes, as `startTracking` wants it.
pub fn tz_minutes(offset: UtcOffset) -> i32 {
    offset.whole_minutes().into()
}

/// The running 7pace timer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerState {
    pub work_item: CardId,
    /// 7pace's title of the item, when it sent one.
    pub title: Option<String>,
    pub started: OffsetDateTime,
}

impl TimerState {
    /// `None` when nothing is being tracked. The start is 7pace's `currentTrackStartedDateTime`
    /// (naive values are local), else `now - currentTrackLength`. A start in the future means the
    /// offsets disagree (7pace's notion of local time isn't ours), so the length wins then.
    pub fn from_dto(dto: &TrackingStateDto, now: OffsetDateTime, local: UtcOffset) -> Option<Self> {
        let work_item = dto.tracking()?;
        let track = dto.track.as_ref()?;
        let from_length = track
            .current_track_length
            .map(|len| now - time::Duration::seconds(len.max(0)));
        let started = track
            .current_track_started_date_time
            .as_deref()
            .and_then(|s| parse_local_timestamp(s, local))
            .map(|t| t.assume_offset(local))
            .filter(|t| from_length.is_none() || *t <= now + time::Duration::MINUTE)
            .or(from_length)
            .unwrap_or(now);
        Some(Self {
            work_item,
            title: track.work_item.as_ref().and_then(|w| w.title.clone()),
            started: started.min(now),
        })
    }

    pub fn elapsed_secs(&self, now: OffsetDateTime) -> i64 {
        (now - self.started).whole_seconds().max(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{datetime, offset};

    #[test]
    fn durations() {
        for (input, secs) in [
            ("1:30", 5400),
            ("0:45", 2700),
            ("1h 30m", 5400),
            ("1h30m", 5400),
            ("1h30", 5400),
            ("90m", 5400),
            ("90 min", 5400),
            ("1.5h", 5400),
            ("1,5", 5400),
            ("2", 7200),
            ("15m", 900),
        ] {
            assert_eq!(parse_duration(input), Some(secs), "{input}");
        }
        for bad in ["", "0", "abc", "1:75", "25h", "-1", "1x"] {
            assert_eq!(parse_duration(bad), None, "{bad}");
        }
        assert_eq!(format_duration(5400), "1h 30m");
        assert_eq!(format_duration(2700), "45m");
        assert_eq!(format_duration(7200), "2h");
        assert_eq!(format_clock(2530), "0:42:10");
    }

    #[test]
    fn rigid_hmm_field() {
        assert_eq!(hmm_from_input("1"), "0:01");
        assert_eq!(hmm_from_input("13"), "0:13");
        assert_eq!(hmm_from_input("130"), "1:30");
        assert_eq!(hmm_from_input("1:30"), "1:30");
        assert_eq!(hmm_from_input("1:3"), "0:13");
        assert_eq!(hmm_from_input("0130"), "1:30");
        assert_eq!(hmm_from_input("12345"), "23:45");
        assert_eq!(hmm_from_input("0"), "");
        assert_eq!(hmm_from_input("x"), "");
        assert_eq!(parse_hmm(""), Some(0));
        assert_eq!(parse_hmm("1:30"), Some(90));
        assert_eq!(parse_hmm("24:00"), Some(1440));
        assert_eq!(parse_hmm("0:75"), None);
        assert_eq!(parse_hmm("24:01"), None);
        assert_eq!(format_hmm(65), "1:05");
        assert_eq!(format_hmm(0), "");
    }

    #[test]
    fn time_of_day_is_24_hour() {
        use time::macros::time;
        for (input, t) in [
            ("9", time!(09:00)),
            ("09", time!(09:00)),
            ("930", time!(09:30)),
            ("0930", time!(09:30)),
            ("9:30", time!(09:30)),
            ("17", time!(17:00)),
            ("1730", time!(17:30)),
            ("17:30", time!(17:30)),
            ("00:00", time!(00:00)),
            ("23:59", time!(23:59)),
            (" 8:05 ", time!(08:05)),
        ] {
            assert_eq!(parse_time_of_day(input), Some(t), "{input}");
        }
        for bad in [
            "", "24:00", "25", "2400", "12:60", "5pm", "5 PM", "1:2:3", "12345", ":30", "9:",
        ] {
            assert_eq!(parse_time_of_day(bad), None, "{bad}");
        }
        assert_eq!(format_time_of_day(time!(09:05)), "09:05");
        assert_eq!(clock_input("5:30 PM"), "5:30");
        assert_eq!(clock_input("17::30"), "17:30");
        assert_eq!(clock_input("123456"), "12345");
    }

    #[test]
    fn timestamps_stay_local() {
        let local = offset!(+1);
        assert_eq!(
            parse_local_timestamp("2026-11-04T09:00:00", local),
            Some(datetime!(2026-11-04 09:00:00))
        );
        assert_eq!(
            parse_local_timestamp("2026-11-04T09:00:00.1234567", local),
            Some(datetime!(2026-11-04 09:00:00))
        );
        // With an offset: converted to local wall-clock time.
        assert_eq!(
            parse_local_timestamp("2026-11-04T08:00:00Z", local),
            Some(datetime!(2026-11-04 09:00:00))
        );
        assert_eq!(
            parse_local_timestamp("2026-11-04T10:00:00+02:00", local),
            Some(datetime!(2026-11-04 09:00:00))
        );
        assert_eq!(parse_local_timestamp("yesterday", local), None);
        assert_eq!(
            wire_timestamp(datetime!(2026-11-04 09:05:00)),
            "2026-11-04T09:05:00"
        );
        assert_eq!(tz_minutes(offset!(+1)), 60);
    }

    #[test]
    fn entry_from_dto() {
        let dto = WorkLogDto {
            id: "w1".into(),
            timestamp: Some("2026-11-04T09:00:00".into()),
            length: 5400,
            work_item_id: Some(117349),
            comment: Some("  ".into()),
            is_can_edit: Some(false),
            ..WorkLogDto::default()
        };
        let e = TimeEntry::from_dto(&dto, offset!(+1)).unwrap();
        assert_eq!(e.start, datetime!(2026-11-04 09:00:00));
        assert_eq!((e.seconds, e.work_item), (5400, Some(117349)));
        assert_eq!(e.comment, None);
        assert!(!e.can_edit && e.can_delete);
    }

    /// `current/false` with a timer running, captured from the media-press org (trimmed
    /// `settings`). Its `timestamp` (.NET ticks) is 15:31:28.67 UTC, so the naive start is local
    /// time (UTC+1): 16:31:02.657 + 26 s.
    #[test]
    fn running_timer_from_live_response() {
        let dto: TrackingStateDto = serde_json::from_str(
            r#"{"track":{"tfsId":118371,"remark":"","activityTypeId":"52028856-e066-4f8f-9d80-9fcc4b77a87a",
               "workItem":null,"trackingState":0,"stoppedTrackType":null,
               "activityCheck":{"isRunning":false,"secondsLeft":0},"currentTrackLength":26,
               "billableTrackLength":null,"totalMeTodayLength":null,"totalMeLength":null,
               "totalTeamLength":-1,"trackAdjustmentLength":null,
               "currentTrackStartedDateTime":"2026-11-06T16:31:02.657","stateChangeDateTime":null,
               "trackTimeZone":null,"workLogId":null},
               "trackSettings":{"isTrackingStartAllowed":true,"responseState":0,"responseMessage":null},
               "settings":{"activityType":{"isEnabled":true},"userInfo":null},
               "timestamp":639295758886715157}"#,
        )
        .unwrap();
        let now = datetime!(2026-11-06 15:31:28 UTC).to_offset(offset!(+1));
        let t = TimerState::from_dto(&dto, now, offset!(+1)).unwrap();
        assert_eq!(t.work_item, 118371);
        assert_eq!(t.title, None);
        assert_eq!(t.elapsed_secs(now), 26);
    }

    #[test]
    fn timer_start_from_length_or_timestamp() {
        let now = datetime!(2026-11-04 09:10:00 +1);
        let dto: TrackingStateDto = serde_json::from_str(
            r#"{"track":{"tfsId":7,"trackingState":"tracking","currentTrackLength":600}}"#,
        )
        .unwrap();
        let t = TimerState::from_dto(&dto, now, offset!(+1)).unwrap();
        assert_eq!((t.work_item, t.elapsed_secs(now)), (7, 600));
        let dto: TrackingStateDto = serde_json::from_str(
            r#"{"track":{"tfsId":7,"currentTrackStartedDateTime":"2026-11-04T09:00:00","currentTrackLength":1}}"#,
        )
        .unwrap();
        let t = TimerState::from_dto(&dto, now, offset!(+1)).unwrap();
        assert_eq!(t.elapsed_secs(now), 600);
        // Started "an hour ahead": the timestamp was read in the wrong offset; trust the length.
        let dto: TrackingStateDto = serde_json::from_str(
            r#"{"track":{"tfsId":7,"currentTrackStartedDateTime":"2026-11-04T10:05:00","currentTrackLength":300}}"#,
        )
        .unwrap();
        let t = TimerState::from_dto(&dto, now, offset!(+1)).unwrap();
        assert_eq!(t.elapsed_secs(now), 300);
        assert_eq!(
            TimerState::from_dto(&TrackingStateDto::default(), now, offset!(+1)),
            None
        );
    }
}
