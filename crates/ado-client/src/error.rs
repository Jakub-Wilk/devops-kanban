//! Error type for every client call.

/// Failures the domain layer must distinguish between.
#[derive(Debug, thiserror::Error)]
pub enum AdoError {
    /// 401 / 203 (ADO returns a 203 sign-in page for bad PATs).
    #[error("authentication failed — check the PAT and its scopes")]
    Unauthorized,
    /// 403.
    #[error("forbidden: {0}")]
    Forbidden(String),
    /// 404.
    #[error("not found: {0}")]
    NotFound(String),
    /// JSON Patch `test /rev` failed — someone else changed the item. Caller should refetch.
    #[error("work item {id} was modified concurrently")]
    Conflict { id: u32 },
    /// 429 / TF400733. `retry_after_secs` from the `Retry-After` header.
    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    /// Any other non-success status, with ADO's `message` / `typeKey` if present.
    #[error("Azure DevOps error {status}: {message}")]
    Api {
        status: u16,
        type_key: Option<String>,
        message: String,
    },
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
