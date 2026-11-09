//! Work item reads and writes.
//!
//! - `POST {project}/_apis/wit/workitemsbatch` — ≤200 ids per call, explicit `fields` (also
//!   org-scoped, `{org}/_apis/wit/workitemsbatch`, for items across projects).
//! - `PATCH {project}/_apis/wit/workitems/{id}` — `application/json-patch+json`; first op is
//!   always `test /rev` for optimistic concurrency.
//! - `POST {project}/_apis/wit/workitems/${type}` — create.
//! - `GET {project}/_apis/wit/workitemtypes` — type colors.
//! - `GET {project}/_apis/wit/workitemtypes/{type}/states` — state colors + categories.

use reqwest::Method;

use crate::client::{Request, Scope};
use crate::models::{
    ListResponse, PatchDocument, WorkItemDto, WorkItemStateDto, WorkItemTypeDto,
    WorkItemsBatchRequest,
};
use crate::{AdoClient, AdoError};

impl AdoClient {
    /// Hydrates `ids` in chunks of `max_batch`, preserving the input order. Missing ids are omitted.
    pub async fn work_items_batch(
        &self,
        project: &str,
        ids: &[u32],
        fields: &[String],
    ) -> Result<Vec<WorkItemDto>, AdoError> {
        self.batch(Scope::Project(project), ids, fields).await
    }

    /// Like [`Self::work_items_batch`], across every project of the organization (for items
    /// that belong to other projects, such as the ones time was logged on).
    pub async fn work_items_batch_org(
        &self,
        ids: &[u32],
        fields: &[String],
    ) -> Result<Vec<WorkItemDto>, AdoError> {
        self.batch(Scope::Org, ids, fields).await
    }

    async fn batch(
        &self,
        scope: Scope<'_>,
        ids: &[u32],
        fields: &[String],
    ) -> Result<Vec<WorkItemDto>, AdoError> {
        let mut out = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(self.config.max_batch) {
            let body = WorkItemsBatchRequest {
                ids: chunk.to_vec(),
                fields: fields.to_vec(),
                error_policy: "omit".into(),
            };
            let list: ListResponse<Option<WorkItemDto>> = self
                .send(Request::new(Method::POST, scope, "wit/workitemsbatch").json(&body)?)
                .await?;
            let mut by_id: std::collections::HashMap<u32, WorkItemDto> = list
                .value
                .into_iter()
                .flatten()
                .map(|w| (w.id, w))
                .collect();
            out.extend(chunk.iter().filter_map(|id| by_id.remove(id)));
        }
        Ok(out)
    }

    /// One work item with all fields and relations.
    pub async fn work_item(&self, project: &str, id: u32) -> Result<WorkItemDto, AdoError> {
        let path = format!("wit/workitems/{id}");
        self.send(
            Request::new(Method::GET, Scope::Project(project), &path).query("$expand", "relations"),
        )
        .await
    }

    /// Applies `patch`. A failed `test /rev` surfaces as [`AdoError::Conflict`].
    pub async fn update_work_item(
        &self,
        project: &str,
        id: u32,
        patch: &PatchDocument,
    ) -> Result<WorkItemDto, AdoError> {
        let path = format!("wit/workitems/{id}");
        self.send(
            Request::new(Method::PATCH, Scope::Project(project), &path)
                .json_patch(patch, Some(id))?,
        )
        .await
    }

    pub async fn create_work_item(
        &self,
        project: &str,
        work_item_type: &str,
        patch: &PatchDocument,
    ) -> Result<WorkItemDto, AdoError> {
        let path = format!("wit/workitems/${work_item_type}");
        self.send(
            Request::new(Method::POST, Scope::Project(project), &path).json_patch(patch, None)?,
        )
        .await
    }

    /// Every work item type of the project. The response is large (it carries each type's form
    /// layout), so call it once per load.
    pub async fn work_item_types(&self, project: &str) -> Result<Vec<WorkItemTypeDto>, AdoError> {
        let list: ListResponse<WorkItemTypeDto> = self
            .send(Request::new(
                Method::GET,
                Scope::Project(project),
                "wit/workitemtypes",
            ))
            .await?;
        Ok(list.value)
    }

    pub async fn work_item_type_states(
        &self,
        project: &str,
        work_item_type: &str,
    ) -> Result<Vec<WorkItemStateDto>, AdoError> {
        let path = format!("wit/workitemtypes/{work_item_type}/states");
        let list: ListResponse<WorkItemStateDto> = self
            .send(Request::new(Method::GET, Scope::Project(project), &path))
            .await?;
        Ok(list.value)
    }
}
