# 7pace Timetracker API notes

media-press tracks time with **7pace Timetracker** (Appfire), an Azure DevOps extension with its
**own** service and REST API. Everything you can do in the 7pace UI can be done through this API:
log time against a work item, edit and delete entries, and start or stop the live timer.

Sources (November 2026):
- Swagger UI: <https://timehub.7pace.com/apireference/>
- Specs: [v3.2](https://timehub.7pace.com/Content/api-reference/api-reference-v3.2.json),
  [v3.3-beta](https://timehub.7pace.com/Content/api-reference/api-reference-v3.3-beta.json)
- Appfire docs: <https://support.appfire.com/space/7TFA/1253539983.md> (REST API),
  [OData](https://support.appfire.com/space/7TFA/1253540266.md)

The shapes here were checked against the published v3.2 OpenAPI spec, but not yet against a live
7pace call. See [Unverified](#unverified).

## Host and versions

| What  | URL | Notes |
|-------|-----|-------|
| REST  | `https://{org}.timehub.7pace.com/api/rest/<resource>?api-version=3.2` | `api-version` is **required** |
| Timer | `https://{org}.timehub.7pace.com/api/tracking/client/...` | |
| OData | `https://{org}.timehub.7pace.com/api/odata/v3.2/<entitySet>` | Read-only reporting; not used by the app |

- This is **not** `dev.azure.com`, so the ADO client, its rate limits and its token don't apply.
- The current stable version is **3.2**. 3.3-beta (adds soft delete) may change without notice. 2.1 is retired, and
  3.0 and 3.1 are deprecated.
- REST responses are an envelope: `{ "data": … }` on success. The **tracking** endpoints
  (`/api/tracking/client/...`) return their `trackingStateModel` **without** the envelope.
- Errors (v3.2 spec) are `{ "status", "errorCode", "errorDescription", "fields": [{name, value}] }`
  at the top level. The client also accepts the older `{ "error": { "code", "message" } }`.

## Authentication

An **Azure DevOps PAT is not accepted.** Use a **7pace API token**:

1. Open 7pace Timetracker → *Settings → Reporting & API → Create New Token*.
2. The token is valid for **1 year** and can only be renewed in the 7pace UI.
3. Send it as `Authorization: Bearer <token>`. Basic auth with an empty username also works.

The token acts as its owner, so any 7pace role except *None* can log their own time.

**Prerequisite:** an org admin has set up the 7pace *Service Account*. Without it, the API does not
work for anyone.

**Later:** 7pace's OAuth PIN pairing flow (`client_id=OpenApi`) suits a desktop app with no token
pasting:
1. `POST /api/pin/create`.
2. The user approves the PIN in 7pace (it expires after 1 minute).
3. Poll `POST /api/pin/status` until it is approved.
4. `POST /token` with `grant_type=authorization_code`. Refresh with `grant_type=refresh_token`.

The access token lasts 1 h.

## Endpoint catalogue

| Area | Method & path | Purpose |
|------|---------------|---------|
| Me | `GET /api/rest/me` | Current user; validates the token in Setup |
| Worklogs | `GET /api/rest/workLogs` | Own entries (filters below) |
| Worklogs | `GET /api/rest/workLogs/all` | Everyone's entries; needs the Product, Budget or Administrator role |
| Worklogs | `POST /api/rest/workLogs` | **Log time on a work item** |
| Worklogs | `POST /api/rest/workLogs/batch` | Create ≤500 at once; `$validateOnly=true` for a dry run |
| Worklogs | `PATCH /api/rest/workLogs/{id}` | Edit (uuid). Omitted fields keep their values |
| Worklogs | `DELETE /api/rest/workLogs/{id}` | Delete |
| Activity types | `GET /api/rest/activityTypes` | `{isEnabled, defaultActivity, activityTypes:[{id, name, color}]}` |
| Timer | `POST /api/tracking/client/startTracking` | Start the live timer on a work item |
| Timer | `POST /api/tracking/client/stopTracking/0` | Stop it (`0` = stopped by the client, `1` = idle or locked) |
| Timer | `GET /api/tracking/client/current/{expand}` | The running timer, if any |

Read filters on `workLogs`: `$workItemIds` (comma-separated, **≤100**, or `FilterWorkItemIdsCountExceeded`),
`$fromTimestamp` / `$toTimestamp`, `$count` (≤500) with `$skip`, and `$includeEditable=true` (adds
`isCanEdit` / `isCanDelete`).

## Logging time on a work item

```http
POST https://media-press.timehub.7pace.com/api/rest/workLogs?api-version=3.2
Authorization: Bearer <7pace token>
Content-Type: application/json

{
  "workItemId": 117349,
  "timeStamp": "2026-11-04T09:00:00",
  "length": 5400,
  "comment": "Export definitions",
  "activityTypeId": "<uuid from /activityTypes>"
}
```

| Field | Type | Notes |
|-------|------|-------|
| `timeStamp` | date-time | **Start** of the work, in the **user's local time**. Defaults to now |
| `length` | int | **Seconds** (5400 = 1 h 30 min) |
| `billableLength` | int? | Seconds; set only when it differs from `length` |
| `workItemId` | int? | Required unless `comment` is set. In the batch schema it is a **string** |
| `comment` | string? | Required unless `workItemId` is set |
| `activityTypeId` | uuid? | Defaults to the user's default activity |
| `userId` | uuid? | Log for someone else. Defaults to the token's user and needs elevated rights |

The response is the created worklog (`data.id` is its uuid). Careful with the spelling: the
**write** model has `timeStamp`, but the **read** model (`GET workLogs`) has `timestamp`. There is no rev or ETag concurrency
like in ADO; a conflicting write returns **409**.

## Live timer

```http
POST /api/tracking/client/startTracking?api-version=3.2
{ "tfsId": 117349, "timeZone": 60, "remark": "…", "activityTypeId": "<uuid>", "isBillable": true }
```

`timeZone` is the UTC offset **in minutes**. Stopping creates the worklog. Starting while a timer
runs switches it (7pace stops and saves the old one).

`GET /api/tracking/client/current/false` returns (unwrapped):

```json
{ "track": { "tfsId": 117349, "trackingState": "tracking", "currentTrackLength": 125,
             "currentTrackStartedDateTime": "…", "workItem": { "id": 117349, "title": "…" } },
  "trackSettings": { "isTrackingStartAllowed": true, "responseState": "OK", "responseMessage": null } }
```

`track` is null when nothing runs; `trackingState: "idle"` also counts as not running.
`trackSettings.responseState` `Error`/`AuthError` with a `responseMessage` can refuse a start
without an HTTP error. Only one timer runs per
user, and the 7pace web UI or desktop client may start or stop it too, so read `current` before showing
the timer's state.

## Totals without calling 7pace

7pace's *Work Item Automation* can write the **total tracked hours** of a work item into a native ADO
field. Our org does this into `Microsoft.VSTS.Scheduling.CompletedWork`: on #117349 the field goes
7.9 → 9.04 → 9.29, each step a separate revision by the person who tracked the time.

So **the board shows totals by fetching `CompletedWork` with the other card fields**, at zero 7pace
cost. Caveats:
- This is configured per project. A project without it will have empty or manual values.
- Time tracked before the sync was enabled is not included.
- 7pace overwrites manual edits to the field.
- 7pace's docs say values are rounded to 1 decimal, but ours have 2, so that part of the docs is outdated.
- After writing a worklog, refetch the card to pick up the new total. Expect a short delay.

## Rate limits

- A 429 carries `Retry-After` (seconds).
- The spec states "10 per second, 100 per minute". The docs call ~100 CRUD requests per user per hour
  "safe" and say limits are dynamic.
- Errors to handle: `ApiDisabled` (the API is off during account migration), `AccessDenied`,
  `FilterWorkItemIdsCountExceeded`.

Never poll 7pace for card totals. Use `CompletedWork`.

## Approvals

7pace can lock weeks through time approval (`/api/rest/timeApproval/...`; states 1 Submitted,
2 Reopened, 3 Approved, 99 Closed). Edits to time in approved weeks are expected to be rejected.
Show 7pace's error message verbatim.

## How the app uses it

Kanban only (`crates/app/src/time_tracking.rs`, `components/time.rs`):
- **Startup / Settings:** `GET rest/me` validates the token, then `GET rest/activityTypes` and the
  timer's `current`.
- **Entries:** the user's own `GET rest/workLogs?$workItemIds=…&$includeEditable=true` only for the
  cards whose details are open (the chevron on a card) and the card being tracked (≤100 ids per
  call, `$count=500` pages), once per card; again after a write or Reload. Collapsed cards make no
  7pace call: their total comes from `CompletedWork`. `isCanEdit == false` (approved weeks) makes an entry read-only.
- **Timer:** `current` on Kanban open, window focus, after start/stop, and every 5 minutes.
- **Writes:** `POST`/`PATCH`/`DELETE rest/workLogs`, `startTracking`, `stopTracking/0`. After each,
  the card's entries are refetched and the ADO card is refetched ~6 s later for `CompletedWork`.
- **Stop:** opens the time dialog with the timer's time; the timer keeps running until the user
  confirms. Confirming calls `stopTracking/0`, which saves the track as a worklog by itself, so the
  app never creates a second one: it reads the card's worklogs, takes the entry that starts at the
  timer's start (±5 min), and `PATCH`es it only if the user changed the duration, timing, activity
  or comment. An untouched duration keeps 7pace's exact length.

## Unverified

The shapes above come from the published v3.2 OpenAPI spec; the client's tests use hand-written
JSON in those shapes. Test these with a real token:

- How REST interprets `timeStamp` with or without a UTC offset (the spec says "local time"; the
  OData samples use `Z`).
- The status code and error when writing into a submitted or approved week (probably 409 or 403).
- The minimum duration and any rounding on writes.
- The exact role needed to log time for another user via `userId`.
- CORS (irrelevant for the desktop app).
- That `currentTrackStartedDateTime` is local time (the app treats a value without an offset as
  local, and falls back to `now − currentTrackLength`).
