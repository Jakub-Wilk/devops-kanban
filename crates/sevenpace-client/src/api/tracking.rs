//! Live timer, equivalent to the start/stop button in the 7pace UI. These endpoints return
//! `trackingStateModel` without the `{ data }` envelope.
//!
//! - `POST /api/tracking/client/startTracking` — starts (or switches) the timer; 7pace stops a
//!   running one first and saves it as a worklog.
//! - `POST /api/tracking/client/stopTracking/0` — `0` = stopped by the client.
//! - `GET /api/tracking/client/current/false` — the running timer, if any. The 7pace UI and
//!   desktop client can start or stop it too, so read this before showing the timer.

use reqwest::Method;

use crate::models::{StartTracking, TrackingStateDto};
use crate::{SevenPaceClient, SevenPaceError};

impl SevenPaceClient {
    pub async fn current_tracking(&self) -> Result<TrackingStateDto, SevenPaceError> {
        self.send_tracking(Method::GET, "tracking/client/current/false", None::<&()>)
            .await
    }

    pub async fn start_tracking(
        &self,
        body: &StartTracking,
    ) -> Result<TrackingStateDto, SevenPaceError> {
        self.send_tracking(Method::POST, "tracking/client/startTracking", Some(body))
            .await
    }

    pub async fn stop_tracking(&self) -> Result<TrackingStateDto, SevenPaceError> {
        self.send_tracking(Method::POST, "tracking/client/stopTracking/0", None::<&()>)
            .await
    }
}
