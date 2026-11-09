//! Serde DTOs mirroring 7pace JSON (camelCase), per the v3.2 OpenAPI spec. Nearly everything is
//! optional: the spec marks little as required, and a missing field must not fail a whole board.
//!
//! Note the spelling: the read model `workLog` has `timestamp`, the write model has `timeStamp`.
//! Both are the work's **start** in the user's local time; `length` is in seconds.
//!
//! The tracking models (`trackingStateModel`) are read leniently: 7pace sends some fields the
//! spec types as strings (e.g. `trackingState`) as integers, and a timer must not fail on that.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDto {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub unique_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityTypeDto {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// `RRGGBB`.
    #[serde(default)]
    pub color: Option<String>,
}

/// `GET rest/workLogs` item.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkLogDto {
    pub id: String,
    /// Start, user-local time (`2026-11-04T09:00:00`, maybe with fractions or an offset).
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub length: i64,
    #[serde(default)]
    pub billable_length: Option<i64>,
    #[serde(default)]
    pub work_item_id: Option<u32>,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub user: Option<UserDto>,
    #[serde(default)]
    pub activity_type: Option<ActivityTypeDto>,
    /// Set with `$includeEditable=true`.
    #[serde(default)]
    pub is_can_edit: Option<bool>,
    #[serde(default)]
    pub is_can_delete: Option<bool>,
}

/// Body of `POST rest/workLogs` and `PATCH rest/workLogs/{id}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkLogWrite {
    /// Start, user-local time without offset (`2026-11-04T09:00:00`).
    pub time_stamp: String,
    /// Seconds.
    pub length: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_item_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_type_id: Option<String>,
}

/// `GET rest/activityTypes`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityTypesDto {
    /// Activity types are switched on for the organization.
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub system_default_activity_type_id: Option<String>,
    #[serde(default)]
    pub activity_types: Vec<ActivityTypeDto>,
}

/// `GET rest/me`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeDto {
    #[serde(default)]
    pub user: Option<UserDto>,
    #[serde(default)]
    pub default_activity_type: Option<ActivityTypeDto>,
    /// UTC offset in minutes.
    #[serde(default)]
    pub timezone: Option<i32>,
}

/// Body of `POST tracking/client/startTracking`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTracking {
    /// UTC offset in minutes.
    pub time_zone: i32,
    pub tfs_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remark: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_type_id: Option<String>,
}

/// `trackingStateModel` (tracking endpoints; not wrapped in `{ data }`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackingStateDto {
    #[serde(default)]
    pub track: Option<TrackDto>,
    #[serde(default)]
    pub track_settings: Option<TrackSettingsDto>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackDto {
    /// The tracked work item.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub tfs_id: Option<u32>,
    #[serde(default, deserialize_with = "lenient_string")]
    pub remark: Option<String>,
    /// `tracking`, `idle`, `idleCheck`, `activityCheck` or `clientInputRequired` per the spec;
    /// the live API sends an integer, kept as its decimal text: `"0"` = tracking, `"1"` = not
    /// tracking (`track` then still holds the last track, with `currentTrackLength` 0).
    #[serde(default, deserialize_with = "lenient_string")]
    pub tracking_state: Option<String>,
    /// Seconds so far.
    #[serde(default, deserialize_with = "lenient_i64")]
    pub current_track_length: Option<i64>,
    #[serde(default, deserialize_with = "lenient_string")]
    pub current_track_started_date_time: Option<String>,
    #[serde(default)]
    pub work_item: Option<TrackedWorkItemDto>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackedWorkItemDto {
    #[serde(default, deserialize_with = "lenient_u32")]
    pub id: Option<u32>,
    #[serde(default, deserialize_with = "lenient_string")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackSettingsDto {
    #[serde(default, deserialize_with = "lenient_bool")]
    pub is_tracking_start_allowed: Option<bool>,
    /// `OK`, `Warning`, `Error` or `AuthError`.
    #[serde(default)]
    pub response_state: Option<Value>,
    #[serde(default, deserialize_with = "lenient_string")]
    pub response_message: Option<String>,
}

impl TrackingStateDto {
    /// The work item being tracked, when the timer runs. 7pace keeps returning the last
    /// `track` after it stops, so only the tracking state counts: `0` (live API) or `tracking`
    /// (spec). Any other state (`1`, `idle`, …) is not running. Without a state, a `track` counts.
    pub fn tracking(&self) -> Option<u32> {
        let t = self.track.as_ref()?;
        let running = match t.tracking_state.as_deref() {
            None => true,
            Some(s) => s == "0" || s.eq_ignore_ascii_case("tracking"),
        };
        running
            .then(|| t.tfs_id.or_else(|| t.work_item.as_ref()?.id))
            .flatten()
    }
}

/// A string, number or bool as text; `null` or absent as `None`.
fn lenient_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<Value>::deserialize(d)? {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s),
        Some(other) => Some(other.to_string()),
    })
}

