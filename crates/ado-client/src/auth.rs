//! Authentication providers.
//!
//! v1 ships PAT only (`Authorization: Basic base64(":" + pat)`). The trait exists so Microsoft
//! Entra ID (bearer token, resource `499b84ac-1321-427f-aa17-267ca6975798`) can be added later
//! without touching call sites. Secrets must never be logged: `Debug` impls redact them.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

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
        format!("Basic {}", STANDARD.encode(format!(":{}", self.pat)))
    }
}

impl fmt::Debug for PatAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PatAuth")
            .field("pat", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_header_has_empty_username() {
        assert_eq!(PatAuth::new("abc").authorization_header(), "Basic OmFiYw==");
    }

    #[test]
    fn debug_redacts_pat() {
        assert!(!format!("{:?}", PatAuth::new("secret")).contains("secret"));
    }
}
