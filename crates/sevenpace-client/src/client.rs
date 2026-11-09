//! `SevenPaceClient`: owns the `reqwest::Client`, base URL and auth.
//!
//! Responsibilities: URL building (`{base}/api/...`), the required `api-version` query param,
//! the auth header, unwrapping the REST `{ "data": … }` envelope (the tracking endpoints return
//! their model unwrapped, and a `DELETE` returns no body), and mapping failures to
//! [`SevenPaceError`]. Errors arrive as `{ status, errorCode, errorDescription, fields }` (v3.2
//! spec); the older `{ "error": { code, message } }` shape is accepted too. Only GETs are retried
//! (once, on 5xx).

use std::sync::Arc;
use std::time::Duration;

use reqwest::{Method, StatusCode, header};
use serde::Serialize;
use serde::de::DeserializeOwned;
use url::Url;

use crate::SevenPaceError;
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

/// `{ "data": T }`.
#[derive(serde::Deserialize)]
struct Envelope<T> {
    data: T,
}

impl SevenPaceClient {
    pub fn new(config: SevenPaceConfig, auth: Arc<dyn AuthProvider>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .user_agent(concat!("devops-kanban/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
            config: Arc::new(config),
            auth,
        }
    }

    pub fn config(&self) -> &SevenPaceConfig {
        &self.config
    }

    /// `{base}/api/{path}?{query}&api-version=…`. `path` segments are URL-encoded.
    pub(crate) fn url(&self, path: &str, query: &[(&str, String)]) -> Url {
        let mut url = self.config.base_url.clone();
        if let Ok(mut seg) = url.path_segments_mut() {
            seg.pop_if_empty()
                .push("api")
                .extend(path.split('/').filter(|s| !s.is_empty()));
        }
        {
            let mut q = url.query_pairs_mut();
            for (k, v) in query {
                q.append_pair(k, v);
            }
            q.append_pair("api-version", &self.config.api_version);
        }
        url
    }

    /// A REST call (`rest/...`), unwrapping `{ "data": … }`.
    pub(crate) async fn send_rest<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&(impl Serialize + ?Sized)>,
    ) -> Result<T, SevenPaceError> {
        let bytes = self.send(method, path, query, body).await?;
        let env: Envelope<T> = serde_json::from_slice(&bytes)?;
        Ok(env.data)
    }

    /// A REST call whose response body is ignored (7pace answers a `DELETE` with an empty body).
    pub(crate) async fn send_rest_no_content(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&(impl Serialize + ?Sized)>,
    ) -> Result<(), SevenPaceError> {
        self.send(method, path, query, body).await.map(drop)
    }

    /// A tracking call (`tracking/client/...`), whose model is not wrapped.
    pub(crate) async fn send_tracking<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&(impl Serialize + ?Sized)>,
    ) -> Result<T, SevenPaceError> {
        let bytes = self.send(method, path, &[], body).await?;
        // The tracking shapes come from the spec, not a live capture: log what 7pace sends
        // (no secrets: the token is only in the request header).
        let text = String::from_utf8_lossy(&bytes);
        let text = truncate(&text, 4000);
        tracing::debug!(path, body = %text, "7pace tracking response");
        serde_json::from_slice(&bytes).map_err(|e| {
            tracing::warn!(path, error = %e, body = %text, "unexpected 7pace tracking response");
            e.into()
        })
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&(impl Serialize + ?Sized)>,
    ) -> Result<Vec<u8>, SevenPaceError> {
        let url = self.url(path, query);
        let body = body.map(serde_json::to_vec).transpose()?;
        let mut attempt = 0;
        loop {
            attempt += 1;
            let mut builder = self
                .http
                .request(method.clone(), url.clone())
                .header(header::AUTHORIZATION, self.auth.authorization_header())
                .header(header::ACCEPT, "application/json");
            if let Some(b) = &body {
                builder = builder
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(b.clone());
            }
            tracing::debug!(%method, path = url.path(), attempt, "7pace request");
            let resp = builder.send().await?;
            let status = resp.status();
            let retry_after = resp
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse().ok());
            let bytes = resp.bytes().await?;
            if status.is_success() {
                return Ok(bytes.to_vec());
            }
            if status.is_server_error() && method == Method::GET && attempt < 2 {
                tracing::warn!(%status, path = url.path(), "transient 7pace error, retrying");
                tokio::time::sleep(Duration::from_millis(800)).await;
                continue;
            }
            return Err(map_error(status, &bytes, retry_after));
        }
    }
}

