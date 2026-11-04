//! Thin async client for the Azure DevOps REST API.
//!
//! Scope: only the Boards (`_apis/work/...`) and Work Item Tracking (`_apis/wit/...`) surface the
//! kanban needs. No UI, no persistence, no platform-specific code — this crate must stay usable
//! from a future web/fullstack target. See `docs/azure-devops-api.md` for the endpoint catalogue.

pub mod api;
pub mod auth;
pub mod client;
pub mod error;
pub mod models;
pub mod rate_limit;

pub use auth::{AuthProvider, PatAuth};
pub use client::{AdoClient, ClientConfig};
pub use error::AdoError;
pub use models::*;
