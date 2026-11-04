//! Background delta sync.
//!
//! Loop every `poll_interval_secs`: WIQL `[System.ChangedDate] > @last` (timePrecision) for the
//! workspace's types → `workitemsbatch` the changed ids → emit them. The receiver applies them
//! with [`Workspace::upsert`](crate::Workspace::upsert), which drops items that left the scope.
//! Skips ids with a write in flight; backs off when rate limited; stops on auth failure.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ado_client::{AdoClient, AdoError, WorkItemDto};
use time::OffsetDateTime;
use tokio::sync::mpsc::UnboundedSender;

use crate::card::CardId;
use crate::queries::Scope;

/// Ids with a local write in progress; sync leaves them alone until it completes.
pub type InFlight = Arc<Mutex<HashSet<CardId>>>;

#[derive(Debug)]
pub enum SyncEvent {
    Changed(Vec<WorkItemDto>),
    /// Transient failure; sync keeps going.
    Failed(String),
    /// PAT expired or revoked; sync stopped.
    Unauthorized,
}

pub struct SyncEngine {
    pub client: AdoClient,
    pub interval: Duration,
    pub scope: Scope,
    pub fields: Vec<String>,
    pub in_flight: InFlight,
}

impl SyncEngine {
    /// Runs until the receiver is dropped or auth fails.
    pub async fn run(self, mut since: OffsetDateTime, tx: UnboundedSender<SyncEvent>) {
        let mut delay = self.interval;
        loop {
            tokio::time::sleep(delay).await;
            if tx.is_closed() {
                return;
            }
            delay = self.interval;
            match self.tick(since).await {
                Ok((items, newest)) => {
                    since = newest.max(since);
                    if !items.is_empty() && tx.send(SyncEvent::Changed(items)).is_err() {
                        return;
                    }
                }
                Err(AdoError::Unauthorized) => {
                    let _ = tx.send(SyncEvent::Unauthorized);
                    return;
                }
                Err(AdoError::RateLimited { retry_after_secs }) => {
                    delay = self.interval * 2 + Duration::from_secs(retry_after_secs);
                    tracing::warn!(?delay, "sync rate limited, backing off");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "sync failed");
                    delay = self.interval * 2;
                    if tx.send(SyncEvent::Failed(e.to_string())).is_err() {
                        return;
                    }
                }
            }
        }
    }

    /// One delta round: changed items (minus in-flight ones) and the newest change date seen.
    pub async fn tick(
        &self,
        since: OffsetDateTime,
    ) -> Result<(Vec<WorkItemDto>, OffsetDateTime), AdoError> {
        let ids = self
            .client
            .wiql(
                &self.scope.project,
                &self.scope.team,
                &self.scope.delta_query(since),
            )
            .await?;
        let ids: Vec<CardId> = {
            let busy = self.in_flight.lock().unwrap_or_else(|e| e.into_inner());
            ids.into_iter().filter(|id| !busy.contains(id)).collect()
        };
        if ids.is_empty() {
            return Ok((Vec::new(), since));
        }
        let items = self
            .client
            .work_items_batch(&self.scope.project, &ids, &self.fields)
            .await?;
        let newest = items
            .iter()
            .filter_map(|w| w.fields.get(crate::card::fields::CHANGED_DATE)?.as_str())
            .filter_map(|s| {
                OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
            })
            .max()
            .unwrap_or(since);
        Ok((items, newest))
    }
}
