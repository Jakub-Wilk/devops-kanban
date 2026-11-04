//! `GET /api/rest/activityTypes` — the org's activity types and the system default.

use reqwest::Method;

use crate::models::ActivityTypesDto;
use crate::{SevenPaceClient, SevenPaceError};

impl SevenPaceClient {
    pub async fn activity_types(&self) -> Result<ActivityTypesDto, SevenPaceError> {
        self.send_rest(Method::GET, "rest/activityTypes", &[], None::<&()>)
            .await
    }
}
