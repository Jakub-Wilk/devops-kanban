//! Error type for the domain layer.

use ado_client::AdoError;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error(transparent)]
    Ado(#[from] AdoError),
    /// A board or backlog configuration ADO returned but we can't use.
    #[error("board unavailable: {0}")]
    BoardUnavailable(String),
    #[error("settings: {0}")]
    Config(String),
    /// OS keyring unavailable or the entry couldn't be written.
    #[error("keyring: {0}")]
    Keyring(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
