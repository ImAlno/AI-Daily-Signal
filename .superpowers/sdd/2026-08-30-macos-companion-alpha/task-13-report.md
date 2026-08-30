# Task 13 report: standalone macOS companion alpha

## Outcome

Task 13 assembles `target/macos/AI Daily Signal.app` as a standalone macOS 26 Apple Silicon accessory app. The exact bundle contains the approved plist, Swift executable, Rust bridge dylib, controller-provided icon, and optional ad-hoc `CodeResources`; it contains no `signal` CLI or other payload.

The build is reproducible through `scripts/build-macos-app.sh`. `scripts/verify-macos-app.sh` enforces the exact bundle, plist, image, architecture, dependency, rpath, install-name, signature, path, CLI, and secret/provider-body contracts. `scripts/smoke-test-macos-app.sh` performs a bounded direct launch of the real bundled executable under an isolated environment and owns only the PID it starts.

No paid provider API was called. Provider tests used deterministic fakes or local loopback servers.

## Review hardening and TDD evidence

The fix round used temporary adversarial fixtures and targeted contracts before implementation:

- The new packaging hardening test first failed because no binary-safe scanner or physical-parent deletion helper existed. After the helper was introduced, it next failed on `CFBundleIconFile = AppIcon`, then `CFBundleShortVersionString = 0.1.0-alpha`, then the missing committed Swift runner. Its final form passes.
- NUL-containing files prove that the scanner detects the exact case-sensitive `SIGNAL_TEST_CREDENTIAL` and `SIGNAL_TEST_PROVIDER_BODY` alternatives. A clean binary containing only the generic text `ordinary-SENTINEL` remains accepted, and a missing scan input produces a distinct error result rather than being treated as no match.
- A temporary repository with `target/macos` redirected to an external directory through a symlink is refused. The external marker survives unchanged. The build now resolves the repository physically, independently creates and verifies `target` and `target/macos`, rejects symlinked components, validates the exact child basename, and deletes only `AI Daily Signal.app`.
- Exact-layout fixtures prove that unexpected regular files, symlinks, extra directories, and a CLI payload are rejected.
- The shared-process test first failed because it found the arbitrary workspace `target/debug/signal`. It now builds `--locked` into a fixture-owned Cargo target, checks `signal 0.1.0`, and invokes only that exact artifact.
- The original concurrency check merely spawned two writers. The final test holds an isolated SQLite `BEGIN IMMEDIATE` barrier, starts the separate CLI writer, starts the bridge writer on a multi-thread runtime, proves both remain pending at the same time before release, and only then commits the barrier.
- The pre-existing-process adversarial test first proved that the original smoke path did not refuse an exact existing bundle process and its bundle-ID-wide quit terminated the process it did not own. The final test refuses the launch and leaves that existing PID alive.
- The hardened verifier first rejected the old bundle metadata, then rejected a rebuilt Swift executable containing absolute worktree object paths. The build now strips non-runtime debug/object records before the final signature. NUL-containing generic checkout-path fixtures prove detection without keying the check to this repository name.
- Mach-O adversarial copies prove rejection of an extra executable rpath, a non-system absolute dependency, and a dylib rpath. Only temporary copies are mutated.
- The committed Swift runner was first absent. Its CLT path recompiles SwiftPM's generated runner with the shipped Testing framework, relinks the same object list as an executable, runs it, and requires both a nonzero summary and exactly one successful alpha acceptance test. The full-Xcode path retains normal `swift test` behavior with the same output assertions.

The original pre-fix report said the verifier found no absolute repository path, but its `grep -I` options skipped binary files. That claim was not reliable. The final evidence below comes from the binary-safe implementation without `-I`, after a rebuilt stripped bundle passed it.

## Acceptance contracts

### Shared app and CLI state

`crates/signal-ffi/tests/shared_process_contract_test.rs` creates a temporary fixture with separate application and Cargo-target roots, seeds one briefing, opens the bridge over the application root, and runs the freshly built real CLI in separate processes. It proves:

- bridge save/read mutations appear in CLI `show` and `saved` output;
- a CLI unsave appears in a new bridge snapshot;
- a bridge-created personal source appears in CLI source output;
- CLI source disable changes the bridge source record and source-configuration revision;
- CLI model add/default selection appears in the bridge snapshot;
- bridge model removal appears in CLI model output;
- the deterministic barrier proves real overlap before release;
- the overlapped CLI save and bridge read preserve both fields;
- CLI and bridge report the same final data generation; and
- SQLite reopens, returns status, and reports `PRAGMA integrity_check = ok`.

### Swift alpha flow

`apps/macos/Tests/SignalAppKitTests/AlphaAcceptanceTests.swift` drives one fake-backed `AppModel` session through welcome, offline first refresh, retry to a populated Today briefing, every required destination, save/read, personal source addition, consented model addition with transient secret clearing, explicitly confirmed fake-only model testing, and confirmed model removal.

## Verification evidence

Environment: macOS 26.6.1, Apple Silicon (`arm64`), Rust 1.98.0, Swift 6.3.3, macOS 26 SDK, Command Line Tools only (no full Xcode).

