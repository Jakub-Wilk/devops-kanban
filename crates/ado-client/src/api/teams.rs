//! `GET _apis/projects`, `GET _apis/projects/{project}/teams`,
//! `GET {project}/{team}/_apis/work/teamsettings[/teamfieldvalues]`.

use reqwest::Method;

use crate::client::{Request, Scope};
use crate::models::{ListResponse, ProjectDto, TeamDto, TeamFieldValuesDto, TeamSettingsDto};
use crate::{AdoClient, AdoError};

impl AdoClient {
    pub async fn projects(&self) -> Result<Vec<ProjectDto>, AdoError> {
        let list: ListResponse<ProjectDto> = self
            .send(Request::new(Method::GET, Scope::Org, "projects").query("$top", 500))
            .await?;
        Ok(list.value)
    }

    pub async fn teams(&self, project: &str) -> Result<Vec<TeamDto>, AdoError> {
        let path = format!("projects/{project}/teams");
        let list: ListResponse<TeamDto> = self
            .send(Request::new(Method::GET, Scope::Org, &path).query("$top", 500))
            .await?;
        Ok(list.value)
    }

    pub async fn team_settings(
        &self,
        project: &str,
        team: &str,
    ) -> Result<TeamSettingsDto, AdoError> {
        self.send(Request::new(
            Method::GET,
            Scope::Team(project, team),
            "work/teamsettings",
        ))
        .await
    }

    pub async fn team_field_values(
        &self,
        project: &str,
        team: &str,
    ) -> Result<TeamFieldValuesDto, AdoError> {
        self.send(Request::new(
            Method::GET,
            Scope::Team(project, team),
            "work/teamsettings/teamfieldvalues",
        ))
        .await
    }
}
