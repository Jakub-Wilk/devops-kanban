//! Work item reads and writes.
//!
//! - `POST {project}/_apis/wit/workitemsbatch` — ≤200 ids per call, explicit `fields`.
//! - `PATCH {project}/_apis/wit/workitems/{id}` — `application/json-patch+json`; first op is
//!   always `test /rev` for optimistic concurrency.
//! - `POST {project}/_apis/wit/workitems/${type}` — create.
//! - `GET {project}/_apis/wit/workitemtypes/{type}/states` — state colors + categories.
