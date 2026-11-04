//! Serde DTOs mirroring 7pace JSON (camelCase), per the v3.2 OpenAPI spec. Nearly everything is
//! optional: the spec marks little as required, and a missing field must not fail a whole board.
//!
//! Note the spelling: the read model `workLog` has `timestamp`, the write model has `timeStamp`.
//! Both are the work's **start** in the user's local time; `length` is in seconds.

use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub tfs_id: Option<u32>,
    #[serde(default)]
    pub remark: Option<String>,
    /// `tracking`, `idle`, `idleCheck`, `activityCheck` or `clientInputRequired`.
    #[serde(default)]
    pub tracking_state: Option<String>,
    /// Seconds so far.
    #[serde(default)]
    pub current_track_length: Option<i64>,
    #[serde(default)]
    pub current_track_started_date_time: Option<String>,
    #[serde(default)]
    pub work_item: Option<TrackedWorkItemDto>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackedWorkItemDto {
    #[serde(default)]
    pub id: Option<u32>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackSettingsDto {
    #[serde(default)]
    pub is_tracking_start_allowed: Option<bool>,
    /// `OK`, `Warning`, `Error` or `AuthError`.
    #[serde(default)]
    pub response_state: Option<serde_json::Value>,
    #[serde(default)]
    pub response_message: Option<String>,
}

impl TrackingStateDto {
    /// The work item being tracked, when the timer runs.
    pub fn tracking(&self) -> Option<u32> {
        let t = self.track.as_ref()?;
        let state = t.tracking_state.as_deref().unwrap_or("tracking");
        (state != "idle")
            .then(|| t.tfs_id.or_else(|| t.work_item.as_ref()?.id))
            .flatten()
    }
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
}
