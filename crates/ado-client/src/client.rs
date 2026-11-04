//! `AdoClient`: owns the `reqwest::Client`, base URL, auth and rate-limit state.
//!
//! All endpoint functions in [`crate::api`] are inherent methods on `AdoClient` grouped by area.
//! Responsibilities here: URL building (`{base}/{org}/{project}/{team}/_apis/...`),
//! `api-version` query param, auth header, JSON (de)serialization, mapping HTTP failures to
//! [`AdoError`](crate::AdoError), retrying transient 5xx, and feeding response headers to
//! [`RateLimiter`].

use std::sync::Arc;
use std::time::Duration;

use reqwest::{Method, StatusCode, header};
use serde::Serialize;
use serde::de::DeserializeOwned;
use url::Url;

use crate::{AdoError, auth::AuthProvider, rate_limit::RateLimiter};

/// Attempts for a request that keeps failing with 5xx (some teams' boards always do).
const MAX_ATTEMPTS: u32 = 3;
const JSON_PATCH: &str = "application/json-patch+json";

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

/// Where an `_apis` path lives.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Scope<'a> {
    Org,
    Project(&'a str),
    Team(&'a str, &'a str),
}

/// One request, before it is sent.
pub(crate) struct Request<'a> {
    pub method: Method,
    pub scope: Scope<'a>,
    /// Path below `_apis/`, `/`-separated; each segment is URL-encoded.
    pub path: &'a str,
    pub query: Vec<(&'a str, String)>,
    pub api_version: Option<&'a str>,
    pub body: Option<serde_json::Value>,
    pub json_patch: bool,
    /// Work item id a failed `test /rev` refers to.
    pub conflict_id: Option<u32>,
}

impl<'a> Request<'a> {
    pub fn new(method: Method, scope: Scope<'a>, path: &'a str) -> Self {
        Self {
            method,
            scope,
            path,
            query: Vec::new(),
            api_version: None,
            body: None,
            json_patch: false,
            conflict_id: None,
        }
    }

    pub fn query(mut self, key: &'a str, value: impl ToString) -> Self {
        self.query.push((key, value.to_string()));
        self
    }

    pub fn api_version(mut self, version: &'a str) -> Self {
        self.api_version = Some(version);
        self
    }

    pub fn json(mut self, body: &impl Serialize) -> Result<Self, AdoError> {
        self.body = Some(serde_json::to_value(body)?);
        Ok(self)
    }

    pub fn json_patch(
        mut self,
        body: &impl Serialize,
        conflict_id: Option<u32>,
    ) -> Result<Self, AdoError> {
        self.body = Some(serde_json::to_value(body)?);
        self.json_patch = true;
        self.conflict_id = conflict_id;
        Ok(self)
    }
}

