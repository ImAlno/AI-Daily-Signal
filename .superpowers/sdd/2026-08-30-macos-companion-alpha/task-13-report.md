# Task 13 report: standalone macOS companion alpha

## Outcome

Task 13 assembles `target/macos/AI Daily Signal.app` as a standalone macOS 26 Apple Silicon accessory app. The bundle contains only the approved plist, Swift executable, Rust bridge dylib, generated code signature metadata, and the controller-provided app icon. It does not contain the optional `signal` CLI.

The build is reproducible through `scripts/build-macos-app.sh`; structure and loader policy are checked by `scripts/verify-macos-app.sh`; and the bounded local standalone launch, termination, environment, output, bundle, and isolated-Application-Support checks are implemented by `scripts/smoke-test-macos-app.sh`.

No paid provider API was called. Provider tests used deterministic fakes or local loopback servers.

## TDD evidence

- The bundle verifier was created and run before `scripts/build-macos-app.sh` existed. It failed for the intended reason: `target/macos/AI Daily Signal.app` was missing.
- The shared-process Rust acceptance test was first run as an explicit unimplemented contract and failed 1/1. Its completed form then passed with an isolated application root and real, separately spawned `signal` CLI processes.
- The Swift alpha acceptance test was first compiled as an explicit unimplemented contract. The local Command Line Tools SwiftPM path compiled it but discovered no executable tests; the direct Swift Testing runner was therefore used for the actual RED/GREEN execution. The completed acceptance flow passes.
- The first local smoke test failed because `lsappinfo find` returns application serial numbers rather than a PID. Diagnostic launch evidence showed the app was running and the captured output was clean. The monitor now uses `lsappinfo` to establish the bundle-ID registration and the exact bundled executable to obtain the PID. The same bounded smoke test then passed.
- Rust 1.98 Clippy exposed a pre-existing `collapsible_if` warning in `crates/signal-core/src/generator.rs` at the supplied base commit. The controller authorized a separate prerequisite commit containing only the semantics-preserving cleanup. Its cancellation contract and the full strict Clippy gate pass.

## Acceptance contracts

### Shared app and CLI state

`crates/signal-ffi/tests/shared_process_contract_test.rs` creates one temporary `SIGNAL_HOME`, seeds one briefing, opens the bridge over that root, and invokes the real debug CLI in separate processes. It proves:

- bridge save/read mutations appear in CLI `show` and `saved` output;
- a CLI unsave appears in a new bridge snapshot;
- a bridge-created personal source appears in CLI source output;
- CLI source disable changes the bridge source record and source-configuration revision;
- CLI model add/default selection appears in the bridge snapshot;
- bridge model removal appears in CLI model output;
- concurrent CLI save and bridge read mutation preserve both fields;
- CLI and bridge report the same final data generation; and
- SQLite reopens and returns status after all separate-process mutations.

### Swift alpha flow

`apps/macos/Tests/SignalAppKitTests/AlphaAcceptanceTests.swift` drives one fake-backed `AppModel` session through:

1. welcome;
2. an offline first refresh and safe retry;
3. a populated Today briefing;
4. Today, Latest, Saved, Sources, and Settings destinations;
5. confirmed save and read mutations;
6. personal source addition;
7. consented model profile addition with transient secret clearing;
8. explicitly confirmed paid-test action through a fake only; and
9. confirmed model removal.

## Verification evidence

Environment: macOS 26.6.1, Apple Silicon (`arm64`), Rust 1.98.0, Swift 6.3.3, macOS 26 SDK, Command Line Tools only (no full Xcode).

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed after the separately committed Rust 1.98 prerequisite cleanup |
| `cargo test --workspace --all-features` | Passed: 259 tests, 0 failed, 1 intentionally ignored Keychain contract |
| `cargo test -p signal-core --test system_credential_contract -- --ignored` | Passed: 1 test, 0 failed; created/read/deleted an ephemeral macOS Keychain item |
| `scripts/generate-swift-bindings.sh` | Passed; both generated runs matched |
| `swift test --package-path apps/macos` | Plain CLT mode cannot import `Testing`; explicit `SIGNAL_SWIFT_CLT_TESTING=1` mode builds but SwiftPM reports no executed tests |
| Direct Swift Testing runner fallback | Passed: 117 tests in 8 suites, 0 failed; includes 1 Task 13 alpha acceptance test |
| `swift build --package-path apps/macos -c release` | Passed |
| `scripts/build-macos-app.sh` | Passed |
| `scripts/verify-macos-app.sh` | Passed |
| `cargo check --workspace --all-targets --all-features --target x86_64-pc-windows-gnu` | Passed; installed cross target used |
| `git diff --check` | Passed |
| `scripts/smoke-test-macos-app.sh` | Passed after monitor fix; actual process started and stopped |

