//! 7pace API token auth.
//!
//! v1 uses a token created in 7pace Settings → Reporting & API (valid 1 year, renewed only in the
//! 7pace UI), sent as `Authorization: Bearer <token>`. The OAuth PIN pairing flow
//! (`client_id=OpenApi`, 1 h access token + refresh token) can be added later as another
//! [`AuthProvider`]. Secrets must never be logged: `Debug` redacts them.

use std::fmt;

pub use ado_client::AuthProvider;

#[derive(Clone)]
pub struct SevenPaceToken {
    token: String,
}

impl SevenPaceToken {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

impl AuthProvider for SevenPaceToken {
    fn authorization_header(&self) -> String {
        todo!("Bearer <token>")
    }
}

impl fmt::Debug for SevenPaceToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SevenPaceToken")
            .field("token", &"<redacted>")
            .finish()
    }
}
