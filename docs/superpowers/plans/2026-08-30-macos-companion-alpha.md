# macOS Companion Alpha Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a standalone, Apple-Silicon macOS 26 menu-bar companion that exposes the existing AI Daily Signal Rust product through a native SwiftUI reading and configuration experience without requiring the CLI.

**Architecture:** A new `signal-ffi` Rust facade exports narrow records, typed errors, and operations through UniFFI 0.32.0. A Swift package wraps generated bindings behind a hand-written `BridgeClient`, drives one `@MainActor` observable model, and renders a menu-bar extra plus AppKit-hosted SwiftUI reading window. The app and optional CLI share Rust-owned configuration, SQLite, and Keychain state; Swift never accesses those stores directly.

**Tech Stack:** Rust 2024, UniFFI 0.32.0 library mode, Tokio, SQLite/WAL, macOS Keychain, Swift 6.3, SwiftUI, Observation, AppKit, Swift Package Manager, shell bundle assembly, macOS 26 arm64.

**Spec:** `docs/superpowers/specs/2026-08-30-macos-companion-alpha-design.md`

## Global Constraints

- Target macOS 26 on Apple Silicon only; do not add Intel or older-macOS compatibility branches.
- The `.app` must run without an installed CLI and without repository-relative runtime paths.
- `signal-core` owns configuration, SQLite, Keychain, collection, providers, summaries, budgets, caching, and all validation; Swift never queries SQLite or recreates this logic.
- The app and CLI use the existing macOS application identifier, Application Support paths, Keychain service, database, and source configuration.
- Credentials, credential references, provider bodies, filesystem paths, raw SQL, and unredacted backend diagnostics never cross the FFI boundary.
- Generated UniFFI files are build outputs and are not committed or hand-edited.
- The visible destinations are Today, Latest, Saved, Sources, and Settings. Do not add placeholder GitHub, search, scheduling, notifications, launch-at-login, update, signing, notarization, or CLI-installation UI.
- The app is menu-bar first, has no permanent Dock icon, and opens the welcome window on first launch.
- First-run copy uses the primary action **Build My First Briefing**; AI setup is optional.
- Use Liquid Glass only for system navigation, toolbars, the popover, selection, and primary controls. Reading content must remain quiet and high-contrast; do not create a translucent card grid.
- Reduced Transparency, Increase Contrast, keyboard navigation, and VoiceOver are functional requirements, not visual polish.
- Refresh is single-flight. Cancellation is explicit because UniFFI does not map native cancellation; conservative provider accounting remains authoritative for requests that may have been sent.
- A failed or cancelled refresh never clears the last successful briefing. Provider failure retains Smart summaries.
- Network/provider tests use deterministic loopback fixtures and never paid APIs.
- Keep the existing three-platform Rust/CLI CI matrix green; Swift and bundle jobs run only on macOS.
- Full-Xcode UI automation, screenshot baselines, public signing, and notarization remain explicitly pending because this development Mac has Command Line Tools but not full Xcode.

---

### Task 1: Make shared product state observable and mutable through `signal-core`

**Files:**
- Modify: `crates/signal-core/src/config.rs`
- Modify: `crates/signal-core/src/storage.rs`
- Modify: `crates/signal-core/src/app.rs`
- Modify: `crates/signal-core/src/lib.rs`
- Modify: `crates/signal-core/tests/storage_test.rs`
- Create: `crates/signal-core/tests/companion_state_test.rs`

**Interfaces:**
- Consumes: existing `StoreStatus.data_generation`, `ConfigRepository`, story saved state, model-profile storage, and summary-variant storage.
- Produces: `StateRevision { data_generation: u64, source_config_revision: String }`; `SignalApp::state_revision(&mut self)`, `reload_config(&mut self)`, `set_read`, `summary_variants`, and `select_summary_variant`; transactional generation bumps for database mutations.

- [ ] **Step 1: Write failing storage generation tests**

Add tests that record `Store::status().data_generation`, perform each mutation, and assert an increment of exactly one:

```rust
#[test]
fn user_visible_database_mutations_increment_data_generation() {
    let store = test_support::temporary_store();
    let before = store.status().unwrap().data_generation;
    store.set_saved("story-1", true).unwrap();
    assert_eq!(store.status().unwrap().data_generation, before + 1);

    store.set_read("story-1", true).unwrap();
    assert_eq!(store.status().unwrap().data_generation, before + 2);
}
```

Cover `set_saved`, the new `set_read`, `select_story_summary`, `create_model_profile`, `set_default_model_profile`, and `remove_model_profile`. Existing refresh commits already increment the generation and must still increment once.

- [ ] **Step 2: Run the focused tests to verify RED**

Run:

```bash
cargo test -p signal-core --test storage_test user_visible_database_mutations_increment_data_generation -- --exact
```

Expected: compile failure because `set_read` does not exist, followed by assertion failures for mutations that do not bump the counter.

- [ ] **Step 3: Add transactional generation bumps and read mutation**

Add one private SQL helper and call it inside the same transaction as each mutation:

```rust
fn bump_data_generation(transaction: &rusqlite::Transaction<'_>) -> Result<()> {
    transaction.execute(
        "UPDATE metadata SET value = CAST(value AS INTEGER) + 1 WHERE key = 'data_generation'",
        [],
    )?;
    Ok(())
}

pub fn set_read(&self, id: &str, read: bool) -> Result<()>;
```

Convert `set_saved` to an immediate transaction. Do not increment on a missing story or a failed mutation. Preserve exactly one increment for refresh commits.

- [ ] **Step 4: Write failing composite-revision and app-surface tests**

Use one temporary application root and two `SignalApp` instances:

```rust
#[test]
fn state_revision_detects_database_and_external_source_config_changes() {
    let fixture = test_support::companion_app_fixture();
    let mut app = fixture.open_app();
    let initial = app.state_revision().unwrap();

    app.set_saved("story-1", true).unwrap();
    let database_change = app.state_revision().unwrap();
    assert!(database_change.data_generation > initial.data_generation);

    fixture.disable_source_in_a_separate_app("example-feed");
    let config_change = app.state_revision().unwrap();
    assert_ne!(config_change.source_config_revision, initial.source_config_revision);
}
```

Also assert `set_read`, `summary_variants`, and `select_summary_variant` return the updated core values and never accept a variant belonging to another story.

- [ ] **Step 5: Implement the composite revision and app wrappers**

Add:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateRevision {
    pub data_generation: u64,
    pub source_config_revision: String,
}

impl ConfigRepository {
    pub fn load(&self) -> Result<AppConfig>;
    pub fn revision(&self) -> Result<String>;
}