/// A number, or a string holding one; anything else as `None`. Fractions are truncated.
fn lenient_i64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Ok(match Option::<Value>::deserialize(d)? {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Some(Value::String(s)) => s
            .trim()
            .parse::<i64>()
            .ok()
            .or_else(|| s.trim().parse::<f64>().ok().map(|f| f as i64)),
        _ => None,
    })
}

fn lenient_u32<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    Ok(lenient_i64(d)?.and_then(|n| u32::try_from(n).ok()))
}

/// A bool, `0`/`1`, or `"true"`/`"false"`.
fn lenient_bool<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    Ok(match Option::<Value>::deserialize(d)? {
        Some(Value::Bool(b)) => Some(b),
        Some(Value::Number(n)) => n.as_i64().map(|n| n != 0),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shapes written from the v3.2 OpenAPI spec (no live capture yet).

    #[test]
    fn worklog_reads_lowercase_timestamp() {
        let w: WorkLogDto = serde_json::from_str(
            r#"{"id":"5b7c6f1e-0000-0000-0000-000000000001","timestamp":"2026-11-04T09:00:00",
               "length":5400,"workItemId":117349,"comment":"Export",
               "activityType":{"id":"a1","name":"Development","color":"3E8EDE"},
               "isCanEdit":true,"isCanDelete":false}"#,
        )
        .unwrap();
        assert_eq!(w.timestamp.as_deref(), Some("2026-11-04T09:00:00"));
        assert_eq!((w.length, w.work_item_id), (5400, Some(117349)));
        assert_eq!(w.is_can_delete, Some(false));
    }

    #[test]
    fn write_uses_camel_time_stamp() {
        let body = serde_json::to_value(WorkLogWrite {
            time_stamp: "2026-11-04T09:00:00".into(),
            length: 1800,
            work_item_id: Some(1),
            ..WorkLogWrite::default()
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"timeStamp":"2026-11-04T09:00:00","length":1800,"workItemId":1})
        );
    }

    #[test]
    fn tracking_state() {
        let running: TrackingStateDto = serde_json::from_str(
            r#"{"track":{"tfsId":117349,"trackingState":"tracking","currentTrackLength":125,
               "currentTrackStartedDateTime":"2026-11-04T08:57:55Z"},
               "trackSettings":{"isTrackingStartAllowed":true,"responseState":"OK"}}"#,
        )
        .unwrap();
        assert_eq!(running.tracking(), Some(117349));
        let stopped: TrackingStateDto =
            serde_json::from_str(r#"{"track":null,"trackSettings":{}}"#).unwrap();
        assert_eq!(stopped.tracking(), None);
        let idle: TrackingStateDto =
            serde_json::from_str(r#"{"track":{"tfsId":5,"trackingState":"idle"}}"#).unwrap();
        assert_eq!(idle.tracking(), None);
    }

    /// `current/false` with nothing running, captured from the media-press org (trimmed
    /// `settings`): 7pace still returns the last track, with `trackingState` 1.
    #[test]
    fn stopped_timer_keeps_its_last_track() {
        let dto: TrackingStateDto = serde_json::from_str(
            r#"{"track":{"tfsId":118371,"remark":null,"activityTypeId":"52028856-e066-4f8f-9d80-9fcc4b77a87a",
               "workItem":null,"trackingState":1,"stoppedTrackType":null,
               "activityCheck":{"isRunning":false,"secondsLeft":0},"currentTrackLength":0,
               "billableTrackLength":null,"totalMeTodayLength":null,"totalMeLength":null,
               "totalTeamLength":-1,"trackAdjustmentLength":null,
               "currentTrackStartedDateTime":"2026-11-06T15:57:24.283","stateChangeDateTime":null,
               "trackTimeZone":null,"workLogId":null},
               "trackSettings":{"isTrackingStartAllowed":true,"responseState":0,"responseMessage":null},
               "settings":{"activityType":{"isEnabled":true},"userInfo":null},
               "timestamp":639295757881603968}"#,
        )
        .unwrap();
        assert_eq!(dto.tracking(), None);
    }

    /// The live API sends enums as integers where the spec has strings
    /// ("invalid type: integer `0`, expected a string"); `0` is tracking.
    #[test]
    fn tracking_state_with_numeric_fields() {
        let dto: TrackingStateDto = serde_json::from_str(
            r#"{"track":{"tfsId":117349,"trackingState":0,"remark":null,"currentTrackLength":125.6,
               "currentTrackStartedDateTime":"2026-11-04T08:57:55","workItem":{"id":117349,"title":42}},
               "trackSettings":{"isTrackingStartAllowed":1,"responseState":0,"responseMessage":null}}"#,
        )
        .unwrap();
        assert_eq!(dto.tracking(), Some(117349));
        let track = dto.track.as_ref().unwrap();
        assert_eq!(track.tracking_state.as_deref(), Some("0"));
        assert_eq!(track.current_track_length, Some(125));
        assert_eq!(
            track.work_item.as_ref().unwrap().title.as_deref(),
            Some("42")
        );
        let settings = dto.track_settings.unwrap();
        assert_eq!(settings.is_tracking_start_allowed, Some(true));
    }
}
