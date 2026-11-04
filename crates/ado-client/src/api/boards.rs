//! `GET {project}/{team}/_apis/work/boards[/{board}]` (+ `/columns`, `/rows`, `/cardsettings`).
//! Some teams' boards return HTTP 500 regardless of api-version; surface as a per-board error.
