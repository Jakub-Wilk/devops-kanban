# Architecture

## Goals and non-goals

- **Goal:** a fast, local-first desktop kanban over Azure DevOps (ADO) work items. Azure DevOps
  stays the single source of truth. The app is a client and adds no new data model.
- **Goal:** respect each team's existing board configuration (columns, state mappings, WIP limits,
  swimlanes, card fields) instead of redefining it.
- **Non-goal (for now):** web/mobile targets, Entra sign-in, and
  editing board configuration (columns, rules).

## Layers

```
┌───────────────────────────── crates/app (Dioxus desktop) ─────────────────────────────┐
│ routes.rs   Setup · Board{project,team,board} · Settings                              │
│ views/      one component per route         components/  cards, columns, dialogs      │
│ state.rs    AppState signals: settings, workspace, drag state, in-flight writes       │
│ services.rs builds client, runs loads/moves/sync as tasks, optimistic update/rollback │
└──────────────────────────────────────────┬────────────────────────────────────────────┘
                                           │ domain types only
┌──────────────────────────── crates/kanban-core ─────────────────────▼─────────────────┐
│ board.rs  Board/Column/Lane/BoardFields   card.rs  Card                               │
│ move_plan.rs  drop → field patches + reorder   sync.rs  SyncEngine (delta polling)    │
│ time.rs   TimeEntry, 7pace read/write paths    cache.rs  last board snapshot on disk  │
│ config.rs Settings + keyring                                                          │
└───────────────────┬───────────────────────────────────────────────┬───────────────────┘
                    │ DTOs                                          │ DTOs
┌───────────────── crates/ado-client ──────▼──────────┐ ┌──── crates/sevenpace-client ──▼───┐
│ client.rs AdoClient (reqwest, api-version, auth)    │ │ client.rs SevenPaceClient          │
│ auth.rs   AuthProvider trait, PatAuth  ◄────────────┼─┤ auth.rs   SevenPaceToken (Bearer)  │
│ api/*  boards · backlog · teams · iterations · wiql │ │ api/*  worklogs · tracking ·       │
│        workitems · comments · order                 │ │        activity_types · me         │
│ models/* DTOs  error.rs  rate_limit.rs              │ │ models.rs  error.rs                │
└─────────────────────────────────────────────────────┘ └────────────────────────────────────┘
        dev.azure.com (ADO PAT)                            {org}.timehub.7pace.com (7pace token)
```

Dependencies only point downward. The client crates and `kanban-core` have no Dioxus or desktop
dependencies, so they can be unit-tested headless and reused by a future web/fullstack target.

## Loading a board

```
Settings(org, project, team, board) + PAT (keyring)
  │
  ├─ GET  {p}/{t}/_apis/work/backlogconfiguration      → board's WITs, state categories, Order field, bugsBehavior
  ├─ GET  {p}/{t}/_apis/work/boards/{board}            → columns, rows, fields.{column,done,row}
  ├─ GET  {p}/{t}/_apis/work/boards/{board}/cardsettings → fields to show per WIT
  ├─ GET  {p}/{t}/_apis/work/teamsettings/teamfieldvalues → team area paths (WIQL scope)
  │
  ├─ POST {p}/{t}/_apis/wit/wiql                        → ids
  │       WHERE [System.WorkItemType] IN (<board WITs>)
  │         AND [System.AreaPath] UNDER <team areas>
  │         AND [System.State] <> 'Removed'
  │         AND (state not Completed OR [System.ChangedDate] >= @Today-N)   -- keep Done column bounded
  │       ORDER BY [<Order field>]
  │
  └─ POST {p}/_apis/wit/workitemsbatch  (chunks of 200, explicit fields) → Card[]
        → Board.placement: column = System.BoardColumn (+ BoardColumnDone, BoardLane)
```

The board config is cached per (project, team, board) and refreshed on demand or when its
`revision` changes. If the cache is enabled, the last snapshot is painted immediately on startup
and then reconciled by a delta sync.

## Moving a card

