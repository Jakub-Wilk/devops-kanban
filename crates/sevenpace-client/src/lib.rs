//! Thin async client for the 7pace Timetracker REST API.
//!
//! 7pace is a separate service from Azure DevOps: its own host
//! (`https://{org}.timehub.7pace.com`), its own token (an ADO PAT is not accepted) and its own
//! api-version (`3.2`). Scope: worklogs, the start/stop timer, activity types and the current user.
//! Same rules as `ado-client`: no UI, no persistence, DTOs mirror the wire JSON.
//! See `docs/7pace-api.md`.

// Scaffold: fields are wired up once the client is implemented. Remove this allow then.
#![allow(dead_code)]

pub mod api;
pub mod auth;
pub mod client;
pub mod error;
pub mod models;

pub use auth::SevenPaceToken;
pub use client::{SevenPaceClient, SevenPaceConfig};
pub use error::SevenPaceError;
