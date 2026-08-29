# AI Daily Signal

AI Daily Signal is a calm, local-first command-line briefing that turns a standard set of AI news feeds into a concise daily signal.

## Status

This is the first milestone: a cross-platform CLI that collects RSS and Atom feeds, stores briefings locally, and produces deterministic Raw and Smart summaries. It is ready for personal use from source; packaged installers are not available yet.

Raw and Smart summaries are generated locally from the feed content and **never call an LLM**. No API key, AI credential, account, or hosted service is required.

## Build and run

Install the [Rust 1.98 toolchain](https://www.rust-lang.org/tools/install), then build the executable:

```sh
cargo build --release -p signal-cli
```

The resulting executable is `target/release/signal` on macOS and Linux, and `target\release\signal.exe` on Windows. Run it from the repository, add its directory to your `PATH`, or copy it to a directory already on your `PATH`.

Start with:

```sh
signal init
signal refresh
signal today
```

`init` writes the standard source configuration and initializes the local SQLite database. `refresh` is the only command that contacts the configured feeds. `today` only reads the cached briefing, so it is finite and works without a network connection after a successful refresh.

Run `signal --help` or read the full [CLI reference](docs/cli.md) for every command.

## Local data

Signal follows each operating system's standard application-directory conventions. It writes `config.toml` in the configuration location and `signal.sqlite3` in the data location.

| Platform | Configuration | Local data |
| --- | --- | --- |
| macOS | `~/Library/Application Support/com.AIDailySignal.AI-Daily-Signal` | `~/Library/Application Support/com.AIDailySignal.AI-Daily-Signal` |
| Linux | `${XDG_CONFIG_HOME:-~/.config}/aidailysignal` | `${XDG_DATA_HOME:-~/.local/share}/aidailysignal` |
| Windows | `%APPDATA%\AIDailySignal\AI Daily Signal\config` | `%APPDATA%\AIDailySignal\AI Daily Signal\data` |

For isolated testing or a portable local workspace, set `SIGNAL_HOME` to a directory. Signal then creates `config/`, `data/`, and `cache/` beneath it instead of using platform directories.

## Design and roadmap

The product direction is documented in the approved [design specification](docs/superpowers/specs/2026-08-29-ai-daily-signal-design.md).

Later milestones add GitHub sources and changelog coverage, opt-in AI summaries with user-selected model profiles and API keys, and the native macOS companion. Signing, installers, and distribution follow after those product capabilities.

## Development checks

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Continuous integration runs these checks on macOS, Linux, and Windows.

## License

MIT. See [LICENSE](LICENSE).