impl AdoClient {
    pub fn new(config: ClientConfig, auth: Arc<dyn AuthProvider>) -> Self {
        let max_batch = config.max_batch.clamp(1, 200);
        Self {
            http: reqwest::Client::builder()
                .user_agent(concat!("devops-kanban/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
            config: Arc::new(ClientConfig {
                max_batch,
                ..config
            }),
            auth,
            rate_limiter: Arc::new(RateLimiter::default()),
        }
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// The REST URL of a work item, as used in relation links.
    pub fn work_item_url(&self, id: u32) -> String {
        let mut url = self.config.base_url.clone();
        if let Ok(mut seg) = url.path_segments_mut() {
            seg.pop_if_empty()
                .push(&self.config.organization)
                .extend(["_apis", "wit", "workItems"])
                .push(&id.to_string());
        }
        url.to_string()
    }

    pub(crate) fn url(&self, req: &Request<'_>) -> Url {
        let mut url = self.config.base_url.clone();
        if let Ok(mut seg) = url.path_segments_mut() {
            seg.pop_if_empty().push(&self.config.organization);
            match req.scope {
                Scope::Org => {}
                Scope::Project(p) => {
                    seg.push(p);
                }
                Scope::Team(p, t) => {
                    seg.push(p).push(t);
                }
            }
            seg.push("_apis");
            seg.extend(req.path.split('/').filter(|s| !s.is_empty()));
        }
        {
            let mut q = url.query_pairs_mut();
            for (k, v) in &req.query {
                q.append_pair(k, v);
            }
            q.append_pair(
                "api-version",
                req.api_version.unwrap_or(&self.config.api_version),
            );
        }
        url
    }

    /// Sends the request, retrying 5xx with backoff, and decodes the JSON response.
    pub(crate) async fn send<T: DeserializeOwned>(&self, req: Request<'_>) -> Result<T, AdoError> {
        let url = self.url(&req);
        let mut attempt = 0;
        loop {
            attempt += 1;
            self.rate_limiter.wait().await;
            let mut builder = self
                .http
                .request(req.method.clone(), url.clone())
                .header(header::AUTHORIZATION, self.auth.authorization_header())
                .header(header::ACCEPT, "application/json");
            if let Some(body) = &req.body {
                let content_type = if req.json_patch {
                    JSON_PATCH
                } else {
                    "application/json"
                };
                builder = builder
                    .header(header::CONTENT_TYPE, content_type)
                    .body(serde_json::to_vec(body)?);
            }
            tracing::debug!(method = %req.method, path = url.path(), attempt, "ado request");
            let resp = builder.send().await?;
            let status = resp.status();
            let retry_after = self.rate_limiter.observe(resp.headers());
            let is_json = resp
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.contains("json"));
            let bytes = resp.bytes().await?;

            if status.is_success() && status != StatusCode::NON_AUTHORITATIVE_INFORMATION {
                if !is_json && !bytes.is_empty() {
                    tracing::warn!(%status, path = url.path(), "non-JSON success, treating as sign-in page");
                    return Err(AdoError::Unauthorized);
                }
                let body: &[u8] = if bytes.is_empty() { b"null" } else { &bytes };
                return Ok(serde_json::from_slice(body)?);
            }
            if status.is_server_error() && attempt < MAX_ATTEMPTS {
                let delay = Duration::from_millis(500 * 2u64.pow(attempt - 1));
                tracing::warn!(%status, ?delay, path = url.path(), "transient ADO error, retrying");
                tokio::time::sleep(delay).await;
                continue;
            }
            tracing::debug!(%status, path = url.path(), "ado error response");
            return Err(map_error(
                status,
                is_json,
                &bytes,
                retry_after,
                req.conflict_id,
            ));
        }
    }
}

/// Maps a non-success response to [`AdoError`].
pub(crate) fn map_error(
    status: StatusCode,
    is_json: bool,
    body: &[u8],
    retry_after: Option<u64>,
    conflict_id: Option<u32>,
) -> AdoError {
    #[derive(serde::Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    struct ErrorBody {
        #[serde(default)]
        message: String,
        #[serde(default)]
        type_key: Option<String>,
    }
    let parsed: ErrorBody = if is_json {
        serde_json::from_slice(body).unwrap_or_default()
    } else {
        ErrorBody::default()
    };
    let message = if parsed.message.is_empty() {
        status
            .canonical_reason()
            .unwrap_or("request failed")
            .to_owned()
    } else {
        parsed.message
    };

    if let Some(id) = conflict_id {
        let rev_mismatch = matches!(
            status,
            StatusCode::CONFLICT | StatusCode::PRECONDITION_FAILED
        ) || parsed
            .type_key
            .as_deref()
            .is_some_and(|k| k.contains("RevisionMismatch") || k.contains("TestOperation"))
            || message.contains("TF26071")
            || (status == StatusCode::BAD_REQUEST && message.contains("/rev"));
        if rev_mismatch {
            return AdoError::Conflict { id };
        }
    }
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::NON_AUTHORITATIVE_INFORMATION => {
            AdoError::Unauthorized
        }
        StatusCode::FORBIDDEN => AdoError::Forbidden(message),
        StatusCode::NOT_FOUND => AdoError::NotFound(message),
        StatusCode::TOO_MANY_REQUESTS => AdoError::RateLimited {
            retry_after_secs: retry_after.unwrap_or(30),
        },
        _ => AdoError::Api {
            status: status.as_u16(),
            type_key: parsed.type_key,
            message,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PatAuth;

    fn client() -> AdoClient {
        AdoClient::new(
            ClientConfig {
                base_url: Url::parse("https://dev.azure.com").unwrap(),
                organization: "media-press".into(),
                api_version: "7.1".into(),
                max_batch: 500,
            },
            Arc::new(PatAuth::new("x")),
        )
    }

    #[test]
    fn team_url_encodes_segments() {
        let c = client();
        let req = Request::new(
            Method::GET,
            Scope::Team("IT Finance", "IT Finance Team"),
            "work/boards/Stories",
        );
        assert_eq!(
            c.url(&req).as_str(),
            "https://dev.azure.com/media-press/IT%20Finance/IT%20Finance%20Team/_apis/work/boards/Stories?api-version=7.1"
        );
    }

    #[test]
    fn preview_version_overrides_default() {
        let c = client();
        let req = Request::new(Method::GET, Scope::Project("P"), "wit/workItems/1/comments")
            .api_version("7.1-preview.4");
        assert!(c.url(&req).as_str().ends_with("api-version=7.1-preview.4"));
    }

    #[test]
    fn max_batch_is_capped() {
        assert_eq!(client().config().max_batch, 200);
    }

    #[test]
    fn work_item_url() {
        assert_eq!(
            client().work_item_url(42),
            "https://dev.azure.com/media-press/_apis/wit/workItems/42"
        );
    }

    #[test]
    fn maps_statuses() {
        assert!(matches!(
            map_error(StatusCode::UNAUTHORIZED, false, b"", None, None),
            AdoError::Unauthorized
        ));
        assert!(matches!(
            map_error(StatusCode::TOO_MANY_REQUESTS, false, b"", Some(12), None),
            AdoError::RateLimited {
                retry_after_secs: 12
            }
        ));
        let body = br#"{"message":"TF401326: Field Board Column cannot be updated","typeKey":"RuleValidationException"}"#;
        match map_error(StatusCode::BAD_REQUEST, true, body, None, Some(5)) {
            AdoError::Api {
                status, message, ..
            } => {
                assert_eq!(status, 400);
                assert!(message.starts_with("TF401326"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn failed_rev_test_is_conflict() {
        let body = br#"{"message":"TF26071: This work item has been changed by someone else since you opened it.","typeKey":"WorkItemRevisionMismatchException"}"#;
        assert!(matches!(
            map_error(StatusCode::BAD_REQUEST, true, body, None, Some(9)),
            AdoError::Conflict { id: 9 }
        ));
    }
}
