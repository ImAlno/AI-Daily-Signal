# Personal macOS alpha

AI Daily Signal includes a personal, menu-bar-first macOS companion that can be built from source. The alpha requires macOS 26 on Apple Silicon. Intel Macs and older macOS releases are not supported.

## Build and open the app

Install Rust 1.98.0 and the Swift command-line tools for the macOS 26 SDK, then run from the repository root:

```sh
scripts/build-macos-app.sh
scripts/verify-macos-app.sh
open -n "target/macos/AI Daily Signal.app"
```

The build script generates the UniFFI Swift bindings, builds the Rust bridge and Swift executable in release mode, and recreates only `target/macos/AI Daily Signal.app`. You can open the app there or copy that complete `.app` to `/Applications` for personal use. Rebuild before copying when the source changes.

The app is standalone: it bundles `libsignal_ffi.dylib` and does not need the `signal` CLI, a repository checkout, or a repository-relative runtime path after it has been copied. The optional CLI can be installed separately; when both use the normal macOS application location, they share source configuration, SQLite briefing state, model metadata, and state revisions. Either surface observes the other surface's saved/read, source, and model changes.

## First briefing and everyday use

On first launch, **Build My First Briefing** installs the standard feed source pack and attempts a foreground refresh. AI is not required. Refreshing contacts enabled source websites, and an offline first attempt can be retried without losing initialization.

Refresh is foreground-only in this alpha: open the app and choose Refresh. Closing the reading window leaves the menu-bar companion running; use **Quit AI Daily Signal** to stop it.

Personal RSS or Atom sources can be added in Sources with a name, category, and weight. Standard sources can be enabled or disabled; personal sources can also be removed. No Terminal work is required for source installation or management.

## AI consent and credentials

AI summaries remain optional. Creating a model profile requires explicit consent before approved story content is sent to that provider. Prices, budgets, limits, and opaque model identifiers are supplied by you; testing a profile is a separate, explicitly confirmed operation that may incur provider cost.

Keychain mode passes the credential directly to the existing macOS Keychain adapter and clears the form value after the save attempt. Environment mode stores only the environment-variable name. Credential values and raw provider response bodies are not persisted in the configuration, SQLite database, app bundle, or app output.

## Alpha boundaries

The following are intentionally deferred:

- public code signing, notarization, downloads, installers, and automatic updates;
- scheduled or background refresh, launch at login, and notifications;
- GitHub, changelog-page, and announcement-page collectors;
- Intel Macs and macOS versions older than 26;
- Xcode UI automation and screenshot baselines.

The local build may receive an ad-hoc signature when macOS reports that one is needed for local launch. That is not public signing or notarization.
