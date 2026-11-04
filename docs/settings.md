# Settings

Settings come from three places, in order of precedence:

1. **Environment variables**, for development and CI only
2. **OS keyring**, which holds the PAT only
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
| `api.base_url`           | string  | `https://dev.azure.com` | ADO host |
| `api.api_version`        | string  | `7.1`                   | api-version for all stable endpoints |
| `api.max_batch`          | int     | `200`                   | Ids per `workitemsbatch` call. The ADO maximum is 200 |

Unknown keys are ignored with a warning. The app rewrites the file when settings change in the UI.

## Secrets: the OS keyring

| Service         | Account        | Value |
|-----------------|----------------|-------|
| `devops-kanban` | `<organization>` | Personal Access Token |

Backends: Windows Credential Manager, macOS Keychain, Linux Secret Service (GNOME Keyring /
KWallet). If no Secret Service is running (e.g. headless WSL), the app can't save the token and
asks for it at each launch. It does not fall back to a plaintext file.

**PAT requirements:** the organization must match `organization`, and the scope must be **Work Items
(Read & write)**. When the PAT expires the API returns 401 or 203. The app then returns to Setup.

## Environment overrides

| Variable             | Overrides |
|----------------------|-----------|
| `DEVOPS_KANBAN_ORG`  | `organization` |
| `DEVOPS_KANBAN_PAT`  | the keyring PAT (dev only; it is never written to the keyring) |
| `RUST_LOG`           | `log_level` |

## Other locations

| What   | Where (Linux) | Notes |
|--------|---------------|-------|
| Cache  | `~/.cache/devops-kanban/boards/<project>/<team>/<board>.json` | Safe to delete |
| Logs   | stderr | Never contain the PAT |

On other platforms the cache path comes from `ProjectDirs::cache_dir()`.
