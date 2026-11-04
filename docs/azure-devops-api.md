# Azure DevOps REST API notes

The endpoints devops-kanban uses, with quirks verified against the `media-press` organization
(November 2026). Paths are relative to `https://dev.azure.com/{organization}/`. `{p}` is the project
and `{t}` the team (a name or an id; URL-encode spaces).

Official reference: <https://learn.microsoft.com/en-us/rest/api/azure/devops/?view=azure-devops-rest-7.1>

## Authentication

```
Authorization: Basic base64(":" + PAT)
```

The username is empty. A bad or expired PAT may return **203 with an HTML sign-in page** instead
of 401, so treat any non-JSON 203 as `Unauthorized`. Required PAT scope: `vso.work_write`
(*Work Items: Read & write*).

## Endpoint catalogue

| Area | Method & path | api-version | Purpose |
|------|---------------|-------------|---------|
| Connection | `GET _apis/connectionData` | 7.1-preview | Signed-in user, PAT check |
| Projects | `GET _apis/projects` | 7.1 | Project picker |
| Teams | `GET _apis/projects/{p}/teams` | 7.1 | Team picker |
| Team settings | `GET {p}/{t}/_apis/work/teamsettings` | 7.1 | `bugsBehavior`, `backlogVisibilities`, default iteration |
| Team areas | `GET {p}/{t}/_apis/work/teamsettings/teamfieldvalues` | 7.1 | Area paths for WIQL scope |
| Iterations | `GET {p}/{t}/_apis/work/teamsettings/iterations?$timeframe=current` | 7.1 | Current sprint (future taskboard) |
| Backlog config | `GET {p}/{t}/_apis/work/backlogconfiguration` | 7.1 | Backlog levels → WITs, state categories, Order field |
| Boards | `GET {p}/{t}/_apis/work/boards` | 7.1 | Board list (one per visible backlog level) |
| Board | `GET {p}/{t}/_apis/work/boards/{board}` | 7.1 | Columns, rows, writable field names |
| Card settings | `GET {p}/{t}/_apis/work/boards/{board}/cardsettings` | 7.1 | Fields shown on cards, per WIT |
| Card rules | `GET {p}/{t}/_apis/work/boards/{board}/cardrulesettings` | 7.1 | Card styling rules (optional) |
| WIQL | `POST {p}/{t}/_apis/wit/wiql?timePrecision=true` | 7.1 | Query → ids |
| Batch get | `POST {p}/_apis/wit/workitemsbatch` | 7.1 | Hydrate ≤200 ids |
| Get one | `GET {p}/_apis/wit/workitems/{id}?$expand=relations` | 7.1 | Detail / refetch after conflict |
| Update | `PATCH {p}/_apis/wit/workitems/{id}` | 7.1 | Move, edit (JSON Patch) |
| Create | `POST {p}/_apis/wit/workitems/${type}` | 7.1 | New card (JSON Patch) |
| WIT types | `GET {p}/_apis/wit/workitemtypes` | 7.1 | Type colours. Large response (includes `xmlForm`), so fetched once per load |
| WIT states | `GET {p}/_apis/wit/workitemtypes/{type}/states` | 7.1 | State colours and categories |
| Comments | `GET/POST {p}/_apis/wit/workItems/{id}/comments` | 7.1-preview.4 | Discussion |
| Reorder | `PATCH {p}/{t}/_apis/work/workitemsorder` | 7.1 | Position within a column |

> **Quirk:** some teams' `work/boards*` and `work/backlogconfiguration` return **HTTP 500** on every
> api-version, while their `teamsettings` and `teamfieldvalues` work (seen on Hubert / `DEV - Hubert`;
> `Hubert Team` in the same project is fine). Treat this as "board unavailable for this team":
> show the error for that board and keep the rest of the app working. Don't retry it in a loop.
> Separately, ADO has transient outages that also surface as 500s, so retry 5xx with backoff
> before concluding a team is broken.