impl SignalApp {
    pub fn reload_config(&mut self) -> Result<()>;
    pub fn state_revision(&mut self) -> Result<StateRevision>;
    pub fn set_read(&self, id: &str, read: bool) -> Result<Story>;
    pub fn summary_variants(&self, story_id: &str) -> Result<Vec<SummaryVariant>>;
    pub fn select_summary_variant(
        &self,
        story_id: &str,
        variant_id: uuid::Uuid,
    ) -> Result<SummaryVariant>;
}
```

`ConfigRepository::revision` hashes the exact bytes of the atomically stored TOML with SHA-256 and returns lowercase hexadecimal. `state_revision` reloads configuration before reading the fingerprint so a persistent companion sees external CLI edits.

- [ ] **Step 6: Run focused and full core tests**

Run:

```bash
cargo test -p signal-core --test storage_test
cargo test -p signal-core --test companion_state_test
cargo test -p signal-core --all-features
```

Expected: all pass; existing JSON and migration tests remain unchanged.

- [ ] **Step 7: Commit**

```bash
git add crates/signal-core
git commit -m "feat: expose observable companion state"
```

---

### Task 2: Add safe personal feed-source management

**Files:**
- Modify: `crates/signal-core/src/app.rs`
- Modify: `crates/signal-core/src/config.rs`
- Modify: `crates/signal-core/src/domain.rs`
- Modify: `crates/signal-core/src/lib.rs`
- Create: `crates/signal-core/tests/source_management_test.rs`

**Interfaces:**
- Consumes: `ConfigRepository::load/save/revision`, `SignalApp::reload_config`, and existing `Source`/`SourceKind::Feed`.
- Produces: `SourceOrigin`, `SourceRecord`, `NewFeedSource`, `SignalApp::list_source_records`, `add_feed_source`, `set_source_enabled`, and `remove_personal_source`.

- [ ] **Step 1: Write failing validation and persistence tests**

Cover nonempty name/category, finite weight in `0.0..=1.0`, HTTP/HTTPS URL with a host and no user info, generated stable personal ID, case-insensitive unique names, reload in a second app, and rejection before writing:

```rust
#[test]
fn personal_feed_round_trips_and_is_visible_to_another_app() {
    let fixture = test_support::companion_app_fixture();
    let mut first = fixture.open_app();
    let added = first.add_feed_source(NewFeedSource {
        name: "Personal research".into(),
        category: "research".into(),
        url: "https://example.com/personal.xml".into(),
        weight: 0.75,
        enabled: true,
    }).unwrap();
    assert_eq!(added.origin, SourceOrigin::Personal);

    let mut second = fixture.open_app();
    assert!(second.list_source_records().unwrap().contains(&added));
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```bash
cargo test -p signal-core --test source_management_test
```

Expected: compile failure for the new types and methods.

- [ ] **Step 3: Implement source origin and CRUD**

Add the exact public surface:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceOrigin { Standard, Personal }

#[derive(Clone, Debug, PartialEq)]
pub struct SourceRecord {
    pub source: Source,
    pub origin: SourceOrigin,
}

pub struct NewFeedSource {
    pub name: String,
    pub category: String,
    pub url: String,
    pub weight: f64,
    pub enabled: bool,
}

impl SignalApp {
    pub fn list_source_records(&mut self) -> Result<Vec<SourceRecord>>;
    pub fn add_feed_source(&mut self, input: NewFeedSource) -> Result<SourceRecord>;
    pub fn remove_personal_source(&mut self, id: &str) -> Result<SourceRecord>;
}
```

Generate IDs as `personal-<hyphenated UUID>`. Determine standard origin by parsing the bundled standard pack and comparing IDs, not by trusting a prefix. Standard sources may be toggled but never removed. Save a cloned candidate configuration first; only replace `self.config` after atomic persistence succeeds.

- [ ] **Step 4: Add removal, collision, and compensation tests**

Assert removal rejects standard IDs, missing IDs are typed `NotFound`, duplicate names are rejected without changing the file, and a failed atomic write leaves the in-memory configuration unchanged.

- [ ] **Step 5: Run focused and regression tests**

Run:

```bash
cargo test -p signal-core --test source_management_test
cargo test -p signal-core --test config_test
cargo test -p signal-cli --test cli_test
```

Expected: all pass; existing source enable/disable CLI behavior is preserved.

- [ ] **Step 6: Commit**

```bash
git add crates/signal-core
git commit -m "feat: manage personal feed sources"
```

---

### Task 3: Add cooperative foreground-refresh cancellation

**Files:**
- Create: `crates/signal-core/src/cancellation.rs`
- Modify: `crates/signal-core/src/error.rs`
- Modify: `crates/signal-core/src/collector.rs`
- Modify: `crates/signal-core/src/generator.rs`
- Modify: `crates/signal-core/src/app.rs`
- Modify: `crates/signal-core/src/lib.rs`
- Create: `crates/signal-core/tests/refresh_cancellation_test.rs`

**Interfaces:**
- Consumes: sequential feed collection, post-ranking AI generation, existing refresh commit behavior, and conservative generation accounting.
- Produces: cloneable `CancellationToken`, `SignalError::Cancelled`, and `SignalApp::refresh_with_control(now, options, token)`.

- [ ] **Step 1: Write failing cancellation state tests**

```rust
#[test]
fn cancellation_token_is_cloneable_and_monotonic() {
    let token = CancellationToken::new();
    let observer = token.clone();
    assert!(!observer.is_cancelled());
    token.cancel();
    assert!(observer.is_cancelled());
    assert!(matches!(observer.check(), Err(SignalError::Cancelled)));
}
```

Run `cargo test -p signal-core --test refresh_cancellation_test cancellation_token_is_cloneable_and_monotonic -- --exact`; expect compile failure.

- [ ] **Step 2: Implement the cancellation primitive**

Use only `Arc<AtomicBool>`:

```rust
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self;
    pub fn cancel(&self);
    pub fn is_cancelled(&self) -> bool;
    pub fn check(&self) -> Result<()>;
}
```

`SignalError::Cancelled` displays only `operation cancelled`.

- [ ] **Step 3: Write failing collection and refresh cancellation tests**

Use loopback feeds and deterministic fake providers to prove:

- cancellation before collection makes zero requests and records no refresh run;
- cancellation after one source stops before the next source;
- cancellation between ranking and AI makes zero provider requests;
- cancellation after a possibly sent provider call preserves its finalized conservative charge; and
- cancellation before briefing commit preserves the prior briefing and generation.

- [ ] **Step 4: Thread cancellation through the existing pipeline**

Add:

```rust
impl FeedCollector {
    pub async fn collect_all_with_cancel(
        &self,
        sources: &[Source],
        collected_at: DateTime<Utc>,
        cancellation: &CancellationToken,
    ) -> Result<CollectionReport>;
}

