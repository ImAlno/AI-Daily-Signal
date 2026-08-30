# AI Daily Signal

AI Daily Signal is a calm, local-first command-line briefing that turns a standard set of AI news feeds into a concise daily signal.

## Status

Milestone two delivers the shared Rust core and cross-platform CLI: local feed collection, deterministic ranking, Raw and Smart summaries, opt-in model profiles, secure credentials, budgeted AI summaries, and immutable local caching. It is ready for personal use from source; packaged installers are not available yet.

The personal SwiftUI macOS alpha is available from source for macOS 26 on Apple Silicon. Build and installation instructions are in the [macOS alpha guide](docs/macos-alpha.md); signing and public distribution remain deferred.

Raw and Smart summaries are generated locally and never call an LLM. AI summaries are optional, require a profile plus one-time data-sharing consent, and fall back to the readable Smart summary when credentials, budget, provider output, or the provider itself are unavailable.

## Build and run

Install the [Rust 1.98 toolchain](https://www.rust-lang.org/tools/install), then build the executable:

```sh
cargo build --release -p signal-cli
```

The resulting executable is `target/release/signal` on macOS and Linux, and `target\release\signal.exe` on Windows. Run it from the repository, add its directory to your `PATH`, or copy it to a directory already on your `PATH`.

Start without AI:

```sh
signal init
signal refresh --no-ai
signal today
```

`init` writes the standard source configuration and initializes the local SQLite database. `refresh` contacts the configured feeds. `today` only reads the cached briefing, so it remains finite and works offline after a successful refresh.

Run `signal --help` or read the full [CLI reference](docs/cli.md) for every command.

## Optional AI summaries

Create a profile interactively to store a credential in the operating system's vault. Signal displays the data-sharing disclosure, requires an explicit yes, and then reads the credential with hidden terminal input:

```sh
signal models add --name daily --provider open-ai --model YOUR_MODEL_ID
signal models use daily
```

For headless use, reference an environment variable by name. The variable's value is read only when a request is made and is never stored in Signal's configuration or database:

```sh
signal models add \
  --name daily-env \
  --provider open-ai \
  --model YOUR_MODEL_ID \
  --credential-env OPENAI_API_KEY \
  --max-summaries 3 \
  --daily-budget-usd 0.25 \
  --input-usd-per-million 1.00 \
  --output-usd-per-million 4.00 \
  --consent-provider-data-sharing
signal models use daily-env
```

Interactive creation always displays the disclosure and asks for confirmation, even if the consent flag is present. A noninteractive caller must supply both `--credential-env` and `--consent-provider-data-sharing`.

Signal has no hardcoded model-ID or price catalog. Model identifiers are opaque user input. Token rates and the optional daily USD cap are also supplied by the user, who is responsible for keeping them current.

After deterministic ranking selects the finite briefing, `signal refresh` automatically generates AI variants for eligible selected stories through the enabled default profile. Use `signal refresh --no-ai` to skip all AI work for that refresh. Provider, credential, malformed-output, and budget failures keep Smart selected and do not turn a successful feed refresh into a failure.

Useful profile and manual-generation commands include:

```sh
signal models list
signal models test daily-env
signal summarize YOUR_STORY_ID
signal summarize YOUR_STORY_ID --profile daily-env --force
signal models remove daily-env
```

`models test` may incur provider cost. Normal summarize calls reuse the newest valid immutable cache entry; `--force` makes a budgeted provider request and adds another immutable variant without overwriting history. See the [CLI reference](docs/cli.md#model-profiles-and-ai-summaries) for every flag, provider rule, and exit status.

## Privacy and credentials

AI receives only the selected story's normalized title, clean excerpt, canonical URL, publication time, category, and sorted source IDs. It does not receive unrelated stories, local read/saved state, scores, Smart summaries, filesystem paths, credentials, or credential references. Official OpenAI requests also set `store: false`; provider redirects are disabled.

Interactive credentials use macOS Keychain, Windows Credential Manager, or Linux Secret Service. Linux needs an active D-Bus user session, a Secret Service provider such as GNOME Keyring, and an unlocked collection. Headless Linux systems without that desktop service should use `--credential-env VARIABLE_NAME`.

Credential values are never accepted as command arguments. Only the nonsecret environment-variable name or application-owned vault reference is persisted. Vault cleanup and restoration after a later local persistence failure are best-effort; any warning remains generic and never includes vault diagnostics or secret material.

Custom OpenAI-compatible profiles require both `--endpoint` and `--dialect responses|chat-completions`. Endpoints must use HTTPS. Literal loopback development endpoints may use HTTP only with `localhost`, `127.0.0.1`, or `[::1]`; URL user information is rejected.

## Local data

Signal follows each operating system's standard application-directory conventions. It writes `config.toml` in the configuration location and `signal.sqlite3` in the data location. Configuration and SQLite contain profile metadata and credential references, never credential values.

| Platform | Configuration | Local data |
| --- | --- | --- |
| macOS | `~/Library/Application Support/com.AIDailySignal.AI-Daily-Signal` | `~/Library/Application Support/com.AIDailySignal.AI-Daily-Signal` |
| Linux | `${XDG_CONFIG_HOME:-~/.config}/aidailysignal` | `${XDG_DATA_HOME:-~/.local/share}/aidailysignal` |
| Windows | `%APPDATA%\AIDailySignal\AI Daily Signal\config` | `%APPDATA%\AIDailySignal\AI Daily Signal\data` |

For isolated testing or a portable local workspace, set `SIGNAL_HOME` to a directory. Signal then creates `config/`, `data/`, and `cache/` beneath it instead of using platform directories.

## Design and roadmap

The product direction is documented in the approved [design specification](docs/superpowers/specs/2026-08-29-ai-daily-signal-design.md), and the milestone-two contracts are in the [model profiles and AI summaries specification](docs/superpowers/specs/2026-08-29-model-profiles-ai-summaries-design.md).

The personal macOS alpha now adds shared-core bindings and a standalone native companion from source. Signing, notarization, installers, and public distribution follow later.

## Development checks

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

The workflow is configured to run normal verification on macOS, Ubuntu, and Windows, plus an explicitly gated real OS credential-store contract. Local and remote provider tests use loopback mock servers and never call paid APIs.

## License

MIT. See [LICENSE](LICENSE).
