//! Live timer, equivalent to the start/stop button in the 7pace UI.
//!
//! - `POST /api/tracking/client/startTracking`
//! - `POST /api/tracking/client/stopTracking/{reason}` — `0` = stopped by the client.
//! - `GET /api/tracking/client/current/{expand}` — the running timer, if any.
