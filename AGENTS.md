# AGENTS.md

Guide for agents and developers working on **devops-kanban**: a Rust + Dioxus **desktop** app
that provides a more ergonomic kanban board on top of Azure DevOps Boards.

Status: **first implementation**. The UI follows the "DevOps Board Manager" design (List of the
full backlog tree in aligned columns, kanban, timeline, two-column item dialog with description,
tags and a searchable parent; dark "Graphite" theme, Inter with IBM Plex Mono for IDs and times).
Keep new UI consistent with `crates/app/assets/main.css` and `dialogs.css` (use the spacing
`--s1…--s6` and radius `--r-sm/md/lg` tokens) and don't redesign without being asked. Row and card style is user-configurable per view (*View options* next to the tabs,
`components/view_options.rs`; `appearance` in `docs/settings.md`). 7pace time
tracking is on the kanban only (timer and add time in the card header, the user's entries in the
card's expandable details, fetched in bulk for the shown cards) plus a compact running-timer chip in
the top bar; Stop opens the time dialog and applies it to the worklog 7pace creates. The app side
is `crates/app/src/time_tracking.rs` and `components/time.rs`.

## Crate map

```
crates/app          Dioxus 0.7 desktop binary `devops-kanban` (routes, views, reactive state)
   │
   ▼
crates/kanban-core  Domain: Board/Column/Card model, MovePlan, SyncEngine, time, cache, Settings
   │
   ├──────────────────────────┐
   ▼                          ▼
crates/ado-client        crates/sevenpace-client
  ADO REST (work, wit)     7pace Timetracker REST (worklogs, timer)
```

- Dependencies point **downward only**. `ado-client`, `sevenpace-client` and `kanban-core` must
  never depend on `dioxus` or on desktop-only APIs. `sevenpace-client` uses `ado-client` only for
  the `AuthProvider` trait. That keeps them unit-testable and a future web target possible.
- Client crate models mirror the wire JSON. `kanban-core` owns the domain types. Convert at the
  boundary; do not leak DTOs into the UI.
- Do **not** use the `azure_devops_rust_api` crate (archived 2026-10-26).

Details: [docs/architecture.md](docs/architecture.md) · APIs: [docs/azure-devops-api.md](docs/azure-devops-api.md),
[docs/7pace-api.md](docs/7pace-api.md) · Settings: [docs/settings.md](docs/settings.md)

## Commands

```sh
cargo check --workspace
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo fmt --all
cargo run -p app            # runs the desktop app (bin: devops-kanban)
dx serve --desktop          # from crates/app, with hot reload (needs dioxus-cli 0.7.x)
```

Linux/WSL needs `libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev libsoup-3.0-dev`.

## Azure DevOps rules (verified against the media-press org)

These are easy to get wrong:

1. **api-version is `7.1` everywhere** except comments (`7.1-preview.4`) and
   `connectionData` (`7.1-preview`, which has no GA version). Some teams' boards and
   backlog configuration return HTTP 500 on any version (e.g. Hubert / `DEV - Hubert`). Retry
   5xx with backoff for transient outages, then report the board as unavailable rather than failing the
   whole app.
2. **Never write `System.BoardColumn`, `System.BoardColumnDone` or `System.BoardLane`.** They are
   read-only (TF401326). To move a card, patch the board's own fields from `board.fields`:
   `WEF_<guid>_Kanban.Column`, `…Kanban.Column.Done` and `…Kanban.Lane`. The GUID differs per board,
   so always read it and never hardcode it. Also patch `System.State` when the target column's
   `stateMappings[type]` differs from the card's current state.
3. **Every work item PATCH starts with `{"op":"test","path":"/rev","value":<rev>}`.** A failure
   means a concurrent edit. Surface it as `AdoError::Conflict`, then refetch and do not retry
   blindly.
4. **Ordering goes through `PATCH …/_apis/work/workitemsorder`** (`previousId`/`nextId`). Never write
   `Microsoft.VSTS.Common.StackRank` directly. Read the order field name from
   `backlogconfiguration.backlogFields.typeFields.Order`.
5. **WIQL returns ids only.** Hydrate them with `workitemsbatch`, at most **200 ids per call**, and
   always pass an explicit `fields` list.
6. **States are per process and customised.** Our org has New, Active, Blocked, Paused, Ready, Done,
   TEST-ACTIVE, TEST-FAILED and Closed. Never hardcode state names; use board `stateMappings` and
   `workItemTypeMappedStates` (categories Proposed/InProgress/Resolved/Completed).
7. **`bugsBehavior` varies.** In our org it is `asTasks`, so Bugs appear with Tasks, not on the Stories board.
   Derive a board's work item types from `backlogconfiguration`.
8. **Respect rate limits.** Honour `Retry-After` and `X-RateLimit-*`, and back off on 429. Polling must stay
   cheap: a delta WIQL on `System.ChangedDate`, never a full reload.

## 7pace rules

Time tracking goes through 7pace Timetracker, a **separate service**. See [docs/7pace-api.md](docs/7pace-api.md).

1. **Separate host and token.** The host is `https://{org}.timehub.7pace.com`, with `api-version=3.2` on every REST call.
   The ADO PAT is **not** accepted there; never send it to 7pace, and never send the 7pace token to ADO.
2. **Units:** `length` and `billableLength` are **seconds**. `timeStamp` is the work's **start** in the
   **user's local time**, not UTC. Convert at the `kanban-core::time` boundary.
3. **Card totals come from `Microsoft.VSTS.Scheduling.CompletedWork`**, which 7pace keeps in sync. Fetch
   it with the other card fields. Never poll 7pace for totals; its rate limits are far tighter than ADO's.
4. **`$workItemIds` takes at most 100 ids**, and `$count` at most 500 per page.
5. **No optimistic concurrency.** A 409 means conflict. After any write, refetch the card for the new total.
6. The 7pace UI or desktop client can also start and stop the timer. Read `tracking/client/current`
   before showing the timer's state.

## Security

- The PAT lives **only** in the OS keyring (service `devops-kanban`, account = organization).
  Never write it to `config.toml`, the cache, logs or error messages. `PatAuth`'s `Debug` redacts
  it; keep it that way.
- The 7pace API token follows the same rules (keyring account `7pace:<organization>`;
  `SevenPaceToken`'s `Debug` redacts it).
- The `DEVOPS_KANBAN_PAT` and `DEVOPS_KANBAN_7PACE_TOKEN` env overrides are for local development only.

## Conventions

- Edition 2024, stable toolchain. Workspace-level dependency versions live in the root
  `Cargo.toml` `[workspace.dependencies]`; crates use `dep.workspace = true`.
- Every module starts with a `//!` doc comment stating its responsibility. Keep it accurate.
- Errors: `thiserror` enums per crate. No `unwrap()`/`expect()` outside tests and `main`.
- Logging: `tracing`. Control the level with `RUST_LOG` or the `log_level` setting.
- Dioxus: 0.7 APIs (`use_signal`, `use_resource`, `use_reactive!`). Pin `0.7`, since 0.8 is
  alpha with breaking Signals changes. For drag and drop, use HTML5 drag events and keep the dragged
  card id in a signal, not in `dataTransfer`.
- Tests: unit-test `kanban-core` logic (move planning, column resolution) with fixture JSON
  captured from real ADO responses. No tests hit the live API.
- Commits: one short subject line, no body unless asked, no AI attribution or co-author trailers.
