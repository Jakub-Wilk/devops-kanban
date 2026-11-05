//! Work item type icons, as Azure DevOps draws them.
//!
//! - `GET {org}/_apis/wit/workItemIcons` — the icon ids (`icon_crown`, `icon_book`, …).
//! - `GET {org}/_apis/wit/workItemIcons/{icon}?color={hex}&v=2` with `Accept: image/svg+xml` —
//!   the icon as SVG, filled with `color`. (With JSON it only returns `{id, url}`.)

use reqwest::Method;

use crate::client::{Request, Scope};
use crate::{AdoClient, AdoError};

impl AdoClient {
    /// The icon `icon` (a type's `icon.id`) as SVG markup in `color` (hex without `#`).
    pub async fn work_item_icon_svg(&self, icon: &str, color: &str) -> Result<String, AdoError> {
        let path = format!("wit/workItemIcons/{icon}");
        self.send_text(
            Request::new(Method::GET, Scope::Org, &path)
                .query("color", color)
                .query("v", 2),
            "image/svg+xml",
        )
        .await
    }
}
