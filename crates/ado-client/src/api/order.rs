//! `PATCH {project}/{team}/_apis/work/workitemsorder` — reorder via `previousId`/`nextId`.
//! Never write `Microsoft.VSTS.Common.StackRank` directly.

use reqwest::Method;

use crate::client::{Request, Scope};
use crate::models::{ListResponse, ReorderRequest, ReorderResult};
use crate::{AdoClient, AdoError};

impl AdoClient {
    /// Places `id` between `previous` and `next` (0 = none). Returns the new order values.
    pub async fn reorder(
        &self,
        project: &str,
        team: &str,
        id: u32,
        previous: Option<u32>,
        next: Option<u32>,
    ) -> Result<Vec<ReorderResult>, AdoError> {
        let body = ReorderRequest {
            ids: vec![id],
            previous_id: previous.unwrap_or(0),
            next_id: next.unwrap_or(0),
            parent_id: 0,
            iteration_path: String::new(),
        };
        let list: ListResponse<ReorderResult> = self
            .send(
                Request::new(
                    Method::PATCH,
                    Scope::Team(project, team),
                    "work/workitemsorder",
                )
                .json(&body)?,
            )
            .await?;
        Ok(list.value)
    }
}
