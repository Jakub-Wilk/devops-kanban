//! `IdentityRef` as returned in `System.AssignedTo` etc. (`displayName`, `uniqueName`, `id`,
//! `imageUrl`), plus the signed-in user from `connectionData`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityRef {
    #[serde(default)]
    pub id: Option<String>,
    pub display_name: String,
    #[serde(default)]
    pub unique_name: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
}

/// `GET _apis/connectionData`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionData {
    pub authenticated_user: AuthenticatedUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatedUser {
    pub id: String,
    #[serde(default)]
    pub provider_display_name: Option<String>,
    #[serde(default)]
    pub custom_display_name: Option<String>,
    #[serde(default)]
    pub properties: serde_json::Value,
}

impl AuthenticatedUser {
    pub fn display_name(&self) -> String {
        self.custom_display_name
            .clone()
            .or_else(|| self.provider_display_name.clone())
            .unwrap_or_default()
    }

    /// The account (usually the e-mail), which is what `System.AssignedTo` accepts.
    pub fn account(&self) -> Option<String> {
        self.properties
            .get("Account")?
            .get("$value")?
            .as_str()
            .map(str::to_owned)
    }
}
