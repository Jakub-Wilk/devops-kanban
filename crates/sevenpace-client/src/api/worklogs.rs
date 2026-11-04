//! Worklogs (time entries).
//!
//! - `GET /api/rest/workLogs` — own entries. Filters: `$workItemIds` (≤100, comma-separated),
//!   `$fromTimestamp` / `$toTimestamp`, `$count` (≤500) + `$skip`, `$includeEditable`.
//! - `GET /api/rest/workLogs/all` — everyone's; needs the Product, Budget or Administrator role.
//! - `POST /api/rest/workLogs` — create one (log time against a work item).
//! - `POST /api/rest/workLogs/batch` — create ≤500; `$validateOnly=true` for a dry run.
//! - `PATCH|DELETE /api/rest/workLogs/{id}` — edit / delete by uuid.
