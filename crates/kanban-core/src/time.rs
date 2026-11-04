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
//! Durations are seconds on the wire; `TimeEntry` converts at the boundary. `timeStamp` is the
//! user's local time, not UTC.

use crate::card::CardId;

/// One worklog, as the domain sees it.
#[derive(Debug, Clone)]
pub struct TimeEntry {
    /// 7pace worklog uuid.
    pub id: String,
    pub work_item: Option<CardId>,
    pub start: time::PrimitiveDateTime,
    pub duration: std::time::Duration,
    pub comment: Option<String>,
    pub activity_type: Option<String>,
    pub user: Option<String>,
}
