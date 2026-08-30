# macOS alpha packaging hardening implementation plan

> **For Codex:** Execute each task with strict red-green-refactor discipline and retain fresh command output for the Task 13 report.

**Goal:** Close the Task 13 packaging review findings with binary-safe verification, deletion and process-ownership safety, deterministic shared-process concurrency, and a reproducible Swift Testing gate.

**Architecture:** Put reusable shell safety and scan primitives in one packaging helper, exercise them with temporary adversarial fixtures, and keep the public build/verifier/smoke entry points fixed to the exact repository bundle. Launch the real bundle executable directly under an isolated environment so the smoke test owns one exact PID and can verify the environment and created state without relying on LaunchServices environment forwarding.

**Tech stack:** Bash, SwiftPM/Swift Testing, Rust/Tokio/rusqlite, Mach-O tooling (`file`, `otool`, `install_name_tool`, `codesign`), macOS plist/image tooling.

---

### Task 1: Capture packaging safety regressions

**Files:**
- Create: `scripts/test-macos-packaging-hardening.sh`
- Create: `scripts/macos-packaging-common.sh`

1. Add temporary NUL-containing credential/provider-body and generic checkout-path fixtures, plus clean and missing-input controls.
2. Add a temporary repository whose `target/macos` is a symlink to an external marker and require deletion validation to refuse it without touching the marker.
3. Add exact bundle-layout fixtures that reject symlinks, unexpected files, and CLI-like payloads.
4. Run the adversarial script before implementing helpers and retain the expected RED output.
5. Implement the smallest binary-safe scan, physical-parent deletion, and exact-layout helpers; rerun GREEN.

### Task 2: Harden bundle assembly and verification

**Files:**
- Modify: `scripts/build-macos-app.sh`
- Modify: `scripts/verify-macos-app.sh`
- Modify: `apps/macos/Resources/Info.plist`

1. Add contract assertions for exact plist icon/version metadata and inspect current Mach-O path/rpath/dependency failures.
2. Remap build prefixes, validate the physical bundle parent before deleting only the exact child, normalize install names/rpaths, then ad-hoc sign only when required.
3. Enforce an exact regular-file allowlist, reject symlinks/CLI payloads, inspect every dependency and rpath, verify icon parity and plist metadata, validate local-signature identity, and use binary-safe scans that distinguish no match from errors.
4. Build and run adversarial/verifier tests until GREEN; inspect actual binaries with the platform tools.

### Task 3: Own and isolate the standalone smoke process

**Files:**
- Modify: `scripts/smoke-test-macos-app.sh`
- Modify: `scripts/test-macos-packaging-hardening.sh`

1. Add a pre-existing-process ownership contract and state/output creation assertions; capture its RED result against the old launcher.
2. Replace ambiguous/global launch and quit behavior with a bounded direct launch of the exact bundled executable, refusing a pre-existing exact instance and tracking only the newly spawned PID.
3. Explicitly remove application-root and loader overrides, verify the child HOME/PATH/environment and bundle identity, require the app to create config and SQLite state, and TERM/KILL only the owned PID.
4. Binary-safely scan captured output, bundle files, and isolated state; rerun the real bounded smoke and the pre-existing-process adversarial case.

### Task 4: Make shared-process and Swift acceptance gates deterministic

**Files:**
- Modify: `crates/signal-ffi/tests/shared_process_contract_test.rs`
- Modify: `crates/signal-ffi/Cargo.toml`
- Create: `scripts/test-swift-testing.sh`
- Modify: `.github/workflows/ci.yml`

1. Add failing CLI provenance and true-overlap assertions to the shared-process contract.
2. Build the real CLI into a fixture-owned Cargo target, version-check it, hold an isolated SQLite write lock, prove the CLI and bridge writes overlap before releasing them, and assert both fields/revision/integrity afterward.
3. Add a committed CLT-capable Swift Testing runner that reports a nonzero executed count and proves `AlphaAcceptanceTests` ran; first record the missing-runner RED, then implement and run GREEN.
4. Use the same runner in the macOS CI job and remove the stale prebuild dependency.

### Task 5: Full verification and handoff

**Files:**
- Modify: `.superpowers/sdd/2026-08-30-macos-companion-alpha/task-13-report.md`

1. Run formatting, strict Clippy, workspace all-feature tests, ignored Keychain contract, generated bindings, committed Swift runner, release Swift build, bundle build/verifier, Windows cross-check when available, adversarial scripts, diff checks, and bounded standalone smoke.
2. Re-inspect plist, image, Mach-O dependencies/rpaths/install names, signatures, and binary-safe scan results.
3. Update the report only with observed evidence and truthful launch/tooling limitations.
4. Stage only fix-round files and commit exactly `fix: harden macOS alpha packaging`; do not push or merge.
