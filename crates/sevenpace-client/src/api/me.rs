//! `GET /api/rest/me` — the token's user; validates the token.

use reqwest::Method;

use crate::models::MeDto;
use crate::{SevenPaceClient, SevenPaceError};

impl SevenPaceClient {
    pub async fn me(&self) -> Result<MeDto, SevenPaceError> {
        self.send_rest(Method::GET, "rest/me", &[], None::<&()>)
            .await
    }
}
