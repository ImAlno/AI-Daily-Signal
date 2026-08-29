# Signal CLI reference

`signal` is the local-first command-line interface for AI Daily Signal. Milestone two includes the shared Rust core and cross-platform CLI. The native macOS companion remains the next milestone.

Signal reads and writes local TOML configuration and SQLite data. Feed access occurs only during `refresh` and `today --refresh`. Optional AI provider access occurs during an AI-enabled refresh, `today --refresh`, `models test`, or `summarize`; cached summary hits make no provider request.

## Installation

Build with Rust 1.98 or newer within the 1.98 toolchain line:

```sh
cargo build --release -p signal-cli
```

Use `target/release/signal` on macOS/Linux or `target\release\signal.exe` on Windows. See the [README](../README.md#local-data) for normal platform locations.

## Global options and output

```text
signal [--plain | --json] <COMMAND>
```

- Default output is a human-readable briefing or result.
- `--plain` selects plain human-readable output for terminals or scripts that do not want styling.
- `--json` emits a pretty, versioned JSON envelope: `{ "schema_version": 1, "data": ... }`.
- `--plain` and `--json` cannot be used together.

JSON retains the milestone-one story, briefing, status, and source fields. Milestone two adds safe structured selected-summary and aggregate generation-report fields. Credential values, credential references, full custom endpoint paths, cache keys, prompt text, and provider response bodies are not serialized.

## Briefing and source commands

### `init`

Creates the standard source configuration if it does not yet exist and initializes the local database. It does not fetch feeds or contact an AI provider.

```sh
signal init
signal --json init
```

### `refresh [--no-ai]`

Fetches enabled RSS/Atom sources, normalizes and deduplicates stories, ranks them deterministically, assembles a finite Smart briefing, and stores the result. A failed source is recorded while successful sources continue; the command succeeds when at least one enabled source succeeds.

After ranking, an ordinary refresh uses the enabled default model profile to generate or select cached AI variants for eligible selected stories. AI never changes which stories were selected or their order. Use `--no-ai` to skip profile lookup, provider requests, and AI cache selection for this invocation:

```sh
signal refresh
signal refresh --no-ai
signal --json refresh --no-ai
```

Missing credentials, consent, cap or budget exhaustion, provider errors, and malformed output leave the local Smart summary selected. They are counted in the generation report and do not turn a successful feed refresh into a failure. There is no automatic fallback to another paid profile.

### `today [--refresh]`

Prints the newest cached briefing only, even if it was generated on an earlier date. Without `--refresh`, it does not fetch feeds or contact an AI provider. Human output begins with `Status: fresh` or `Status: stale`; a stale briefing remains readable. It exits with code 4 when no briefing is stored.

```sh
signal today
signal --plain today
signal --json today
```

`--refresh` explicitly runs the normal AI-enabled refresh path before printing:

```sh
signal today --refresh
```

To refresh feeds without AI, run `signal refresh --no-ai` followed by `signal today`.

With `--json`, the `data` object includes a top-level `is_stale` boolean and each briefing item has its own `is_stale` value. A partial refresh may carry forward items from failed sources; carried items are stale while newly collected items remain fresh.

### `latest [--limit NUMBER]`

Lists the most recently stored stories. `--limit` defaults to 20.

```sh
signal latest
signal latest --limit 5
signal --json latest --limit 5
```

### `show <id>`

Prints one stored story by its local ID, including the currently selected AI variant when that story appears in the newest briefing.

```sh
signal show YOUR_STORY_ID
signal --json show YOUR_STORY_ID
```

### `save <id> [--remove]`

Marks a stored story as saved. Pass `--remove` to clear its saved state.

```sh
signal save YOUR_STORY_ID
signal save YOUR_STORY_ID --remove
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

## Model profiles and AI summaries

Model profile selectors accept a profile name or UUID. Names are case-insensitive. Provider families are `open-ai`, `anthropic`, `gemini`, and `open-ai-compatible`.

Signal does not ship a catalog of model identifiers or prices. `--model` values are opaque, user-supplied identifiers and are preserved for provider requests, cache identity, and reporting. USD token rates are user-supplied and must be kept current by the user.

### `models list`

Lists configured profiles and shows each profile's name, provider, opaque model, custom endpoint host and dialect when present, credential source type, consent state, enabled/default state, request limits, user rates, and daily cap. It never shows a credential value or vault account reference.

```sh
signal models list
signal --json models list
```

### `models add`

Required flags are `--name`, `--provider`, and `--model`. Supported optional flags are:

```text
--endpoint URL
--dialect responses|chat-completions
--credential-env VARIABLE_NAME
--max-summaries NUMBER
--daily-budget-usd USD
--input-usd-per-million USD
--output-usd-per-million USD
--max-output-tokens NUMBER
--timeout-seconds NUMBER
--max-retries NUMBER
--consent-provider-data-sharing
```

Interactive creation without `--credential-env` displays the data-sharing disclosure, requires an explicit `y` or `yes`, and then prompts for the credential with hidden input. The secret is stored in the OS credential vault and is never accepted as an option or positional argument:

```sh
signal models add \
  --name daily \
  --provider open-ai \
  --model YOUR_MODEL_ID \
  --max-summaries 3
```

For an environment-backed profile, pass only the environment variable's name. Signal stores that name and resolves its value when a request is made; it never silently falls through between vault and environment sources:

```sh
signal models add \
  --name daily-env \
  --provider anthropic \
  --model YOUR_MODEL_ID \
  --credential-env ANTHROPIC_API_KEY \
  --consent-provider-data-sharing
```

Interactive creation always displays the disclosure and requires confirmation, even when the consent flag is supplied. Noninteractive creation requires both `--credential-env VARIABLE_NAME` and `--consent-provider-data-sharing`. This stored one-time consent enables future automatic generation for that profile; Signal does not ask again before each refresh.

Official `open-ai`, `anthropic`, and `gemini` profiles do not accept custom endpoint or dialect flags. An `open-ai-compatible` profile requires both:

```sh
signal models add \
  --name compatible \
  --provider open-ai-compatible \
  --model YOUR_MODEL_ID \
  --endpoint https://gateway.example/v1 \
  --dialect responses \
  --credential-env COMPATIBLE_API_KEY \
  --consent-provider-data-sharing
```

Custom endpoints must use HTTPS and must not contain URL user information. For local development only, literal loopback hosts `localhost`, `127.0.0.1`, and `[::1]` may use HTTP. Redirects are disabled, and profile output shows only the destination host rather than a complete sensitive path.

The default profile limits are 5 summaries per refresh, 384 maximum output tokens, a 30-second timeout, and 2 retries. A daily monetary cap is disabled unless configured. Input and output rates must be supplied together and must be nonzero; a daily cap requires both rates. Currency is parsed exactly into integer micro-US-dollars rather than floating point.

Before a request, Signal estimates prompt tokens conservatively and reserves the maximum output-token cost using the user rates. The daily check atomically counts completed charges plus active reservations across concurrent processes. Reported provider usage replaces the estimate when safely available. Unreported success, malformed output, timeouts, and other possibly-sent failures retain a conservative charge; a proven pre-send connection failure is uncharged. Stale reservations expire after the full request/retry horizon plus a safety margin.

The per-refresh cap counts successful or possibly-sent outbound provider requests. Cache hits and failures proven not sent do not consume it.

### `models use <profile>`

Sets the default profile used by automatic refresh generation:

```sh
signal models use daily
```

Only the enabled, consented default profile is used automatically. Signal never silently tries a different paid profile.

### `models test <profile>`

Makes one explicit, bounded request using fixed synthetic nonprivate story text. It uses the profile's credential, validation, retry, accounting, and daily-budget paths and warns immediately before dispatch because provider cost may apply:

```sh
signal models test daily
signal --json models test daily
```

A non-successful explicit test prints only a typed, redacted result and exits with code 6.

### `models remove <profile> [--yes]`

Removes profile metadata but retains historical immutable summary variants and generation attempts. Interactive removal asks for confirmation; noninteractive removal requires `--yes`:

```sh
signal models remove daily
signal models remove daily --yes
```

Application-owned vault credentials are deleted best-effort. Environment variables are never changed. If vault deletion fails, output contains only a generic warning instructing the user to remove the stored credential manually; backend diagnostics are never exposed. Likewise, restoring or deleting a vault entry after a later profile-persistence failure is best-effort and preserves the original generic local error.

### `summarize <story-id> [--model <profile>] [--force]`

Generates or selects an AI summary for one stored story. Without `--model`, it uses the default profile; the override applies only to this invocation. The same credential, consent, request validation, daily budget, cache, immutable persistence, and structured-output validation used by automatic generation apply here:

```sh
signal summarize YOUR_STORY_ID
signal summarize YOUR_STORY_ID --model daily-env
signal summarize YOUR_STORY_ID --model daily-env --force
signal --json summarize YOUR_STORY_ID
```

Normal calls select the newest valid immutable variant for the cache identity without a provider request or cap/budget consumption. Cache identity changes when canonical story content, provider, normalized endpoint, opaque model, dialect, prompt version, output limit, or summary settings change. Credentials and credential references are excluded.

`--force` bypasses the lookup, makes a budgeted request, and inserts another immutable variant with a distinct generation identity. It never overwrites prior text, even when the variants share a cache key. A non-successful explicit summarize retains Smart, returns a redacted status, and exits with code 6.

## Provider data-sharing and privacy

AI generation occurs only after local deterministic ranking has selected a briefing story. The provider receives exactly:

- normalized title;
- clean excerpt;
- canonical URL;
- publication time;
- category; and
- sorted source IDs.

It does not receive story IDs, unrelated stories or local history, read/saved state, ranking scores, Smart summaries, filesystem paths, credentials, credential references, cache keys, or local configuration. The prompt requires one bounded JSON object with `what_happened`, `why_it_matters`, and an optional `caveat`. Invalid or oversized output becomes a typed Smart fallback; no automatic paid repair request is made.

Credentials are sent only in each provider's authentication header. Gemini keys are not placed in URLs, official OpenAI requests set `store: false`, response bodies are decoded through a 256 KiB cap, and provider error bodies are not persisted or displayed.

## Credential storage prerequisites

Interactive system-store profiles use:

- macOS Keychain Services;
- Windows Credential Manager; or
- Linux Secret Service.

Linux requires a running D-Bus user session, an available Secret Service provider such as GNOME Keyring, and an unlocked collection. Desktop sessions commonly provide these. For a headless session without Secret Service, use an environment-backed profile such as `--credential-env GEMINI_API_KEY`; set the variable's value through your shell, service manager, or secret-injection system rather than placing it in Signal commands or files.

## Isolated local state with `SIGNAL_HOME`

Set `SIGNAL_HOME` to make all non-vault state live below one directory. This is useful for tests, demos, and portable scratch data:

```sh
SIGNAL_HOME="$PWD/.signal-demo" signal init
SIGNAL_HOME="$PWD/.signal-demo" signal sources list
```

The directory contains `config/config.toml`, `data/signal.sqlite3`, and `cache/`. Profile metadata and environment variable names can appear there; credential values must not.

In PowerShell:

```powershell
$env:SIGNAL_HOME = "$PWD\.signal-demo"
signal init
```

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Command completed successfully. |
| 2 | Invalid command or invalid/unreadable configuration. |
| 3 | Network/feed refresh failed because no enabled source succeeded, or another non-explicit provider path failed. |
| 4 | Requested local item or briefing was not found. |
| 5 | Local database, storage, or credential operation failed. |
| 6 | Explicit `models test` or `summarize` generation did not succeed because of profile, consent, credential, cap, budget, provider, or output validation status. |

Errors are intentionally concise and do not echo vault diagnostics, credential values, provider response bodies, filesystem paths, story identifiers, source URLs, or configuration contents.

## Scope boundary

This milestone is the shared core and cross-platform CLI. It includes secure opt-in AI model profiles and AI summaries alongside local Raw and Smart summaries. The native SwiftUI macOS companion, packaged installers, signing, and distribution are later milestones.
