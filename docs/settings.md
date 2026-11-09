# Settings

Settings come from three places, in order of precedence:

1. **Environment variables**, for development and CI only
2. **OS keyring**, which holds the tokens (ADO PAT, 7pace token) only
3. **`config.toml`**, for everything else

## `config.toml`

Location: `directories::ProjectDirs::from("tv", "media-press", "devops-kanban").config_dir()`

| OS      | Path |
|---------|------|
| Linux   | `~/.config/devops-kanban/config.toml` |
| Windows | `%APPDATA%\media-press\devops-kanban\config\config.toml` |
| macOS   | `~/Library/Application Support/tv.media-press.devops-kanban/config.toml` |

The Setup view creates this file on first run. Start from
[config.example.toml](../config.example.toml).

| Key                      | Type    | Default                 | Meaning |
|--------------------------|---------|-------------------------|---------|
| `organization`           | string  | none (required)         | ADO organization, e.g. `media-press` |
| `default_project`        | string? | none                    | Project opened at startup |
| `default_team`           | string? | none                    | Team within that project (e.g. `IT Finance Team`) |
| `default_board`          | string? | none                    | Board / backlog level name (`Stories`, `Features`, `Epics`) |
| `poll_interval_secs`     | int     | `30`                    | Delta-sync interval. Values below 10 are clamped to 10 to stay within rate limits |
| `cache_enabled`          | bool    | `true`                  | Persist the last board snapshot for instant startup |
| `log_level`              | string  | `info`                  | `tracing` filter (`RUST_LOG` syntax). `RUST_LOG` overrides it |
| `last_view`              | string? | none                    | Last tab on the board screen: `list`, `kanban` or `timeline` |
| `timeline_info_width`    | int?    | none (640)              | Width in px of the Timeline's item list, set by dragging the separator |
| `window_maximized`       | bool    | `false`                 | The window was maximised when last used, so the app opens maximised. Written by the app |
| `window_size`            | int[2]? | none (1440×980)         | The window's normal inner size in logical px. Written by the app |
| `maximized_size`         | int[2]? | none                    | Its inner size when maximised. A window that opens maximised is created at this size, so it never shows at the normal size first; un-maximising then restores `window_size`. Written by the app |
| `one_off_tag`            | string  | `Ad-Hoc`                | Tag for one-off items (`""` turns it off). When set, the item dialog shows a checkbox with this tag next to Tags (features and stories, not epics or tasks); checking it adds the tag and leaves the parent empty, the only way to save an item without a parent. Set in *Settings* |
| `api.base_url`           | string  | `https://dev.azure.com` | ADO host |
| `api.api_version`        | string  | `7.1`                   | api-version for all stable endpoints |
| `api.max_batch`          | int     | `200`                   | Ids per `workitemsbatch` call. The ADO maximum is 200 |
| `time_tracking.enabled`  | bool    | `false`                 | 7pace on the kanban (timer, add time, your entries). Toggled in *Settings*; saving a token turns it on. Card totals from `CompletedWork` work without it |
| `time_tracking.base_url` | string  | `https://{organization}.timehub.7pace.com` | 7pace host |
| `time_tracking.api_version` | string | `3.2`               | 7pace REST api-version (required on every call) |

### Appearance

Set per view under *View options*, the button right after the List / Kanban / Timeline
tabs; changes show on the view straight away. Older configs without these
keys get the defaults, which match the original look.

| Key                          | Type   | Default | Meaning |
|------------------------------|--------|---------|---------|
| `appearance.list.tint`       | bool   | `true`  | List rows lean their background towards the work item type colour |
| `appearance.list.accent`     | string | `left`  | 4 px bar in the type colour on List rows: `left`, `right` or `off` |
| `appearance.list.icon`       | bool   | `false` | The type's icon (as set in Azure DevOps) before each List title |
| `appearance.timeline.tint`   | bool   | `false` | Timeline rows: the same three options as the List, on the row's info column. Defaults keep the plain look |
| `appearance.timeline.accent` | string | `off`   | 〃 accent bar: `left`, `right` or `off` |
| `appearance.timeline.icon`   | bool   | `false` | 〃 type icon before the title |
| `appearance.kanban.tint`     | bool   | `false` | Kanban cards lean their background towards their status colour |
| `appearance.kanban.accent`   | string | `left`  | 4 px bar in the status colour on kanban cards: `left`, `right` or `off` |
| `appearance.kanban.hover_widen` | bool | `false` | The column under the mouse widens while the others narrow (same total width); the done column never widens and resets to equal widths |
| `appearance.kanban.hover_open`  | bool | `false` | Cards open as soon as they are hovered and close shortly after the mouse leaves; a pin replaces the expand chevron and keeps a card open at the top of its column |

