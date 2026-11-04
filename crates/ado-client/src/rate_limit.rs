//! Cooperative rate limiting.
//!
//! ADO allows ~200 TSTU per sliding 5-minute window per user. Responses carry `Retry-After` and
//! `X-RateLimit-{Resource,Delay,Limit,Remaining,Reset}`. The limiter records these and makes the
//! next request wait when `Retry-After` was seen or `Remaining` is near zero.

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::header::HeaderMap;

/// Below this many remaining TSTUs, wait for the window to reset.
const LOW_REMAINING: f64 = 5.0;
/// Never sleep longer than this on a single request.
const MAX_WAIT: Duration = Duration::from_secs(60);

/// Shared throttle state, updated from every response.
#[derive(Debug, Default)]
pub struct RateLimiter {
    not_before: Mutex<Option<Instant>>,
}

impl RateLimiter {
    /// Sleeps until the limiter allows the next request.
    pub async fn wait(&self) {
        let until = *self.not_before.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(until) = until {
            let now = Instant::now();
            if until > now {
                let delay = (until - now).min(MAX_WAIT);
                tracing::debug!(?delay, "rate limiter delaying request");
                tokio::time::sleep(delay).await;
            }
        }
    }

    /// Records throttling headers. Returns the `Retry-After` seconds when present.
    pub fn observe(&self, headers: &HeaderMap) -> Option<u64> {
        let retry_after = header_f64(headers, "retry-after").map(|s| s.max(0.0) as u64);
        let delay = retry_after.map(Duration::from_secs).or_else(|| {
            let remaining = header_f64(headers, "x-ratelimit-remaining")?;
            if remaining > LOW_REMAINING {
                return None;
            }
            let reset = header_f64(headers, "x-ratelimit-reset")? as u64;
            let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
            Some(Duration::from_secs(reset.saturating_sub(now)))
        });
        if let Some(delay) = delay {
            let mut slot = self.not_before.lock().unwrap_or_else(|e| e.into_inner());
            *slot = Some(Instant::now() + delay.min(MAX_WAIT));
        }
        retry_after
    }
}

fn header_f64(headers: &HeaderMap, name: &str) -> Option<f64> {
    headers.get(name)?.to_str().ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn retry_after_is_reported() {
        let limiter = RateLimiter::default();
        let mut h = HeaderMap::new();
        h.insert("Retry-After", HeaderValue::from_static("7"));
        assert_eq!(limiter.observe(&h), Some(7));
        assert!(limiter.not_before.lock().unwrap().is_some());
    }

    #[test]
    fn plenty_remaining_does_not_throttle() {
        let limiter = RateLimiter::default();
        let mut h = HeaderMap::new();
        h.insert("X-RateLimit-Remaining", HeaderValue::from_static("150"));
        assert_eq!(limiter.observe(&h), None);
        assert!(limiter.not_before.lock().unwrap().is_none());
    }
}
