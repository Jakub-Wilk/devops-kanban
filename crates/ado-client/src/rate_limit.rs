//! Cooperative rate limiting.
//!
//! ADO allows ~200 TSTU per sliding 5-minute window per user. Responses carry `Retry-After` and
//! `X-RateLimit-{Resource,Delay,Limit,Remaining,Reset}`. The limiter records these and makes the
//! next request wait when `Retry-After` was seen or `Remaining` is near zero.

/// Shared throttle state, updated from every response.
#[derive(Debug, Default)]
pub struct RateLimiter {}