## Board configuration

`GET IT Finance/_apis/work/boards/Stories?api-version=7.1` (trimmed):

```json
{
  "name": "Stories",
  "revision": 0,
  "columns": [
    { "id": "96694e5a-…", "name": "New",    "itemLimit": 0, "stateMappings": { "User Story": "New" },    "columnType": "incoming" },
    { "id": "0b4f8ee7-…", "name": "Active", "itemLimit": 5, "stateMappings": { "User Story": "Active" }, "isSplit": false, "columnType": "inProgress" },
    { "id": "899a4497-…", "name": "Blocked","itemLimit": 5, "stateMappings": { "User Story": "Blocked" },"isSplit": false, "columnType": "inProgress" },
    "… Paused, Ready, Done, TEST-ACTIVE, TEST-FAILED …",
    { "id": "af2ec698-…", "name": "Closed", "itemLimit": 0, "stateMappings": { "User Story": "Closed" }, "columnType": "outgoing" }
  ],
  "rows": [ { "id": "00000000-0000-0000-0000-000000000000" } ],
  "allowedMappings": { "InProgress": { "User Story": ["Active", "New", "Blocked", "…"] } },
  "fields": {
    "columnField": { "referenceName": "WEF_05102A5E6E294938B163F2FBD09ECDF2_Kanban.Column" },
    "rowField":    { "referenceName": "WEF_05102A5E6E294938B163F2FBD09ECDF2_Kanban.Lane" },
    "doneField":   { "referenceName": "WEF_05102A5E6E294938B163F2FBD09ECDF2_Kanban.Column.Done" }
  }
}
```

- `itemLimit` is the WIP limit. `0` means no limit. The incoming and outgoing columns never have one.
- `isSplit: true` divides the column into *Doing* and *Done*. Membership is tracked by the `…Column.Done` boolean.
- The all-zero row id is the default swimlane. Named rows have a `name` and a `color`.
- `stateMappings` is per work item type. A column can map different types to different states.
- The `WEF_<guid>` prefix is **unique per board**. Always read it from `fields`.

`cardsettings` returns `cards: { "<WIT>": [ { "fieldIdentifier": "System.AssignedTo", "displayFormat": "AvatarAndFullName" }, … ] }`.

## Backlog configuration

`GET IT Finance/_apis/work/backlogconfiguration` (trimmed):

```json
{
  "requirementBacklog": { "id": "Microsoft.RequirementCategory", "name": "Stories", "workItemTypes": [ { "name": "User Story" } ] },
  "taskBacklog":        { "id": "Microsoft.TaskCategory",        "name": "Tasks",   "workItemTypes": [ { "name": "Task" } ] },
  "portfolioBacklogs":  [ { "name": "Features", "workItemTypes": [ { "name": "Feature" } ] },
                          { "name": "Epics", "isHidden": true, "workItemTypes": [ { "name": "Epic" } ] } ],
  "workItemTypeMappedStates": [
    { "workItemTypeName": "User Story",
      "states": { "New": "Proposed", "Active": "InProgress", "Blocked": "InProgress", "Paused": "InProgress",
                  "Ready": "InProgress", "Done": "Resolved", "TEST-ACTIVE": "Resolved", "TEST-FAILED": "Resolved",
                  "Closed": "Completed" } }
  ],
  "backlogFields": { "typeFields": { "Order": "Microsoft.VSTS.Common.StackRank",
                                     "Effort": "Microsoft.VSTS.Scheduling.StoryPoints",
                                     "Team": "System.AreaPath" } },
  "bugsBehavior": "asTasks"
}
```

- Board name ↔ backlog level: the board name equals the backlog `name` (`Stories`, `Features`, `Epics`).
- `bugsBehavior`: `asRequirements` puts Bugs on the Stories board, `asTasks` (our org) puts them with Tasks,
  and `off` hides them.
