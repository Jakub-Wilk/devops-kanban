//! `GET {project}/{team}/_apis/work/backlogconfiguration` — backlog levels, WIT ↔ state-category
//! mapping, order field, `bugsBehavior`.

use reqwest::Method;

use crate::client::{Request, Scope};
use crate::models::BacklogConfigurationDto;
use crate::{AdoClient, AdoError};

impl AdoClient {
    pub async fn backlog_configuration(
        &self,
        project: &str,
        team: &str,
    ) -> Result<BacklogConfigurationDto, AdoError> {
        self.send(Request::new(
            Method::GET,
            Scope::Team(project, team),
            "work/backlogconfiguration",
        ))
        .await
    }
}