```
drop(card, target column/lane/position)
  │
  ├─ kanban-core::MovePlan::new(board, card, target)
  │     field_changes: WEF_…_Kanban.Column, (…Column.Done), (…Kanban.Lane),
  │                    System.State  if column.state_mappings[card.type] != card.state
  │     reorder:       (previousId, nextId) if the position changed
  │     warnings:      WIP limit exceeded, type not allowed in column
  │
  ├─ app: apply optimistically to the Store, mark the card "in flight"
  │
  ├─ PATCH {p}/_apis/wit/workitems/{id}   [test /rev, replace …]   → new rev
  ├─ PATCH {p}/{t}/_apis/work/workitemsorder  (only if reorder)
  │
  └─ ok      → replace the card with the server copy (new rev, rules may have changed fields)
     Conflict→ refetch the card, roll back the optimistic move, notify
     other   → roll back, surface the error
```

ADO can reject a state transition, for example because of rules or required fields. The rollback
path handles this. The `Api` error message is shown verbatim.

## State model (app)

- `AppState` (`crates/app/src/state.rs`) is a `Copy` struct of `Signal`s, provided once in `App`
  and read with `use_app()`: settings, the `AdoClient`, load status, the loaded
  `kanban_core::Workspace` (board configs per backlog level, `HashMap<CardId, Card>`, placements),
  ids with a write in flight, the dragged item and drop slot, the open dialog and toasts.
- The workspace is one signal. Boards are a few hundred cards, so re-rendering the active tab on a
  change is cheap. If that stops being true, move cards into a `#[derive(Store)]` so one card's
  update re-renders only that card.
- Rollback keeps a copy of the card (and the board's placement for moves) before the optimistic
  update and restores it on failure.
- The stylesheet is embedded with `include_str!` rather than `asset!`. Without `dx`, Dioxus resolves
  assets next to the executable, so `cargo run` would start unstyled.

## Sync

`SyncEngine` runs as a background task (tokio interval, `poll_interval_secs`, default 30 s):

1. Run a WIQL with the board's scope plus `[System.ChangedDate] > '<last_sync>'` and `timePrecision=true`.
2. Batch-fetch the changed ids and upsert them into the Store. Skip cards with a move in flight.
   Drop cards that left the scope (state Removed, moved to another area or type).
3. Advance `last_sync` to the max `ChangedDate` seen.

The engine pauses when the window is hidden or minimised, and backs off on rate limiting. Webhooks
and service hooks are not possible for a desktop client.

## Time tracking (7pace)

7pace is a separate service with its own token. See [7pace-api.md](7pace-api.md). There are two paths:

```
Card totals (always, no 7pace call)
  workitemsbatch fields += Microsoft.VSTS.Scheduling.CompletedWork   ← 7pace keeps it in sync

Entries / logging / timer (time_tracking.enabled + 7pace token)
  open card    → GET  /api/rest/workLogs?$workItemIds={id}            → TimeEntry[]
  log time     → POST /api/rest/workLogs {workItemId, timeStamp(local), length(s), activityTypeId}
  start/stop   → POST /api/tracking/client/startTracking | stopTracking/0
  after write  → refetch the card from ADO for the new CompletedWork total
```

A missing or expired 7pace token disables only these features; the board keeps working.
`GET /api/tracking/client/current` is read when the app gains focus, because the timer can be started or
stopped from the 7pace UI too.

## Errors and resilience

| `AdoError`     | Handling                                                                 |
|----------------|---------------------------------------------------------------------------|
| `Unauthorized` | stop sync, route to Setup ("PAT expired or lacks Work Items scope")       |
| `Conflict`     | refetch the item, roll back the optimistic change                                |
| `RateLimited`  | the limiter delays further requests until `Retry-After`; sync interval backs off |
| `Api`/`Http`   | roll back where applicable, show the message; transient errors retried with backoff |

## Extension points

- **Entra ID auth:** a new `AuthProvider` impl (bearer token, resource
  `499b84ac-1321-427f-aa17-267ca6975798`). Nothing else changes.
- **Web/fullstack target:** add a Dioxus `web` feature to `app`. `ado-client` already uses reqwest,
  which compiles to wasm, and ADO supports CORS. Keyring and cache would need web replacements.
- **Other views** (sprint taskboard, cross-project "my work"): new WIQL builders and grouping in
  `kanban-core`. The move/sync machinery is reused.
