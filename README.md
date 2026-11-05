# devops-kanban

A desktop kanban board for **Azure DevOps** work items, built with Rust and
[Dioxus](https://dioxuslabs.com). It reads your team's existing board configuration (columns,
state mappings, WIP limits, swimlanes, card fields) and lets you work the board faster than the
Azure DevOps web UI. Time can be logged through 7pace Timetracker. Every change is written
straight back to Azure DevOps or 7pace. There is no server and no separate database.

> **Status:** first implementation. A List of the whole backlog tree (epics down to tasks, in
> aligned columns, expandable per row or per level, sortable by due date or priority), a kanban
> (the team's real Features and Stories boards, with hideable columns, a per-person WIP meter and
> "Move back" after a move) and a timeline, with drag and drop written back to Azure DevOps.
> Items are created and edited in a two-column dialog (searchable parent, tags, description, state
> with the column it moves to, and a list of unsaved changes). 7pace time tracking on kanban
> cards: start/stop the timer (Stop lets you adjust what gets logged), add time, and edit your
> entries in the card's expandable details. Row and card style (tint, accent bar, type icons, hover behaviour) is set
> per view under *View options*, next to the view tabs.

## Prerequisites

- Rust (stable, 1.88+). The toolchain is pinned by `rust-toolchain.toml`.
- Linux / WSL system libraries for the webview:
  ```sh
  sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev libsoup-3.0-dev
  ```
  WSL needs WSLg to show the window. Windows and macOS need nothing extra.
- Optional, for hot reload: the Dioxus CLI **0.7.x**
  ```sh
  cargo binstall dioxus-cli@0.7   # or: cargo install dioxus-cli --version ^0.7
  ```

## Build and run

```sh
cargo run -p app                 # build + launch
cd crates/app && dx serve --desktop   # dev loop with hot reload
cargo test --workspace
```

## Connecting to Azure DevOps

The app authenticates with a **Personal Access Token (PAT)**.

1. In Azure DevOps, open *User settings → Personal access tokens → New token*.
2. Set the organization (e.g. `media-press`) and the scope **Work Items: Read & write**. Add
   **Graph: Read** to see people's profile pictures; without it the app shows initials.
3. On first launch, enter the organization and paste the token. The app stores the token in your
   OS keyring (Windows Credential Manager, macOS Keychain, or Secret Service on Linux), never in a file.
4. Pick project → team → board.

Microsoft Entra sign-in may be added later. The auth layer is designed for it.

### 7pace Timetracker (optional)

Time tracking needs a separate **7pace API token**, because 7pace doesn't accept the Azure DevOps PAT:

1. In Azure DevOps, open *7pace Timetracker → Settings → Reporting & API → Create New Token*.
2. Paste it in the app's *Settings → 7pace time tracking*. It is checked against 7pace and goes to
   the OS keyring like the PAT. Saving it turns time tracking on.
3. The token is valid for 1 year. Renew it in the same 7pace screen.

Each kanban card then gets a timer (▶/■), *Add time*, and your own entries (click one to edit or
delete it). A running timer also shows in the top bar on every tab. Hours already tracked appear on
cards without a token, because 7pace syncs them into *Completed Work*.

## Settings

Non-secret settings live in `config.toml` in the platform config directory:

| OS      | Path                                                    |
|---------|---------------------------------------------------------|
| Linux   | `~/.config/devops-kanban/config.toml`                   |
| Windows | `%APPDATA%\media-press\devops-kanban\config\config.toml`|
| macOS   | `~/Library/Application Support/tv.media-press.devops-kanban/config.toml` |

Start from [config.example.toml](config.example.toml). The main keys are `organization`, the default
project/team/board, `poll_interval_secs` and `cache_enabled`. [docs/settings.md](docs/settings.md)
covers every key, the keyring entry and the environment overrides.

## Project layout

```
crates/ado-client        Azure DevOps REST client (UI-agnostic)
crates/sevenpace-client  7pace Timetracker REST client (UI-agnostic)
crates/kanban-core       Board model, move logic, sync, time tracking, cache, settings
crates/app               Dioxus desktop UI
docs/                    Architecture, API notes, settings reference
```

## Documentation

- [docs/architecture.md](docs/architecture.md): layers, data flow, state, sync
- [docs/azure-devops-api.md](docs/azure-devops-api.md): the endpoints used and their quirks
- [docs/7pace-api.md](docs/7pace-api.md): 7pace Timetracker API (logging time, timer)
- [docs/settings.md](docs/settings.md): configuration reference
- [AGENTS.md](AGENTS.md): contributor and AI-agent guide (`CLAUDE.md` imports it)
