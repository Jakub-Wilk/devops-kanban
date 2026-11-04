//! Error type for every client call.

#[derive(Debug, thiserror::Error)]
pub enum SevenPaceError {
    /// 401 — token missing, expired (1 year) or revoked.
    #[error("7pace authentication failed — check the 7pace API token")]
    Unauthorized,
    /// 403 / `AccessDenied` — e.g. `workLogs/all` without the Product, Budget or Admin role.
    #[error("forbidden: {0}")]
    Forbidden(String),
    /// 409 "Data conflict". 7pace has no rev-based concurrency, so this is the only signal.
    #[error("conflict: {0}")]
    Conflict(String),
    /// 422 validation, or 400 with e.g. `FilterWorkItemIdsCountExceeded` (more than 100 ids).
    #[error("invalid request: {0}")]
    Validation(String),
    /// 429; `retry_after_secs` from the `Retry-After` header.
    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    /// `ApiDisabled` — 7pace turns the API off during account migration.
    #[error("the 7pace API is disabled for this organization")]
    ApiDisabled,
    /// Any other non-success status, with the envelope's error code and message if present.
    #[error("7pace error {status}: {message}")]
    Api {
        status: u16,
        code: Option<String>,
        message: String,
    },
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
