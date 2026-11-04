//! Background delta sync.
//!
//! Loop every `poll_interval_secs`: WIQL `[System.ChangedDate] > @last` (timePrecision) scoped to
//! the board's types + team area paths → `workitemsbatch` the changed ids → emit updates. Pauses
//! while a move is in flight for the same card; backs off when rate limited.

use std::time::Duration;

use ado_client::AdoClient;

pub struct SyncEngine {
    pub client: AdoClient,
    pub interval: Duration,
}
