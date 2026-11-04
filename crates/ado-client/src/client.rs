//! `AdoClient`: owns the `reqwest::Client`, base URL, auth and rate-limit state.
//!
//! All endpoint functions in [`crate::api`] are inherent methods on `AdoClient` grouped by area.
//! Responsibilities here: URL building (`{base}/{org}/{project}/{team}/_apis/...`),
//! `api-version` query param, auth header, JSON (de)serialization, mapping HTTP failures to
//! [`AdoError`](crate::AdoError), and feeding response headers to [`RateLimiter`].

use std::sync::Arc;

use url::Url;

use crate::{auth::AuthProvider, rate_limit::RateLimiter};

/// Static client configuration (mirrors the `[api]` section of the app settings).
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// `https://dev.azure.com`
    pub base_url: Url,
    /// Organization name, e.g. `media-press`.
    pub organization: String,
    /// Default api-version, e.g. `7.1`. Preview endpoints (comments) pin their own.
    pub api_version: String,
    /// Max ids per `workitemsbatch` call (hard limit 200).
    pub max_batch: usize,
}

#[derive(Clone)]
pub struct AdoClient {
    pub(crate) http: reqwest::Client,
    pub(crate) config: Arc<ClientConfig>,
    pub(crate) auth: Arc<dyn AuthProvider>,
    pub(crate) rate_limiter: Arc<RateLimiter>,
}

impl AdoClient {
    pub fn new(_config: ClientConfig, _auth: Arc<dyn AuthProvider>) -> Self {
        todo!()
    }
}
