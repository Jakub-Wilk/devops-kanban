//! `GET _apis/connectionData` — the identity the PAT belongs to ("Me", "Assign to me").

use reqwest::Method;

use crate::client::{Request, Scope};
use crate::models::ConnectionData;
use crate::{AdoClient, AdoError};

impl AdoClient {
    /// Also the cheapest way to validate a PAT.
    pub async fn connection_data(&self) -> Result<ConnectionData, AdoError> {
        self.send(Request::new(Method::GET, Scope::Org, "connectionData"))
            .await
    }
}