The local Swift fallback rebuilt the SwiftPM test products with the explicit Command Line Tools Testing framework paths, recompiled SwiftPM's generated runner with `-F /Library/Developer/CommandLineTools/Library/Developer/Frameworks`, relinked it as an executable with `lib_TestingInterop` on its rpath, and invoked it with `--testing-library swift-testing`. This is test execution, not a build-only claim.

## Bundle and launch inspection

- `file` reports `Mach-O 64-bit executable arm64` for `Contents/MacOS/AI Daily Signal` and `Mach-O 64-bit dynamically linked shared library arm64` for `Contents/Frameworks/libsignal_ffi.dylib`.
- `otool -L` reports `@rpath/libsignal_ffi.dylib` from the executable.
- `otool -l` reports `@executable_path/../Frameworks` as an `LC_RPATH`.
- `otool -D` reports `@rpath/libsignal_ffi.dylib` as the dylib install name.
- `plutil` reports bundle ID `com.AIDailySignal.AI-Daily-Signal`, `LSUIElement = true`, and `LSMinimumSystemVersion = 26.0`.
- `codesign --verify --deep --strict` passed. `codesign -dv` reports a local ad-hoc signature, no Team identifier, and an arm64 thin bundle. The build added it only after strict verification reported the loader edits had invalidated the linker signature.
- The source and bundled icons have identical SHA-256 `a16eeac9870c857a1172c6f651ae6c2ce4c7ea31b16d0a49d926ef4a3c3c3f26`. `file`/`sips` report a 1024x1024 8-bit RGBA PNG. The controller supplied and visually verified this asset; Task 13 did not regenerate or replace it.
- The verifier found no absolute repository path, bundled CLI, credential sentinel, or provider-body sentinel.

The bounded launch used `open -n -F -g` with stdout and stderr redirected, an isolated `HOME` whose Application Support root was `~/Library/Application Support/com.AIDailySignal.AI-Daily-Signal`, and `PATH=/usr/bin:/bin:/usr/sbin:/sbin`. `lsappinfo` found the required bundle ID, the exact bundled executable process was alive within ten seconds, and `osascript` quit it within five seconds. The process PATH contained no repository CLI directory. Captured stdout/stderr, all regular bundle files, and all regular isolated Application Support files were scanned without finding credential/provider-body sentinels.

## CI and documentation

The existing three-platform normal and credential-contract matrices are unchanged. A separate `macos-companion` job on `macos-latest` installs Rust 1.98.0, generates bindings, builds the real CLI needed by the shared-process contract, runs all bridge tests, runs Swift tests, builds Swift release, assembles the app, and verifies the bundle. It deliberately makes no GUI-launch, screenshot, signing, notarization, or paid-provider claim.

`docs/macos-alpha.md` and the README describe source building/copying, standalone operation without a CLI, optional shared CLI state, foreground refresh, first-run source installation, source management, provider consent, Keychain/environment credentials, exact supported platform, and the alpha boundaries.

## Deferred and not claimed

- public signing, notarization, downloads, installers, and updates;
- scheduled/background refresh, launch at login, and notifications;
- GitHub/changelog/announcement collectors;
- Intel and pre-macOS-26 support;
- Xcode UI automation and screenshot baselines;
- native remote CI run links for this local commit.

No full-Xcode GUI automation, screenshot, public-signing, notarization, or remote-CI result is fabricated. The only local signature is ad hoc and the only GUI evidence is the bounded real-process launch/quit smoke test.
