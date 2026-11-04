//! Authentication providers.
//!
//! v1 ships PAT only (`Authorization: Basic base64(":" + pat)`). The trait exists so Microsoft
//! Entra ID (bearer token, resource `499b84ac-1321-427f-aa17-267ca6975798`) can be added later
//! without touching call sites. Secrets must never be logged: `Debug` impls redact them.

use std::fmt;

/// Produces the value of the `Authorization` header for each request.
pub trait AuthProvider: Send + Sync {
    fn authorization_header(&self) -> String;
}

/// Personal Access Token auth. Required scope: *Work Items (Read & write)*.
#[derive(Clone)]
pub struct PatAuth {
    pat: String,
}

impl PatAuth {
    pub fn new(pat: impl Into<String>) -> Self {
        Self { pat: pat.into() }
    }
}

impl AuthProvider for PatAuth {
    fn authorization_header(&self) -> String {
        todo!("Basic base64(\":\" + pat)")
    }
}

impl fmt::Debug for PatAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PatAuth")
            .field("pat", &"<redacted>")
            .finish()
    }
}
