//! `SevenPaceClient`: owns the `reqwest::Client`, base URL and auth.
//!
//! Responsibilities: URL building (`{base}/api/rest/<resource>`), the required `api-version`
//! query param, the auth header, unwrapping the `{ "data": … }` / `{ "error": … }` envelope, and
//! mapping failures to [`SevenPaceError`](crate::SevenPaceError). 429 carries `Retry-After`.

use std::sync::Arc;

use url::Url;

use crate::auth::AuthProvider;

/// Static client configuration (mirrors the `[time_tracking]` section of the app settings).
#[derive(Debug, Clone)]
pub struct SevenPaceConfig {
    /// `https://{organization}.timehub.7pace.com`
    pub base_url: Url,
    /// `3.2` (current stable; `3.3-beta` may change without notice).
    pub api_version: String,
}

#[derive(Clone)]
pub struct SevenPaceClient {
    pub(crate) http: reqwest::Client,
    pub(crate) config: Arc<SevenPaceConfig>,
    pub(crate) auth: Arc<dyn AuthProvider>,
}

impl SevenPaceClient {
    pub fn new(_config: SevenPaceConfig, _auth: Arc<dyn AuthProvider>) -> Self {
        todo!()
    }
}
