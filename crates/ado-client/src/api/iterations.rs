//! `GET {project}/{team}/_apis/work/teamsettings/iterations[?$timeframe=current]`.

use reqwest::Method;

use crate::client::{Request, Scope};
use crate::models::{IterationDto, ListResponse};
use crate::{AdoClient, AdoError};

impl AdoClient {
    /// The team's iterations; `current_only` adds `$timeframe=current`.
    pub async fn iterations(
        &self,
        project: &str,
        team: &str,
        current_only: bool,
    ) -> Result<Vec<IterationDto>, AdoError> {
        let mut req = Request::new(
            Method::GET,
            Scope::Team(project, team),
            "work/teamsettings/iterations",
        );
        if current_only {
            req = req.query("$timeframe", "current");
        }
        let list: ListResponse<IterationDto> = self.send(req).await?;
        Ok(list.value)
    }
}
