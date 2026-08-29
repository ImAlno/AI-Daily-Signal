# Signal CLI reference

`signal` is the local-first command-line interface for AI Daily Signal. It reads and writes a local TOML configuration and SQLite database. Only `refresh` and `today --refresh` access configured network feeds.

## Installation

Build with Rust 1.98 or newer within the 1.98 toolchain line:

```sh
cargo build --release -p signal-cli
```

Use `target/release/signal` on macOS/Linux or `target\\release\\signal.exe` on Windows. See the [README](../README.md#local-data) for normal platform locations.

## Global options and output

```text
signal [--plain | --json] <COMMAND>
```

- Default output is a human-readable briefing or result.
- `--plain` selects plain human-readable output. It is suitable for terminals and scripts that do not want terminal styling.
- `--json` emits a pretty, versioned JSON envelope: `{ "schema_version": 1, "data": ... }`.
- `--plain` and `--json` cannot be used together.

All output examples here are local and contain no credentials or secrets.

## Commands

### `init`

Creates the standard source configuration if it does not yet exist and initializes the local database. It does not fetch feeds.

```sh
signal init
signal --json init
```

### `refresh`

Fetches enabled RSS/Atom sources, ranks the collected items, stores the resulting briefing, and prints it. A failed source is recorded while successful sources continue; the command succeeds when at least one enabled source succeeds.

```sh
signal refresh
signal --json refresh
```

### `today`

Prints today's cached briefing only. It does not refresh or make a network request. It exits with code 4 when no briefing for the current date is stored.

```sh
signal today
signal --plain today
signal --json today
```

Use `--refresh` for the explicit refresh path, which fetches sources before printing the new briefing:

```sh
signal today --refresh
```

### `latest`

Lists the most recently stored stories. `--limit` defaults to 20.

```sh
signal latest
signal latest --limit 5
signal --json latest --limit 5
```

### `show <id>`

Prints one stored story by its local ID.

```sh
signal show story-id
signal --json show story-id
```

### `save <id>`

Marks a stored story as saved. Pass `--remove` to clear its saved state.

```sh
signal save story-id
signal save story-id --remove
```

### `saved`

Lists saved stories.

```sh
signal saved
signal --json saved
```

### `status`

Shows the local story count, most recent refresh time, and data-generation number.

```sh
signal status
signal --json status
```

### `sources list`

Lists the standard configured sources and whether each one is enabled.

```sh
signal sources list
signal --json sources list
```

### `sources enable <id>` and `sources disable <id>`

Enables or disables a source and persists that choice in `config.toml`. At least one source must remain enabled before a refresh can run.

```sh
signal sources disable github-blog
signal sources enable github-blog
```

## Isolated local state with `SIGNAL_HOME`

Set `SIGNAL_HOME` to make all state live below one directory. This is useful for tests, demos, and portable scratch data:

```sh
SIGNAL_HOME="$PWD/.signal-demo" signal init
SIGNAL_HOME="$PWD/.signal-demo" signal sources list
```

The directory contains `config/config.toml`, `data/signal.sqlite3`, and `cache/`. Do not place API keys in this configuration: model providers and AI summaries are intentionally out of scope for this milestone.

In PowerShell, use:

```powershell
$env:SIGNAL_HOME = "$PWD\\.signal-demo"
signal init
```

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Command completed successfully. |
| 2 | Invalid command or invalid/unreadable configuration. |
| 3 | Network/feed refresh failed because no enabled source succeeded. |
| 4 | Requested local item or briefing was not found. |
| 5 | Local database or storage operation failed. |

Errors are intentionally concise and do not echo filesystem paths, story identifiers, source URLs, or configuration contents.

## Scope boundary

This CLI uses the bundled standard feed pack and local deterministic ranking. Raw and Smart summaries do not call an LLM. GitHub sources, AI providers/API keys, AI-generated summaries, and the macOS companion are later milestones.