impl SignalApp {
    pub async fn refresh_with_control(
        &self,
        now: DateTime<Utc>,
        options: RefreshOptions,
        cancellation: &CancellationToken,
    ) -> Result<RefreshReport>;
}
```

Keep `refresh` and `refresh_with_options` as compatibility wrappers using a fresh uncancelled token. Check between source requests, before each provider dispatch, after provider completion/account finalization, and immediately before briefing commit. Do not claim to interrupt an already sent HTTP request.

- [ ] **Step 5: Run focused, AI, and workspace tests**

```bash
cargo test -p signal-core --test refresh_cancellation_test
cargo test -p signal-core --features test-support --test ai_generation_test
cargo test --workspace --all-features
```

Expected: all pass; no test contacts paid APIs.

- [ ] **Step 6: Commit**

```bash
git add crates/signal-core
git commit -m "feat: cancel foreground refreshes safely"
```

---

### Task 4: Bootstrap UniFFI and expose read-only companion snapshots

**Files:**
- Modify: `Cargo.toml`
- Modify: `.gitignore`
- Create: `crates/signal-ffi/Cargo.toml`
- Create: `crates/signal-ffi/uniffi.toml`
- Create: `crates/signal-ffi/src/lib.rs`
- Create: `crates/signal-ffi/src/error.rs`
- Create: `crates/signal-ffi/src/types.rs`
- Create: `crates/signal-ffi/src/client.rs`
- Create: `crates/signal-ffi/src/bin/uniffi-bindgen.rs`
- Create: `crates/signal-ffi/tests/snapshot_contract_test.rs`
- Create: `scripts/generate-swift-bindings.sh`

**Interfaces:**
- Consumes: `SignalApp`, `StateRevision`, Today/Latest/Saved, source records, safe model profiles, refresh status, and summary variants.
- Produces: `CompanionClient`, `CompanionSnapshot`, all FFI-safe records/enums, redacted `CompanionError`, `signal-ffi` cdylib, and deterministic Swift binding generation.

- [ ] **Step 1: Add failing FFI contract tests**

The test must open a fixture through an injected constructor and assert a populated snapshot contains the same user-visible values as `signal-core`, while debug/error output contains none of four sentinels: credential value, credential reference account, provider body, or temporary root path.

```rust
#[tokio::test]
async fn snapshot_maps_core_state_without_private_material() {
    let fixture = test_support::companion_app_fixture();
    let client = CompanionClient::for_test(fixture.open_app());
    let snapshot = client.snapshot().await.unwrap();
    assert_eq!(snapshot.today.unwrap().items[0].story.title, "A deterministic signal");
    assert!(!format!("{snapshot:?}").contains(fixture.credential_sentinel()));
}
```

- [ ] **Step 2: Add the crate and verify RED**

Pin UniFFI exactly:

```toml
[lib]
crate-type = ["cdylib", "rlib"]

[features]
bindgen-cli = ["uniffi/cli"]
test-support = ["signal-core/test-support"]

[dependencies]
signal-core = { path = "../signal-core" }
uniffi = { version = "=0.32.0", features = ["tokio"] }