```toml
[appearance.list]
tint = true
accent = "left"
icon = false

[appearance.kanban]
accent = "off"
```

### Per-board preferences

`[boards."<project>/<team>/<board>"]` tables hold UI preferences the app writes when you change
them on the kanban. Delete a table to reset that board.

| Key              | Type     | Default | Meaning |
|------------------|----------|---------|---------|
| `hidden_columns` | string[] | `[]`    | Column names hidden on that board (from the *Columns* menu) |
| `pinned`         | int[]    | `[]`    | Cards pinned open at the top of their column (with `appearance.kanban.hover_open`) |
| `epic`           | int?     | none    | Epic the kanban is filtered to (from the *Epic* picker) |
| `assignees`      | string[] | `[]`    | Kanban *Assigned to* filter: `@me`, `@unassigned` or a `uniqueName` |
| `areas`          | string[] | `[]`    | Kanban *Area* filter: area paths, each including the areas below it |

```toml
[boards."IT Finance/IT Finance Team/Stories"]
hidden_columns = ["TEST-FAILED", "Closed"]
epic = 118181
assignees = ["@me"]
```

### Per-team filters

`[views."<project>/<team>"]` tables hold the board screen's filters, so they survive tab switches
and relaunches. The iteration is shared by every tab; the List and the Timeline each keep their
own filter bar in a `list` and a `timeline` sub-table (the Kanban's is in `[boards…]` above). The
search box is not saved. Empty lists mean "no filter".

| Key                     | Type     | Default | Meaning |
|-------------------------|----------|---------|---------|
| `iteration`             | string?  | none    | Iteration path from the header picker (includes child iterations) |
| `list_sort`             | string   | `backlog` | Order of the List within each parent: `backlog` (ADO rank; the only order you can drag in), `target_date` or `priority` |
| `<tab>.epic_states`     | string[] | `[]`    | Epic statuses shown |
| `<tab>.assignees`       | string[] | `[]`    | `@me`, `@unassigned` or a `uniqueName` |
| `<tab>.areas`           | string[] | `[]`    | Area paths, each including the areas below it |
| `<tab>.item_states`     | string[] | `[]`    | Work item statuses shown. Offered: the statuses of the kanban columns that aren't hidden |

```toml
[views."IT Finance/IT Finance Team"]
iteration = "IT Finance\\Sprint 12"

[views."IT Finance/IT Finance Team".list]
epic_states = ["New", "Active"]
assignees = ["@me"]

[views."IT Finance/IT Finance Team".timeline]
item_states = ["Active", "Blocked"]
```

Unknown keys are ignored. The app rewrites the file when settings change in the UI.

## Secrets: the OS keyring

| Service         | Account        | Value |
|-----------------|----------------|-------|
| `devops-kanban` | `<organization>` | Personal Access Token |
| `devops-kanban` | `7pace:<organization>` | 7pace API token |

Backends: Windows Credential Manager, macOS Keychain, Linux Secret Service (GNOME Keyring /
KWallet). If no Secret Service is running (e.g. headless WSL), the app can't save the token and
asks for it at each launch. It does not fall back to a plaintext file.

**PAT requirements:** the organization must match `organization`, and the scope must be **Work Items
(Read & write)**. Add **Graph (Read)** for profile pictures; without it avatars show initials. When
the PAT expires the API returns 401 or 203. The app then returns to Setup.

**7pace token:** created in *7pace Timetracker → Settings → Reporting & API*, valid for 1 year. It
can't be an ADO PAT. When it expires, only the time-tracking features stop; the board keeps working.

## Environment overrides

| Variable             | Overrides |
|----------------------|-----------|
| `DEVOPS_KANBAN_ORG`  | `organization` |
| `DEVOPS_KANBAN_PAT`  | the keyring PAT (dev only; it is never written to the keyring) |
| `DEVOPS_KANBAN_7PACE_TOKEN` | the keyring 7pace token (dev only) |
| `RUST_LOG`           | `log_level` |

## Other locations

| What   | Where (Linux) | Notes |
|--------|---------------|-------|
| Cache  | `~/.cache/devops-kanban/boards/<project>/<team>/<board>.json` | Safe to delete |
| Logs   | stderr | Never contain either token |

On other platforms the cache path comes from `ProjectDirs::cache_dir()`.