- The state categories (`Proposed`/`InProgress`/`Resolved`/`Completed`/`Removed`) are how we reason
  about states without hardcoding custom names.

## Querying cards

```http
POST {p}/{t}/_apis/wit/wiql?api-version=7.1&timePrecision=true
Content-Type: application/json

{ "query": "SELECT [System.Id] FROM WorkItems
            WHERE [System.TeamProject] = @project
              AND [System.WorkItemType] IN ('User Story')
              AND [System.AreaPath] UNDER 'IT Finance'
              AND [System.State] <> 'Removed'
            ORDER BY [Microsoft.VSTS.Common.StackRank]" }
```

- The response is `workItems: [{ id, url }]` only. Results are capped at roughly 20,000; page with `[System.Id] > last`.
- In team context (`{t}` in the path) `@CurrentIteration` resolves correctly.

```http
POST {p}/_apis/wit/workitemsbatch?api-version=7.1
{ "ids": [1, 2, …≤200], "fields": ["System.Id", "System.Rev", "System.WorkItemType", "System.Title",
  "System.State", "System.BoardColumn", "System.BoardColumnDone", "System.BoardLane",
  "System.AssignedTo", "System.Tags", "System.Parent", "Microsoft.VSTS.Common.StackRank", "…card fields"],
  "errorPolicy": "omit" }
```

`fields` and `$expand` are mutually exclusive in batch. Fetch relations separately when needed.

## Moving a card

```http
PATCH {p}/_apis/wit/workitems/1234?api-version=7.1
Content-Type: application/json-patch+json

[
  { "op": "test",    "path": "/rev", "value": 7 },
  { "op": "add",     "path": "/fields/WEF_05102A5E6E294938B163F2FBD09ECDF2_Kanban.Column", "value": "Blocked" },
  { "op": "add",     "path": "/fields/System.State", "value": "Blocked" }
]
```

- `System.BoardColumn`, `System.BoardColumnDone` and `System.BoardLane` are **read-only**. Writing them fails with
  TF401326. Read them, but write only the `WEF_…` fields.
- Patch `System.State` only when the target column's `stateMappings[type]` differs from the current state.
  Patching only the state also works, because ADO moves the card to the first column mapped to that state, but
  that is ambiguous when several columns share a state.
- A failed `test /rev` returns an error mentioning the revision (HTTP 400/409/412 depending on
  the path). Map it to `AdoError::Conflict`.
- The response is the updated work item. Replace the local copy, because rules may have changed other fields.

## Reordering

```http
PATCH {p}/{t}/_apis/work/workitemsorder?api-version=7.1
{ "ids": [1234], "previousId": 1200, "nextId": 1250, "parentId": 0, "iterationPath": "" }
```

ADO computes the new `StackRank` values and returns them. Do not write StackRank directly.

## Change detection

Desktop clients can't receive service hooks, so poll:

```sql
... AND [System.ChangedDate] > '2026-11-04T12:00:00.000Z'   -- with timePrecision=true
```

Then batch-fetch the changed ids. Removed or out-of-scope items disappear from scoped queries.
Detect them by refetching ids we hold that no longer match.

## Rate limits

- About 200 TSTU (throughput units) per user per sliding 5-minute window.
- Response headers: `Retry-After`, `X-RateLimit-Resource`, `X-RateLimit-Delay`, `X-RateLimit-Limit`,
  `X-RateLimit-Remaining`, `X-RateLimit-Reset`.
- Throttled requests are first slowed down, then rejected with **429** (TF400733).
- <https://learn.microsoft.com/en-us/azure/devops/integrate/concepts/rate-limits>

## Things we don't use, and why

- `azure_devops_rust_api` crate: archived 2026-10-26 and unmaintained.
- Legacy Azure DevOps OAuth apps: no new registrations since 2025 and slated for removal. Use Entra if
  OAuth is ever needed.
- Service hooks / webhooks: they need a publicly reachable endpoint, which a desktop app doesn't have.