[[bin]]
name = "uniffi-bindgen"
path = "src/bin/uniffi-bindgen.rs"
required-features = ["bindgen-cli"]
```

Configure generated module names explicitly in `crates/signal-ffi/uniffi.toml`:

```toml
[bindings.swift]
module_name = "SignalFFIBindings"
ffi_module_name = "CSignalFFI"
generate_module_map = true
generate_immutable_records = true
```

Run `cargo test -p signal-ffi --features test-support`; expect missing types and methods.

- [ ] **Step 3: Define the safe FFI type system**

Use UniFFI records/enums with strings for RFC 3339 timestamps and UUIDs. At minimum define:

```rust
#[derive(Clone, Debug, uniffi::Record)]
pub struct FfiStateRevision {
    pub data_generation: u64,
    pub source_config_revision: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct CompanionSnapshot {
    pub revision: FfiStateRevision,
    pub status: FfiCollectionStatus,
    pub today: Option<FfiBriefing>,
    pub latest: Vec<FfiStory>,
    pub saved: Vec<FfiStory>,
    pub sources: Vec<FfiSource>,
    pub model_profiles: Vec<FfiModelProfile>,
    pub default_model_profile_id: Option<String>,
    pub has_usable_ai_profile: bool,
}
```

Define explicit `FfiStory`, `FfiScore`, `FfiBriefingItem`, `FfiBriefing`, `FfiSummaryFields`, `FfiSummaryVariant`, `FfiSource`, `FfiModelProfile`, `FfiProfileLimits`, `FfiRefreshMetadata`, `FfiCollectionState`, `FfiProviderKind`, `FfiApiDialect`, and credential-source-kind enums. Model records expose credential source kind only.

- [ ] **Step 4: Implement typed redacted errors and snapshot mapping**

```rust
#[derive(Clone, Debug, thiserror::Error, uniffi::Error)]
pub enum CompanionError {
    #[error("setup is incomplete")] NotInitialized,
    #[error("input is invalid")] InvalidInput,
    #[error("item was not found")] NotFound,
    #[error("credential is unavailable")] CredentialUnavailable,
    #[error("provider consent is required")] ConsentRequired,
    #[error("daily budget is exhausted")] BudgetExhausted,
    #[error("provider is unavailable")] ProviderUnavailable,
    #[error("network is unavailable")] Offline,
    #[error("refresh is already running")] RefreshAlreadyRunning,
    #[error("operation was cancelled")] Cancelled,
    #[error("local storage is unavailable")] StorageUnavailable,
}
```

Map by typed `SignalError`/generation category only; discard inner strings. `CompanionClient` owns `tokio::sync::Mutex<SignalApp>` and exports an async `snapshot()` plus `state_revision()`.

- [ ] **Step 5: Add reproducible binding generation**

The bindgen binary is exactly:

```rust
fn main() {
    uniffi::uniffi_bindgen_main();
}
```

`scripts/generate-swift-bindings.sh` must:

```bash
cargo build -p signal-ffi --release
cargo run -p signal-ffi --features bindgen-cli --bin uniffi-bindgen -- \
  generate --library --language swift \
  --out-dir apps/macos/Generated \
  target/release/libsignal_ffi.dylib
```

Generate into a temporary directory first. Copy `SignalFFIBindings.swift` into `apps/macos/Generated/Swift/`; copy `CSignalFFI.h` and rename `CSignalFFI.modulemap` to `apps/macos/Generated/CSignalFFI/module.modulemap`. Keep the header beside that module map. Ignore `apps/macos/Generated/`. Generate twice into two temporary directories and `diff -ru` them in a test/script check.

- [ ] **Step 6: Run FFI, binding, and workspace verification**

```bash
cargo test -p signal-ffi --features test-support
scripts/generate-swift-bindings.sh
cargo test --workspace --all-features
git diff --check
```

Expected: snapshot contracts pass, generated Swift/header/modulemap exist only in ignored output, and all existing crates remain green.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore crates/signal-ffi scripts/generate-swift-bindings.sh
git commit -m "feat: expose companion snapshots through UniFFI"
```

---

### Task 5: Export story, source, and model mutations through the bridge

**Files:**
- Modify: `crates/signal-ffi/src/types.rs`
- Modify: `crates/signal-ffi/src/client.rs`
- Modify: `crates/signal-ffi/src/error.rs`
- Create: `crates/signal-ffi/tests/mutation_contract_test.rs`

**Interfaces:**
- Consumes: Tasks 1–2 core mutation methods and existing secure model/profile APIs.
- Produces: FFI mutation request/report records and exported methods used by the Swift `BridgeClient`.

- [ ] **Step 1: Write failing story mutation tests**

Cover `set_story_saved`, `set_story_read`, `select_summary_variant`, and `regenerate_story`. Every successful result includes the changed safe record and new revision; mismatched story/variant IDs return `NotFound` or `StorageUnavailable` without echoing identifiers.

- [ ] **Step 2: Write failing source mutation tests**

Define and test:

```rust
#[derive(Clone, Debug, uniffi::Record)]
pub struct AddFeedSourceRequest {
    pub name: String,
    pub category: String,
    pub url: String,
    pub weight: f64,
    pub enabled: bool,
}

pub async fn add_feed_source(&self, request: AddFeedSourceRequest)
    -> Result<FfiSourceMutation, CompanionError>;
pub async fn set_source_enabled(&self, id: String, enabled: bool)
    -> Result<FfiSourceMutation, CompanionError>;
pub async fn remove_personal_source(&self, id: String)
    -> Result<FfiSourceMutation, CompanionError>;
```

Assert standard source removal is rejected and source revisions change without relying on the SQLite generation.

- [ ] **Step 3: Write failing model and credential tests**

Define an input enum that carries a secret only on the call stack:

```rust
#[derive(Clone, Debug, uniffi::Enum)]
pub enum AddCredentialRequest {
    SystemStore { secret: String },
    Environment { variable: String },
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct AddModelProfileRequest {
    pub name: String,
    pub provider: FfiProviderKind,
    pub model: String,
    pub endpoint: Option<String>,
    pub dialect: Option<FfiApiDialect>,
    pub credential: AddCredentialRequest,
    pub consent_provider_data_sharing: bool,
    pub limits: FfiProfileLimitsInput,
}
```

Use decimal USD strings in `FfiProfileLimitsInput` and parse them with `MoneyMicros::parse_usd`; never use floating point for money. Tests scan SQLite, TOML, result debug output, and all error displays for the secret.

- [ ] **Step 4: Implement minimal exported operations**

Export:

```rust
set_story_saved(id: String, saved: bool)
set_story_read(id: String, read: bool)
select_summary_variant(story_id: String, variant_id: String)
regenerate_story(story_id: String, profile: Option<String>, force: bool)
add_feed_source(request: AddFeedSourceRequest)
set_source_enabled(id: String, enabled: bool)
remove_personal_source(id: String)
add_model_profile(request: AddModelProfileRequest)
set_default_model_profile(profile: String)
test_model_profile(profile: String)
remove_model_profile(profile: String)
```

All are methods on `CompanionClient`; network/provider methods are async. Convert the system-store `String` immediately to `SecretString`, drop the request before awaiting anything else, and never derive or implement `Debug` for secret-bearing requests.

- [ ] **Step 5: Run mutation, credential, and workspace tests**

```bash
cargo test -p signal-ffi --features test-support --test mutation_contract_test
cargo test -p signal-core --test credentials_test
cargo test --workspace --all-features
```

Expected: all pass; real Keychain remains gated.

- [ ] **Step 6: Commit**

```bash
git add crates/signal-ffi
git commit -m "feat: export companion mutations"
```

---

### Task 6: Export single-flight refresh and cancellation

**Files:**
- Modify: `crates/signal-ffi/src/types.rs`
- Modify: `crates/signal-ffi/src/client.rs`
- Create: `crates/signal-ffi/tests/refresh_contract_test.rs`

**Interfaces:**
- Consumes: `CancellationToken` and `SignalApp::refresh_with_control` from Task 3.
- Produces: `CompanionClient::refresh`, `cancel_operation`, operation-state records, and single-flight enforcement.

- [ ] **Step 1: Write failing single-flight tests**

Start a loopback refresh held at a barrier, then assert a second ID receives `RefreshAlreadyRunning`. Assert a different ID cannot cancel the active operation.

```rust
let first = tokio::spawn(client.clone().refresh("refresh-a".into(), true));
barrier.wait().await;
assert_eq!(
    client.refresh("refresh-b".into(), true).await.unwrap_err(),
    CompanionError::RefreshAlreadyRunning,
);
assert!(!client.cancel_operation("refresh-b".into()));
assert!(client.cancel_operation("refresh-a".into()));
```

- [ ] **Step 2: Implement operation ownership without blocking cancellation**

Keep cancellation state in a separate short-held `std::sync::Mutex<Option<ActiveRefresh>>`, not behind the async app mutex:

```rust
struct ActiveRefresh {
    id: String,
    cancellation: CancellationToken,
}

pub async fn refresh(
    &self,
    operation_id: String,
    ai: bool,
) -> Result<FfiRefreshResult, CompanionError>;

pub fn cancel_operation(&self, operation_id: String) -> bool;
```

Use an RAII cleanup guard so panic/error/cancellation cannot leave the client permanently busy. Do not hold a standard mutex across `.await`.

- [ ] **Step 3: Add failure-preservation and accounting tests**

Assert cancellation preserves the previous snapshot, provider failure produces a successful refresh report with Smart fallback counts, total source failure maps to `Offline` while retaining cached Today, and a possibly sent provider call remains conservatively charged.

- [ ] **Step 4: Run bridge and core cancellation suites**

```bash
cargo test -p signal-ffi --features test-support --test refresh_contract_test
cargo test -p signal-core --test refresh_cancellation_test
cargo test --workspace --all-features
```

- [ ] **Step 5: Commit**

```bash
git add crates/signal-ffi
git commit -m "feat: bridge cancellable refreshes"
```

---

### Task 7: Build the Swift package, bridge adapter, and observable state model

**Files:**
- Create: `apps/macos/Package.swift`
- Create: `apps/macos/Sources/SignalAppKit/Models/SnapshotModels.swift`
- Create: `apps/macos/Sources/SignalAppKit/Bridge/BridgeClient.swift`
- Create: `apps/macos/Sources/SignalAppKit/Bridge/UniFFIBridgeClient.swift`
- Create: `apps/macos/Sources/SignalAppKit/State/AppModel.swift`
- Create: `apps/macos/Sources/SignalAppKit/State/AppPreferences.swift`
- Create: `apps/macos/Sources/SignalAppKit/Formatting/SignalFormatters.swift`
- Create: `apps/macos/Tests/SignalAppKitTests/AppModelTests.swift`
- Create: `apps/macos/Tests/SignalAppKitTests/FakeBridgeClient.swift`

**Interfaces:**
- Consumes: ignored UniFFI Swift/header/modulemap output from Tasks 4–6.
- Produces: local Swift value models, `BridgeClient`, production adapter, `AppModel`, deterministic fake, and state/formatting APIs used by every view task.

- [ ] **Step 1: Create the package manifest and verify the generated module links**

Use:

```swift
// swift-tools-version: 6.2
import Foundation
import PackageDescription

let packageRoot = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let repositoryRoot = packageRoot.deletingLastPathComponent().deletingLastPathComponent()
let rustReleaseDirectory = repositoryRoot.appendingPathComponent("target/release").path

let package = Package(
    name: "AIDailySignalMac",
    platforms: [.macOS(.v26)],
    products: [
        .library(name: "SignalAppKit", targets: ["SignalAppKit"]),
        .executable(name: "SignalMacApp", targets: ["SignalMacApp"]),
    ],
    targets: [
        .systemLibrary(name: "CSignalFFI", path: "Generated/CSignalFFI"),
        .target(
            name: "SignalFFIBindings",
            dependencies: ["CSignalFFI"],
            path: "Generated/Swift",
            linkerSettings: [.unsafeFlags([
                "-L", rustReleaseDirectory,
                "-lsignal_ffi",
                "-Xlinker", "-rpath",
                "-Xlinker", "@executable_path/../Frameworks",
            ])]
        ),
        .target(name: "SignalAppKit", dependencies: ["SignalFFIBindings"]),
        .executableTarget(name: "SignalMacApp", dependencies: ["SignalAppKit"]),
        .testTarget(name: "SignalAppKitTests", dependencies: ["SignalAppKit"]),
    ]
)
```

The generation script arranges UniFFI output into those two generated target paths and supplies linker flags for `libsignal_ffi.dylib`. Run `scripts/generate-swift-bindings.sh && swift build --package-path apps/macos`; expected initial failure is missing app/state source.

- [ ] **Step 2: Write failing `AppModel` state-transition tests**

Test exact transitions:

```swift
@Test @MainActor
func refreshIsSingleFlightAndCancellationUsesTheActiveIdentifier() async {
    let bridge = FakeBridgeClient(snapshot: .fixture)
    let model = AppModel(bridge: bridge, preferences: MemoryAppPreferences())
    await model.refresh()
    #expect(bridge.refreshIdentifiers.count == 1)
    model.cancelRefresh()
    #expect(bridge.cancelledIdentifiers == bridge.refreshIdentifiers)
}
```

Also cover welcome completion after offline initialization, snapshot replacement, composite revision polling/coalescing, stale/offline/storage states, selection retention, secret-field clearing, and user-facing error copy that never includes fake backend detail.

- [ ] **Step 3: Define the hand-written Swift boundary**

```swift
protocol BridgeClient: Sendable {
    func snapshot() async throws -> AppSnapshot
    func stateRevision() async throws -> StateRevision
    func refresh(operationID: String, ai: Bool) async throws -> RefreshResult
    func cancelOperation(id: String) -> Bool
    func setSaved(storyID: String, saved: Bool) async throws -> Story
    func setRead(storyID: String, read: Bool) async throws -> Story
    func selectSummary(storyID: String, variantID: String) async throws -> SummaryVariant
    func regenerate(storyID: String, profile: String?, force: Bool) async throws -> GenerationResult
    func addSource(_ input: FeedSourceInput) async throws -> Source
    func setSourceEnabled(id: String, enabled: Bool) async throws -> Source
    func removeSource(id: String) async throws -> Source
    func addModel(_ input: ModelProfileInput) async throws -> ModelProfile
    func setDefaultModel(_ selector: String) async throws -> ModelProfile
    func testModel(_ selector: String) async throws -> ModelTestResult
    func removeModel(_ selector: String) async throws -> ModelRemovalResult
}
```

Local models are immutable `Sendable`, `Equatable`, and `Identifiable` where relevant. They contain only display-safe fields.

- [ ] **Step 4: Implement `@MainActor AppModel`**

Use Observation:

```swift
@MainActor @Observable
final class AppModel {
    private(set) var snapshot: AppSnapshot?
    private(set) var phase: AppPhase = .loading
    private(set) var activeOperationID: String?
    var destination: Destination = .today
    var selectedStoryID: String?

    func start() async
    func buildFirstBriefing() async
    func refresh(ai: Bool = true) async
    func cancelRefresh()
    func pollRevisionWhileActive() async
}
```

Use one `Task` for polling with a two-second interval, cancel it on inactive state, and never overlap reloads. Secret form values remain view-local and are cleared with `defer` around the bridge call.

- [ ] **Step 5: Implement the UniFFI adapter and formatting**

Map generated records to local types in one file. Do not expose generated types elsewhere. Use `Date.ISO8601FormatStyle` and `RelativeDateTimeFormatter`; invalid bridge timestamps become a safe unknown date rather than a crash.

- [ ] **Step 6: Run Swift and Rust verification**

```bash
scripts/generate-swift-bindings.sh
swift test --package-path apps/macos
swift build --package-path apps/macos
cargo test -p signal-ffi --features test-support
```

Expected: all pass with Swift 6 strict-concurrency checks enabled.

- [ ] **Step 7: Commit**

```bash
git add apps/macos
git commit -m "feat: add macOS companion state model"
```

---

### Task 8: Implement the welcome, menu-bar extra, and reading-window shell

**Required design skill:** Read and apply `frontend-design:frontend-design` before editing views.

**Files:**
- Create: `apps/macos/Sources/SignalMacApp/SignalMacApp.swift`
- Create: `apps/macos/Sources/SignalMacApp/AppEnvironment.swift`
- Create: `apps/macos/Sources/SignalMacApp/AppDelegate.swift`
- Create: `apps/macos/Sources/SignalAppKit/Window/WindowCoordinator.swift`
- Create: `apps/macos/Sources/SignalAppKit/Views/WelcomeView.swift`
- Create: `apps/macos/Sources/SignalAppKit/Views/MenuBarContentView.swift`
- Create: `apps/macos/Sources/SignalAppKit/Views/ReadingWindowView.swift`
- Create: `apps/macos/Sources/SignalAppKit/Views/EmptyStateView.swift`
- Create: `apps/macos/Tests/SignalAppKitTests/AppPresentationTests.swift`

**Interfaces:**
- Consumes: `AppModel`, `AppPreferences`, destinations, phases, and safe snapshot records from Task 7.
- Produces: accessory lifecycle, first-run welcome, fixed menu popover, full-window navigation shell, and window-opening commands used by later screens.

- [ ] **Step 1: Write failing presentation-policy tests**

Test pure policy values rather than pixel snapshots:

```swift
@Test
func firstLaunchOpensWelcomeAndCompletedLaunchStaysInMenuBar() {
    #expect(AppPresentation(welcomeCompleted: false).launchAction == .openBriefing)
    #expect(AppPresentation(welcomeCompleted: true).launchAction == .remainInMenuBar)
}
```

Also assert the popover action set is exactly status, top signal, refresh/cancel, Open Briefing, Settings, and Quit; it must not expose a scrollable story feed.

- [ ] **Step 2: Implement accessory lifecycle and window ownership**

Use `NSApplication.ActivationPolicy.accessory`. `WindowCoordinator` owns one `NSWindow` with an `NSHostingController<ReadingWindowView>` and exposes:

```swift
@MainActor
public final class WindowCoordinator {
    public func open(destination: Destination = .today)
    public func close()
}
```

First launch opens it after app startup; subsequent launches remain in the menu bar until requested. Closing the window does not terminate the process.

- [ ] **Step 3: Implement the welcome view and first refresh**

Use the exact primary copy **Build My First Briefing**, include local-first/AI-optional text, and state that refreshing contacts enabled sources. Offline failure marks welcome complete and presents retry without pretending a briefing exists.

- [ ] **Step 4: Implement the menu-bar popover**

Use `MenuBarExtra(...).menuBarExtraStyle(.window)`. The icon and VoiceOver label must distinguish current, refreshing, partially stale, offline, and failed without color-only meaning. Top signal is one title plus provenance, never a list.

- [ ] **Step 5: Implement the reading shell**

Use `NavigationSplitView` with exactly Today, Latest, Saved, Sources, and Settings. Set a default window size near 1,120×760, a practical minimum around 860×600, native toolbar placement, and keyboard shortcuts for refresh (`⌘R`), open source (`⌘O` when a story is selected), save (`⌘S`), and Settings (`⌘,`).

- [ ] **Step 6: Run Swift tests/build and manual compile checks**

```bash
swift test --package-path apps/macos --filter AppPresentationTests
swift test --package-path apps/macos
swift build --package-path apps/macos
```

- [ ] **Step 7: Commit**

```bash
git add apps/macos
git commit -m "feat: add companion shell and welcome"
```

---

### Task 9: Implement Today, Latest, Saved, and story detail

**Required design skill:** Read and apply `frontend-design:frontend-design` before editing views.

**Files:**
- Create: `apps/macos/Sources/SignalAppKit/Views/TodayView.swift`
- Create: `apps/macos/Sources/SignalAppKit/Views/StoryListView.swift`
- Create: `apps/macos/Sources/SignalAppKit/Views/StoryRowView.swift`
- Create: `apps/macos/Sources/SignalAppKit/Views/StoryDetailView.swift`
- Create: `apps/macos/Sources/SignalAppKit/Views/SummaryVariantPicker.swift`
- Create: `apps/macos/Sources/SignalAppKit/Views/GenerationPopover.swift`
- Modify: `apps/macos/Sources/SignalAppKit/State/AppModel.swift`
- Create: `apps/macos/Tests/SignalAppKitTests/ReadingFlowTests.swift`

**Interfaces:**
- Consumes: snapshot stories/briefing/variants and story mutation methods from Task 7.
- Produces: finite editorial reading flows and actions for save, read, source opening, variant selection, and regeneration.

- [ ] **Step 1: Write failing reading-flow state tests**

Cover deterministic section/order preservation, Today empty state, Latest chronological input preservation, Saved removal updating both lists, selection surviving snapshot replacement by ID, and failed regeneration retaining the selected Smart/AI summary.

- [ ] **Step 2: Implement finite story lists**

Today renders briefing sections and their existing order. Latest and Saved render only the loaded snapshot; do not add pagination, infinite scrolling, engagement counters, or unread badges. Rows show title, primary source, relative time, category, summary provenance, stale marker, and saved state.

- [ ] **Step 3: Implement the story detail reading hierarchy**

Use this order:

1. source, time, category, staleness;
2. title;
3. provenance chip (`Smart` or provider/model);
4. What happened;
5. Why it matters;
6. optional Caveat;
7. compact score explanation and source list; and
8. actions.

Raw mode shows original excerpt; Smart mode shows `smartSummary`; AI mode shows only validated structured fields.

- [ ] **Step 4: Implement actions and optimistic-state discipline**

Do not permanently mutate Swift state before Rust confirms. Disable only the action in flight, replace returned records on success, and show category-based redacted error copy on failure. Open canonical URLs through `NSWorkspace.shared.open` only after constructing a valid HTTPS/HTTP `URL` from the bridge-provided string.

- [ ] **Step 5: Implement variant selection and regeneration**

The variant picker lists Raw, Smart, then immutable AI variants newest first with provider/model/time. Regeneration requires an explicit profile selection when no default exists and preserves the previous selection on failure. `force` is a labeled secondary action.

- [ ] **Step 6: Run reading tests and build**

```bash
swift test --package-path apps/macos --filter ReadingFlowTests
swift test --package-path apps/macos
swift build --package-path apps/macos
```

- [ ] **Step 7: Commit**

```bash
git add apps/macos
git commit -m "feat: add native briefing reader"
```

---

### Task 10: Implement native source management

**Required design skill:** Read and apply `frontend-design:frontend-design` before editing views.

**Files:**
- Create: `apps/macos/Sources/SignalAppKit/Views/SourcesView.swift`
- Create: `apps/macos/Sources/SignalAppKit/Views/SourceEditorView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/State/AppModel.swift`
- Create: `apps/macos/Tests/SignalAppKitTests/SourceSettingsTests.swift`

**Interfaces:**
- Consumes: safe source records and source mutations from Task 7.
- Produces: standard/personal grouped source list, add form, enable/disable actions, and personal-only removal confirmation.

- [ ] **Step 1: Write failing source form tests**

Test trimming of name/category only, preservation of URL text until Rust validates it, finite weight bounds, disabled submission while saving, standard source removal absence, confirmation for personal removal, and safe rollback after bridge failure.

- [ ] **Step 2: Implement grouped source list**

Use native `List` sections **Standard Sources** and **Personal Sources**. Rows show name, category, host, weight, origin, and enabled toggle. Standard and personal origin must have text labels, not color-only distinction.

- [ ] **Step 3: Implement the add sheet**

Use a grouped `Form` with name, feed URL, category, weight, and enabled controls. Keep validation copy local and generic; Rust remains authoritative. After success, dismiss and replace the source snapshot with the returned revision-aware state.

- [ ] **Step 4: Implement toggle/removal failure behavior**

Disable the affected row while a call is active. On failure restore the confirmed Rust state and show `The source could not be updated.` Standard rows never expose Delete.

- [ ] **Step 5: Run source UI tests/build**

```bash
swift test --package-path apps/macos --filter SourceSettingsTests
swift test --package-path apps/macos
swift build --package-path apps/macos
```

- [ ] **Step 6: Commit**

```bash
git add apps/macos
git commit -m "feat: manage feeds from the companion"
```

---

### Task 11: Implement model, credential, consent, and budget settings

**Required design skill:** Read and apply `frontend-design:frontend-design` before editing views.

**Files:**
- Create: `apps/macos/Sources/SignalAppKit/Views/SettingsView.swift`
- Create: `apps/macos/Sources/SignalAppKit/Views/ModelsSettingsView.swift`
- Create: `apps/macos/Sources/SignalAppKit/Views/ModelProfileEditorView.swift`
- Create: `apps/macos/Sources/SignalAppKit/Views/BudgetFieldsView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/State/AppModel.swift`
- Create: `apps/macos/Tests/SignalAppKitTests/ModelSettingsTests.swift`

**Interfaces:**
- Consumes: safe model records and model mutations from Task 7.
- Produces: native profile creation, secure Keychain/environment credential mode, consent, default selection, bounded paid test, budgets, and removal.

- [ ] **Step 1: Write failing secret-lifecycle and form tests**

Use a sentinel and assert it exists only in the view-local editor state before submission, is passed once to the fake bridge, is cleared in both success and failure paths, and never enters `AppModel`, error copy, snapshots, or debug output.

- [ ] **Step 2: Implement provider-aware profile fields**

Show endpoint/dialect only for OpenAI-compatible profiles. Preserve the opaque model identifier exactly. Use secure text entry for Keychain mode and a normal variable-name field for environment mode. The form supports exact defaults: 5 summaries, 384 output tokens, 30-second timeout, and 2 retries.

- [ ] **Step 3: Implement budget input without floating-point money**

Keep daily budget and per-million rates as decimal strings. Explain that all three are optional together, are sent to Rust for exact micro-USD parsing, and that conservative estimates can stop calls before the displayed cap. Do not calculate money in Swift.

- [ ] **Step 4: Implement explicit consent and paid test flow**

The consent checkbox appears beside the disclosure that approved story content is sent to the selected provider. Save is disabled until consent is explicit. Testing a model always shows `This test sends synthetic text and may incur provider cost.` before the bridge call.

- [ ] **Step 5: Implement default/removal semantics**

Default selection is available only for enabled, consented profiles. Removal requires confirmation, retains historical summaries, and shows only the safe credential-deletion warning category returned by Rust.

- [ ] **Step 6: Run model UI, FFI mutation, and Keychain tests**

```bash
swift test --package-path apps/macos --filter ModelSettingsTests
swift test --package-path apps/macos
cargo test -p signal-ffi --features test-support --test mutation_contract_test
cargo test -p signal-core --test system_credential_contract -- --ignored
```

Expected: all pass on the development Mac; captured output contains no sentinel.

- [ ] **Step 7: Commit**

```bash
git add apps/macos
git commit -m "feat: configure AI safely on macOS"
```

---

### Task 12: Finish Liquid Glass, accessibility, and deterministic visual states

**Required design skill:** Read and apply `frontend-design:frontend-design` before editing views.

**Files:**
- Create: `apps/macos/Sources/SignalAppKit/Design/VisualPolicy.swift`
- Create: `apps/macos/Sources/SignalAppKit/Design/SignalGlass.swift`
- Create: `apps/macos/Sources/SignalAppKit/Design/PreviewFixtures.swift`
- Modify: all view files under `apps/macos/Sources/SignalAppKit/Views/`
- Create: `apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift`
- Create: `apps/macos/Tests/SignalAppKitTests/PreviewFixtureTests.swift`

**Interfaces:**
- Consumes: completed view hierarchy from Tasks 8–11.
- Produces: semantic visual policy, restrained native glass, deterministic preview fixtures, accessibility labels/ordering/shortcuts, and non-Xcode test evidence.

- [ ] **Step 1: Write failing visual-policy tests**

```swift
@Test
func reducedTransparencyUsesOpaqueSurfacesAndVisibleBoundaries() {
    let policy = VisualPolicy(reduceTransparency: true, increaseContrast: false)
    #expect(policy.readingSurface == .opaque)
    #expect(policy.glassAllowed == false)
    #expect(policy.separatorEmphasis == .standard)
}
```

Cover light/dark semantic colors, increased-contrast boundaries, non-color status symbols/labels, minimum control sizing, visible keyboard focus, and stable accessibility sort priorities.

- [ ] **Step 2: Implement semantic visual policy and glass wrapper**

`SignalGlass` uses standard macOS 26 material/glass APIs and returns an opaque system background when Reduce Transparency is active. Use `GlassEffectContainer` only for related custom interactive controls. Never apply glass to story prose or every row.

- [ ] **Step 3: Apply the product hierarchy**

Use native typography, SF Symbols, system spacing, quiet reading surfaces, one restrained accent, and edge/separator depth. Remove generic gradient hero cards, glowing borders, pill overload, and repeated translucent panels. Group forms remain native grouped forms.

- [ ] **Step 4: Add deterministic fixture coverage**

Create fixtures for welcome, empty, populated, selected AI, Smart fallback, stale partial refresh, offline cached briefing, provider failure, dark appearance, Reduced Transparency, and Increase Contrast. Tests assert unique stable IDs, valid URLs/timestamps, no secret-like fields, and every app phase is represented.

- [ ] **Step 5: Add accessibility metadata and keyboard verification**

Every icon-only control receives a label and help text. Status labels include words such as `Refreshing` or `Partially stale`. Lists and detail use meaningful headings. Add tests over pure descriptors/commands for `⌘R`, `⌘O`, `⌘S`, and `⌘,`.

- [ ] **Step 6: Run Swift tests/build**

```bash
swift test --package-path apps/macos --filter AccessibilityPolicyTests
swift test --package-path apps/macos --filter PreviewFixtureTests
swift test --package-path apps/macos
swift build --package-path apps/macos
```

- [ ] **Step 7: Commit**

```bash
git add apps/macos
git commit -m "feat: polish accessible Liquid Glass UI"
```

---

### Task 13: Assemble the standalone app and close alpha acceptance

**Files:**
- Create: `apps/macos/Resources/Info.plist`
- Create: `apps/macos/Resources/AppIcon.png`
- Create: `scripts/build-macos-app.sh`
- Create: `scripts/verify-macos-app.sh`
- Modify: `.github/workflows/ci.yml`
- Modify: `README.md`
- Create: `docs/macos-alpha.md`
- Create: `crates/signal-ffi/tests/shared_process_contract_test.rs`
- Create: `apps/macos/Tests/SignalAppKitTests/AlphaAcceptanceTests.swift`

**Interfaces:**
- Consumes: complete Rust bridge, Swift package, UI, generated bindings, and existing CLI.
- Produces: `target/macos/AI Daily Signal.app`, launch smoke test, shared app/CLI acceptance proof, secret scan, macOS CI job, and personal-use documentation.

- [ ] **Step 1: Create the product icon asset**

Use the `imagegen` skill to generate one 1024×1024 icon with this exact art direction: a refined macOS app icon, deep graphite rounded-square field, one luminous calm signal arc rising through a subtle frosted layer, restrained cobalt-to-cyan light, precise Apple-like depth, no text, no robot, no brain, no sparkles, no generic AI circuitry. Export a single PNG and verify it remains legible at 32×32.

- [ ] **Step 2: Write failing bundle-structure verification**

`scripts/verify-macos-app.sh` checks:

```text
AI Daily Signal.app/Contents/Info.plist
AI Daily Signal.app/Contents/MacOS/AI Daily Signal
AI Daily Signal.app/Contents/Frameworks/libsignal_ffi.dylib
AI Daily Signal.app/Contents/Resources/AppIcon.png
```

It asserts bundle ID `com.AIDailySignal.AI-Daily-Signal`, `LSUIElement=true`, minimum system `26.0`, executable arm64 architecture, `@rpath/libsignal_ffi.dylib` linkage, no absolute repository path, no bundled `signal` CLI, and no credential/provider-body test sentinel.

Run before the build script exists; expected failure is missing bundle.

- [ ] **Step 3: Implement deterministic bundle assembly**

`scripts/build-macos-app.sh` runs:

```bash
scripts/generate-swift-bindings.sh
cargo build -p signal-ffi --release
swift build --package-path apps/macos -c release
```

Then it recreates only `target/macos/AI Daily Signal.app`, copies the Swift executable and dylib, applies `install_name_tool -id @rpath/libsignal_ffi.dylib`, writes the approved plist/resources, and performs ad-hoc signing only when `codesign` reports it is required for local launch. It must never delete outside `target/macos/AI Daily Signal.app`.

- [ ] **Step 4: Add shared-process and alpha acceptance tests**

The Rust contract starts with one isolated application root, mutates saved/read/source/model state through the bridge, then invokes the real CLI in separate processes and asserts both directions observe the same state and revision without corruption.

Swift acceptance tests use the fake bridge to drive welcome → offline retry → populated Today → save/read → source add → profile add/test/remove and assert every required destination/action is reachable.

- [ ] **Step 5: Add local launch and secret scans**

After building:

```bash
open -n "target/macos/AI Daily Signal.app"
```

Poll for the bundle identifier for at most ten seconds, then quit it through `osascript`. Capture stdout/stderr, scan them and every regular file in the bundle and isolated Application Support root for credential and provider-body sentinels, and confirm the app runs with a `PATH` that does not contain the repository CLI.

- [ ] **Step 6: Add macOS-only CI without weakening existing jobs**

Add a `macos-companion` job on `macos-latest` that installs Rust 1.98.0, runs the generation script, bridge tests, `swift test`, release Swift build, bundle assembly, and bundle verification. Keep the existing normal and credential-contract matrices unchanged. Do not claim GUI launch, screenshots, signing, or notarization in remote CI.

- [ ] **Step 7: Document personal installation and boundaries**

`docs/macos-alpha.md` documents:

- `scripts/build-macos-app.sh`;
- opening/copying the generated `.app`;
- standalone operation without the CLI;
- optional shared CLI state;
- Keychain and AI consent behavior;
- macOS 26/Apple Silicon requirement;
- foreground-only refresh; and
- deferred signing/notarization, scheduling, notifications, GitHub, Intel, and older macOS support.

Update README milestone text from “next milestone” to “personal macOS alpha available from source.”

- [ ] **Step 8: Run the complete clean acceptance gate**

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test -p signal-core --test system_credential_contract -- --ignored
scripts/generate-swift-bindings.sh
swift test --package-path apps/macos
swift build --package-path apps/macos -c release
scripts/build-macos-app.sh
scripts/verify-macos-app.sh
cargo check --workspace --all-targets --all-features --target x86_64-pc-windows-gnu
git diff --check
```

Also run the bounded local launch/quit smoke test. Record Xcode UI automation, screenshots, public signing/notarization, and native remote CI links as pending unless they actually exist.

- [ ] **Step 9: Commit**

```bash
git add apps/macos/Resources scripts .github/workflows/ci.yml README.md docs/macos-alpha.md crates/signal-ffi/tests/shared_process_contract_test.rs apps/macos/Tests/SignalAppKitTests/AlphaAcceptanceTests.swift
git commit -m "feat: package the macOS companion alpha"
```

---

## Final acceptance checklist

1. The standalone bundle launches on the development Apple Silicon Mac without an installed CLI or repository-relative runtime dependency.
2. Welcome initialization and first refresh work without AI configuration.
3. The menu bar reports every required state and opens the full briefing.
4. Today, Latest, Saved, Sources, and Settings render Rust-owned state.
5. Story save/read/open/variant/regeneration flows work and preserve prior content on failure.
6. Personal feeds can be added, toggled, and removed without Terminal.
7. Model profiles, consent, exact budgets, Keychain/environment credentials, paid test, default selection, and removal work without secret persistence or output.
8. Optional CLI and app changes become mutually visible through the composite state revision; concurrent access remains safe.
9. Partial, offline, provider, cancellation, and storage failures preserve the last successful briefing and remain redacted.
10. Light/dark, Reduce Transparency, Increase Contrast, keyboard, and VoiceOver policies have deterministic command-line-testable evidence.
11. Rust, UniFFI, Swift, bundle, loader, launch, Keychain, cross-target, and secret-scan gates pass without paid API calls.

## Spec coverage map

| Spec area | Implemented by |
|---|---|
| Product decisions and alpha boundary | Tasks 8, 12, 13 |
| Rust bridge and shared local state | Tasks 1–6 |
| Snapshot and operation contract | Tasks 4–6 |
| Swift state model and external-change polling | Task 7 |
| Accessory lifecycle, welcome, and menu bar | Task 8 |
| Today, Latest, Saved, and story actions | Task 9 |
| Personal source management | Tasks 2, 5, 10 |
| Models, Keychain, consent, and budgets | Tasks 5, 11 |
| Liquid Glass and accessibility fallbacks | Task 12 |
| Cancellation and failure preservation | Tasks 3, 6, 8, 9 |
| Standalone build, launch, security, CI, and docs | Task 13 |
| All eleven alpha acceptance criteria | Task 13 final gate and the final acceptance checklist above |
