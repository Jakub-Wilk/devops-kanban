//! `GET {project}/{team}/_apis/work/boards[/{board}]` (+ `/cardsettings`).
//! Some teams' boards return HTTP 500 regardless of api-version; surface as a per-board error.

use reqwest::Method;

use crate::client::{Request, Scope};
use crate::models::{BoardDto, BoardReference, CardSettingsDto, ListResponse};
use crate::{AdoClient, AdoError};

impl AdoClient {
    pub async fn boards(&self, project: &str, team: &str) -> Result<Vec<BoardReference>, AdoError> {
        let list: ListResponse<BoardReference> = self
            .send(Request::new(
                Method::GET,
                Scope::Team(project, team),
                "work/boards",
            ))
            .await?;
        Ok(list.value)
    }

    pub async fn board(
        &self,
        project: &str,
        team: &str,
        board: &str,
    ) -> Result<BoardDto, AdoError> {
        let path = format!("work/boards/{board}");
        self.send(Request::new(Method::GET, Scope::Team(project, team), &path))
            .await
    }

    pub async fn card_settings(
        &self,
        project: &str,
        team: &str,
        board: &str,
    ) -> Result<CardSettingsDto, AdoError> {
        let path = format!("work/boards/{board}/cardsettings");
        self.send(Request::new(Method::GET, Scope::Team(project, team), &path))
            .await
    }
}
