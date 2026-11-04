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
            let mut skip = 0;
            loop {
                let page: Vec<WorkLogDto> = self
                    .send_rest(
                        Method::GET,
                        "rest/workLogs",
                        &[
                            ("$workItemIds", ids.clone()),
                            ("$count", PAGE.to_string()),
                            ("$skip", skip.to_string()),
                            ("$includeEditable", "true".into()),
                        ],
                        None::<&()>,
                    )
                    .await?;
                let n = page.len();
                out.extend(page);
                if n < PAGE {
                    break;
                }
                skip += n;
            }
        }
        Ok(out)
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
        let _: serde_json::Value = self
            .send_rest(Method::DELETE, &path, &[], None::<&()>)
            .await?;
        Ok(())
    }
}