| Gate | Result |
| --- | --- |
| Shell syntax for every Task 13 script | Passed |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed |
| `cargo test --workspace --all-features` | Passed: 259 tests, 0 failed, 1 intentionally ignored Keychain contract |
| `cargo test -p signal-core --test system_credential_contract -- --ignored` | Passed: 1 test, 0 failed; ephemeral macOS Keychain item created/read/deleted |
| Targeted deterministic shared-process contract | Passed: 1 test, 0 failed; fresh CLI build and overlap assertions included |
| `scripts/generate-swift-bindings.sh` | Passed; both generated runs matched |
| `scripts/test-swift-testing.sh` | Passed: 117 tests in 8 suites, 0 failed; alpha acceptance executed exactly once |
| `swift build --package-path apps/macos -c release` | Passed |
| `scripts/test-macos-packaging-hardening.sh` | Passed, including NUL scans, scan-error distinction, symlink deletion refusal, exact layout, plist, and runner contracts |
| `scripts/build-macos-app.sh` plus exact verifier | Passed |
| `scripts/test-macos-verifier-adversarial.sh` | Passed; 3 malformed Mach-O fixtures rejected |
| `cargo check --workspace --all-targets --all-features --target x86_64-pc-windows-gnu` | Passed; installed cross target used |
| `git diff --check` | Passed |
| Pre-existing exact process ownership adversarial test | Passed; unowned PID remained alive |
| Final standalone launch/stop smoke | Passed in 2.72 seconds |

The direct CLT runner is committed and is the local/CI test entry point, not a manual fallback or build-only claim. It asserts the executed counts from the runner output before returning success.

## Bundle inspection

- `file` reports `Mach-O 64-bit executable arm64` for `Contents/MacOS/AI Daily Signal` and `Mach-O 64-bit dynamically linked shared library arm64` for `Contents/Frameworks/libsignal_ffi.dylib`.
- The executable has exactly one non-system dependency, `@rpath/libsignal_ffi.dylib`. Every other parsed dependency is under `/System/Library` or `/usr/lib`.
- The dylib identity is exactly `@rpath/libsignal_ffi.dylib`; all remaining parsed dylib dependencies are system dependencies.
- The executable has exactly one `LC_RPATH`, `@executable_path/../Frameworks`; the dylib has none. The original extra `@loader_path` was removed before signing.
- `plutil` reports identifier `com.AIDailySignal.AI-Daily-Signal`, executable `AI Daily Signal`, icon `AppIcon.png`, numeric short version `0.1.0`, bundle version `1`, package type `APPL`, `LSUIElement = true`, and minimum system version `26.0`.
- `codesign --verify --deep --strict` passed. `codesign -dv` reports an arm64 thin app, `Signature=adhoc`, and `TeamIdentifier=not set`. No public-signing claim is made.
- The source and bundled icons have identical SHA-256 `a16eeac9870c857a1172c6f651ae6c2ce4c7ea31b16d0a49d926ef4a3c3c3f26`. `file` and `sips` report a 1024x1024, 8-bit RGBA PNG. The controller supplied and visually verified this asset; Task 13 did not regenerate or replace it.
- The exact regular-file allowlist passed: plist, executable, dylib, icon, and `_CodeSignature/CodeResources`. No symlink or other directory/file exists.
- The final binary-safe scans found no checkout/build marker, bundled CLI, specific credential sentinel, or provider-body sentinel. Dependency-cache source paths are not treated as checkout leaks; generic worktree, `apps/macos`, and target build-path shapes are.

## Standalone launch inspection

LaunchServices `open --env` does not provide a strong child-environment ownership contract, so the final smoke executes the exact real bundle executable directly. This still loads the bundle's plist, resources, and `@executable_path/../Frameworks` dylib. `lsappinfo info` mapped the owned PID to the exact bundle path and bundle identifier.

The final run:

- refused any pre-existing process whose exact argv was the bundled executable;
- started PID 75314 from the exact absolute bundle executable and stopped only that PID;
- used `HOME` under a new temporary directory and `PATH=/usr/bin:/bin:/usr/sbin:/sbin`;
- explicitly removed and verified the absence of `SIGNAL_HOME`, `DYLD_LIBRARY_PATH`, `DYLD_FRAMEWORK_PATH`, and `DYLD_INSERT_LIBRARIES`;
- did not precreate the application state root;
- observed the app create both `config.toml` and `signal.sqlite3` under isolated `~/Library/Application Support/com.AIDailySignal.AI-Daily-Signal`;
- obtained `PRAGMA integrity_check = ok` from that database;
- captured app stdout/stderr and the inspected child/LaunchServices records;
- stopped normally through bounded TERM handling, with KILL available only for the still-owned exact PID fallback; and
- binary-safely scanned the bundle, captured output/records, and isolated Application Support without finding a credential/provider-body sentinel.

The measured full smoke command completed in 2.72 seconds, inside the ten-second bound. It uses no regex PID selection, global bundle-ID quit, `open`, or `osascript`.

## CI and documentation

The existing three-platform normal and credential-contract matrices are unchanged. The separate `macos-companion` job now uses the committed Swift runner, lets the shared-process test produce its own provenance-bound CLI, runs the packaging safety contracts, builds release Swift, assembles/verifies the app, and runs the Mach-O adversarial verifier. It deliberately makes no CI GUI-launch, screenshot, public-signing, notarization, or paid-provider claim. No remote CI result is claimed for this local commit.

`docs/macos-alpha.md` and the README describe source building/copying, standalone operation without a CLI, optional shared CLI state, foreground refresh, first-run source installation, source management, provider consent, Keychain/environment credentials, exact supported platform, and the alpha boundaries.

## Deferred and not claimed

- public signing, notarization, downloads, installers, and updates;
- scheduled/background refresh, launch at login, and notifications;
- GitHub/changelog/announcement collectors;
- Intel and pre-macOS-26 support;
- Xcode UI automation and screenshot baselines; and
- native remote CI run links for this local commit.

No full-Xcode GUI automation, screenshot, public-signing, notarization, or remote-CI result is fabricated. The only local signature is ad hoc, and the only GUI evidence is the bounded owned-process standalone launch/stop smoke test.
