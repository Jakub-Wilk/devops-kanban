//! On-disk cache of the last loaded workspace (JSON in the platform cache dir) so the app paints
//! instantly on startup, then reconciles with a delta sync. Disabled via `cache_enabled = false`.

use std::path::PathBuf;

use crate::config::project_dirs;
use crate::error::CoreError;
use crate::workspace::Workspace;

/// Bumped when the snapshot shape changes; older files are ignored.
const VERSION: u32 = 3;

#[derive(serde::Serialize, serde::Deserialize)]
struct Snapshot {
    version: u32,
    organization: String,
    workspace: Workspace,
}

fn path(project: &str, team: &str) -> Option<PathBuf> {
    let safe = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_alphanumeric() || " -_.".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    };
    project_dirs().map(|d| {
        d.cache_dir()
            .join("boards")
            .join(safe(project))
            .join(format!("{}.json", safe(team)))
    })
}

/// The cached workspace, if present, readable and for the same organization.
pub fn load(organization: &str, project: &str, team: &str) -> Option<Workspace> {
    let bytes = std::fs::read(path(project, team)?).ok()?;
    let snap: Snapshot = serde_json::from_slice(&bytes)
        .inspect_err(|e| tracing::debug!(%e, "ignoring unreadable cache"))
        .ok()?;
    (snap.version == VERSION && snap.organization == organization).then_some(snap.workspace)
}

pub fn save(organization: &str, workspace: &Workspace) -> Result<(), CoreError> {
    let path = path(&workspace.project, &workspace.team)
        .ok_or_else(|| CoreError::Config("no cache directory".into()))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let snap = Snapshot {
        version: VERSION,
        organization: organization.to_owned(),
        workspace: workspace.clone(),
    };
    std::fs::write(path, serde_json::to_vec(&snap)?)?;
    Ok(())
}
