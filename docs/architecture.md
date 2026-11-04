# Architecture

## Goals and non-goals

- **Goal:** a fast, local-first desktop kanban over Azure DevOps (ADO) work items. Azure DevOps
  stays the single source of truth. The app is a client and adds no new data model.
- **Goal:** respect each team's existing board configuration (columns, state mappings, WIP limits,
  swimlanes, card fields) instead of redefining it.
- **Non-goal (for now):** the visual and interaction design, web/mobile targets, Entra sign-in, and
  editing board configuration (columns, rules).

## Layers

```
┌───────────────────────────── crates/app (Dioxus desktop) ─────────────────────────────┐
│ routes.rs   Setup · Board{project,team,board} · Settings                              │
│ views/      one component per route         components/  cards, columns, dialogs      │
│ state.rs    Store<BoardState>, settings, drag state, in-flight moves                  │
│ services.rs builds client, runs loads/moves/sync as tasks, optimistic update/rollback │
└──────────────────────────────────────────┬────────────────────────────────────────────┘
                                           │ domain types only
┌──────────────────────────── crates/kanban-core ─────────────────────▼─────────────────┐
│ board.rs  Board/Column/Lane/BoardFields   card.rs  Card                               │
│ move_plan.rs  drop → field patches + reorder   sync.rs  SyncEngine (delta polling)    │
│ cache.rs  last board snapshot on disk          config.rs  Settings + keyring          │
└──────────────────────────────────────────┬────────────────────────────────────────────┘
                                           │ DTOs
┌──────────────────────────── crates/ado-client ──────────────────────▼─────────────────┐
│ client.rs AdoClient (reqwest, URL/api-version, auth header)   auth.rs AuthProvider/Pat │
│ api/*  boards · backlog · teams · iterations · wiql · workitems · comments · order    │
│ models/* serde DTOs        error.rs AdoError        rate_limit.rs Retry-After/X-RateLimit│
└───────────────────────────────────────────────────────────────────────────────────────┘
```

Dependencies only point downward. `ado-client` and `kanban-core` have no Dioxus or desktop
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

- `Store<BoardState>`, using Dioxus 0.7 `#[derive(Store)]`: board config, `HashMap<CardId, Card>`,
  and per-column ordered id lists. Per-field stores mean that updating one card re-renders only that
  card and the columns whose lists changed.
- `Signal<Settings>`, `Signal<ConnectionStatus>`, `Signal<Option<CardId>>` for the card being
  dragged, and `Signal<HashMap<CardId, PendingMove>>` for rollback.
- Context is provided once in `App`. Views read it with `use_context`.

## Sync

`SyncEngine` runs as a background task (tokio interval, `poll_interval_secs`, default 30 s):

1. Run a WIQL with the board's scope plus `[System.ChangedDate] > '<last_sync>'` and `timePrecision=true`.
2. Batch-fetch the changed ids and upsert them into the Store. Skip cards with a move in flight.
   Drop cards that left the scope (state Removed, moved to another area or type).
3. Advance `last_sync` to the max `ChangedDate` seen.

The engine pauses when the window is hidden or minimised, and backs off on rate limiting. Webhooks
and service hooks are not possible for a desktop client.

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
