//! On-disk cache of the last loaded board (JSON in the platform cache dir) so the app paints
//! instantly on startup, then reconciles with a delta sync. Disabled via `cache_enabled = false`.
