//! User profile pictures (Graph, on the identity host).
//!
//! - `GET vssps/{org}/_apis/graph/Subjects/{descriptor}/avatars?size=large` (`7.1-preview.1`) —
//!   the picture as base64 in JSON. The `MemberAvatars` URL that ADO puts in identity `imageUrl`s
//!   only accepts a browser sign-in, not a PAT, so it can't be used. Needs a PAT with
//!   **Graph (Read)**; without it this returns `Unauthorized` and the app shows initials.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqwest::Method;
use serde::Deserialize;

use crate::client::{Request, Scope};
use crate::{AdoClient, AdoError};

const GRAPH_API_VERSION: &str = "7.1-preview.1";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AvatarDto {
    value: String,
    #[serde(default)]
    is_auto_generated: bool,
}

impl AdoClient {
    /// The photo of the user with this subject descriptor (`aad.…`) as image bytes. `None` when
    /// they have no photo (ADO would return a generated initials image).
    pub async fn member_avatar(&self, descriptor: &str) -> Result<Option<Vec<u8>>, AdoError> {
        let path = format!("graph/Subjects/{descriptor}/avatars");
        let dto: AvatarDto = self
            .send(
                Request::new(Method::GET, Scope::Org, &path)
                    .query("size", "large")
                    .api_version(GRAPH_API_VERSION)
                    .vssps(),
            )
            .await?;
        if dto.is_auto_generated {
            return Ok(None);
        }
        STANDARD
            .decode(dto.value.as_bytes())
            .map(Some)
            .map_err(|e| AdoError::Api {
                status: 200,
                type_key: None,
                message: format!("avatar is not base64: {e}"),
            })
    }
}