/// At most `max` bytes of `s`, cut on a char boundary.
fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Maps a non-success response to [`SevenPaceError`].
pub(crate) fn map_error(
    status: StatusCode,
    body: &[u8],
    retry_after: Option<u64>,
) -> SevenPaceError {
    #[derive(serde::Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    struct Flat {
        #[serde(default)]
        error_code: Option<String>,
        #[serde(default)]
        error_description: Option<String>,
        #[serde(default)]
        error: Option<Nested>,
    }
    #[derive(serde::Deserialize, Default)]
    struct Nested {
        #[serde(default)]
        code: Option<String>,
        #[serde(default)]
        message: Option<String>,
    }
    let parsed: Flat = serde_json::from_slice(body).unwrap_or_default();
    let nested = parsed.error.unwrap_or_default();
    let code = parsed.error_code.or(nested.code);
    let message = parsed
        .error_description
        .or(nested.message)
        .or_else(|| code.clone())
        .unwrap_or_else(|| {
            status
                .canonical_reason()
                .unwrap_or("request failed")
                .to_owned()
        });
    if code.as_deref() == Some("ApiDisabled") {
        return SevenPaceError::ApiDisabled;
    }
    match status {
        StatusCode::UNAUTHORIZED => SevenPaceError::Unauthorized,
        StatusCode::FORBIDDEN => SevenPaceError::Forbidden(message),
        StatusCode::CONFLICT => SevenPaceError::Conflict(message),
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => {
            SevenPaceError::Validation(message)
        }
        StatusCode::TOO_MANY_REQUESTS => SevenPaceError::RateLimited {
            retry_after_secs: retry_after.unwrap_or(60),
        },
        _ => SevenPaceError::Api {
            status: status.as_u16(),
            code,
            message,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SevenPaceToken;

    fn client() -> SevenPaceClient {
        SevenPaceClient::new(
            SevenPaceConfig {
                base_url: Url::parse("https://media-press.timehub.7pace.com").unwrap(),
                api_version: "3.2".into(),
            },
            Arc::new(SevenPaceToken::new("t")),
        )
    }

    #[test]
    fn rest_url_has_api_version_and_filters() {
        let url = client().url(
            "rest/workLogs",
            &[("$workItemIds", "1,2".into()), ("$count", "500".into())],
        );
        assert_eq!(
            url.as_str(),
            "https://media-press.timehub.7pace.com/api/rest/workLogs?%24workItemIds=1%2C2&%24count=500&api-version=3.2"
        );
    }

    #[test]
    fn rest_url_with_date_range() {
        let url = client().url(
            "rest/workLogs",
            &[
                ("$fromTimestamp", "2026-11-08T00:00:00".into()),
                ("$toTimestamp", "2026-11-17T00:00:00".into()),
            ],
        );
        assert_eq!(
            url.as_str(),
            "https://media-press.timehub.7pace.com/api/rest/workLogs?%24fromTimestamp=2026-11-08T00%3A00%3A00&%24toTimestamp=2026-11-17T00%3A00%3A00&api-version=3.2"
        );
    }

    #[test]
    fn tracking_url() {
        assert_eq!(
            client().url("tracking/client/stopTracking/0", &[]).as_str(),
            "https://media-press.timehub.7pace.com/api/tracking/client/stopTracking/0?api-version=3.2"
        );
    }

    #[test]
    fn maps_spec_error_body() {
        let body = br#"{"status":409,"errorCode":"TimeIntervalIntersection","errorDescription":"The week is approved","fields":[]}"#;
        match map_error(StatusCode::CONFLICT, body, None) {
            SevenPaceError::Conflict(m) => assert_eq!(m, "The week is approved"),
            e => panic!("{e:?}"),
        }
        let legacy = br#"{"error":{"code":"ApiDisabled","message":"migrating"}}"#;
        assert!(matches!(
            map_error(StatusCode::FORBIDDEN, legacy, None),
            SevenPaceError::ApiDisabled
        ));
        assert!(matches!(
            map_error(StatusCode::UNAUTHORIZED, b"", None),
            SevenPaceError::Unauthorized
        ));
        assert!(matches!(
            map_error(StatusCode::TOO_MANY_REQUESTS, b"", Some(7)),
            SevenPaceError::RateLimited {
                retry_after_secs: 7
            }
        ));
    }
}
