//! `POST {project}/{team}/_apis/wit/wiql` — returns ids only. Use `timePrecision=true` for
//! `ChangedDate` deltas. Results cap around 20k: page by `[System.Id] > last`.

use reqwest::Method;

use crate::client::{Request, Scope};
use crate::models::{WiqlRequest, WiqlResponse};
use crate::{AdoClient, AdoError};

impl AdoClient {
    /// Runs a WIQL query in team context and returns the matching ids in query order.
    pub async fn wiql(&self, project: &str, team: &str, query: &str) -> Result<Vec<u32>, AdoError> {
        let body = WiqlRequest {
            query: query.to_owned(),
        };
        let resp: WiqlResponse = self
            .send(
                Request::new(Method::POST, Scope::Team(project, team), "wit/wiql")
                    .query("timePrecision", "true")
                    .json(&body)?,
            )
            .await?;
        Ok(resp.work_items.into_iter().map(|w| w.id).collect())
    }
}
