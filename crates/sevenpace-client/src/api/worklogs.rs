//! Worklogs (time entries).
//!
//! - `GET /api/rest/workLogs` — own entries. Filters: `$workItemIds` (≤100, comma-separated),
//!   `$fromTimestamp` / `$toTimestamp`, `$count` (≤500) + `$skip`, `$includeEditable`.
//! - `GET /api/rest/workLogs/all` — everyone's; needs the Product, Budget or Administrator role
//!   (not used: cards list only the user's own entries).
//! - `POST /api/rest/workLogs` — create one (log time against a work item).
//! - `PATCH|DELETE /api/rest/workLogs/{id}` — edit / delete by uuid. 7pace has no optimistic
//!   concurrency; a 409 is the only conflict signal.

use reqwest::Method;

use crate::models::{WorkLogDto, WorkLogWrite};
use crate::{SevenPaceClient, SevenPaceError};

/// 7pace's `$workItemIds` limit.
pub const MAX_IDS: usize = 100;
/// 7pace's `$count` limit.
const PAGE: usize = 500;

impl SevenPaceClient {
    /// The user's own worklogs on `work_item_ids`, with `isCanEdit` / `isCanDelete`.
    pub async fn worklogs(&self, work_item_ids: &[u32]) -> Result<Vec<WorkLogDto>, SevenPaceError> {
        let mut out = Vec::new();
        for chunk in work_item_ids.chunks(MAX_IDS) {
            let ids = chunk
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            out.extend(self.paged(vec![("$workItemIds", ids)]).await?);
        }
        Ok(out)
    }

    /// The user's own worklogs whose timestamp is between `from` and `to` (both
    /// `2026-11-09T00:00:00`, user-local), with `isCanEdit` / `isCanDelete`. Callers should query
    /// a little wider than they need and filter on the parsed start: how 7pace reads these
    /// bounds (local or UTC) isn't documented.
    pub async fn worklogs_between(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<WorkLogDto>, SevenPaceError> {
        self.paged(vec![
            ("$fromTimestamp", from.to_owned()),
            ("$toTimestamp", to.to_owned()),
        ])
        .await
    }

    /// `GET rest/workLogs` with `filters`, every page of `$count` = 500.
    async fn paged(&self, filters: Vec<(&str, String)>) -> Result<Vec<WorkLogDto>, SevenPaceError> {
        let mut out = Vec::new();
        let mut skip = 0;
        loop {
            let mut query = filters.clone();
            query.push(("$count", PAGE.to_string()));
            query.push(("$skip", skip.to_string()));
            query.push(("$includeEditable", "true".into()));
            let page: Vec<WorkLogDto> = self
                .send_rest(Method::GET, "rest/workLogs", &query, None::<&()>)
                .await?;
            let n = page.len();
            out.extend(page);
            if n < PAGE {
                return Ok(out);
            }
            skip += n;
        }
    }

    pub async fn create_worklog(&self, body: &WorkLogWrite) -> Result<WorkLogDto, SevenPaceError> {
        self.send_rest(Method::POST, "rest/workLogs", &[], Some(body))
            .await
    }

    pub async fn update_worklog(
        &self,
        id: &str,
        body: &WorkLogWrite,
    ) -> Result<WorkLogDto, SevenPaceError> {
        let path = format!("rest/workLogs/{id}");
        self.send_rest(Method::PATCH, &path, &[], Some(body)).await
    }

    pub async fn delete_worklog(&self, id: &str) -> Result<(), SevenPaceError> {
        let path = format!("rest/workLogs/{id}");
        // The response body is empty: parsing it as `{ data }` failed although the entry was
        // deleted ("EOF while parsing a value").
        self.send_rest_no_content(Method::DELETE, &path, &[], None::<&()>)
            .await
    }
}
