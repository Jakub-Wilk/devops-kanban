//! `GET _apis/connectionData` — the identity the PAT belongs to ("Me", "Assign to me").
//! Preview-only on every api-version, so it pins `7.1-preview`.

use reqwest::Method;

use crate::client::{Request, Scope};
use crate::models::ConnectionData;
use crate::{AdoClient, AdoError};

const CONNECTION_DATA_API_VERSION: &str = "7.1-preview";

impl AdoClient {
    /// Also the cheapest way to validate a PAT.
    pub async fn connection_data(&self) -> Result<ConnectionData, AdoError> {
        self.send(
            Request::new(Method::GET, Scope::Org, "connectionData")
                .api_version(CONNECTION_DATA_API_VERSION),
        )
        .await
    }
}
