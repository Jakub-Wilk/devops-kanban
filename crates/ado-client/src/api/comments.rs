//! `GET|POST {project}/_apis/wit/workItems/{id}/comments` (preview api-version `7.1-preview.4`).

use reqwest::Method;
use serde_json::json;

use crate::client::{Request, Scope};
use crate::models::{CommentDto, CommentList};
use crate::{AdoClient, AdoError};

const COMMENTS_API_VERSION: &str = "7.1-preview.4";

impl AdoClient {
    pub async fn comments(&self, project: &str, id: u32) -> Result<Vec<CommentDto>, AdoError> {
        let path = format!("wit/workItems/{id}/comments");
        let list: CommentList = self
            .send(
                Request::new(Method::GET, Scope::Project(project), &path)
                    .api_version(COMMENTS_API_VERSION),
            )
            .await?;
        Ok(list.comments)
    }

    pub async fn add_comment(
        &self,
        project: &str,
        id: u32,
        text: &str,
    ) -> Result<CommentDto, AdoError> {
        let path = format!("wit/workItems/{id}/comments");
        self.send(
            Request::new(Method::POST, Scope::Project(project), &path)
                .api_version(COMMENTS_API_VERSION)
                .json(&json!({ "text": text }))?,
        )
        .await
    }
}
