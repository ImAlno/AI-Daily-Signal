# Cross-Platform CLI Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first useful vertical slice of AI Daily Signal: a deterministic RSS/Atom briefing that runs through the same `signal` CLI on macOS, Linux, and Windows.

**Architecture:** A Rust 2024 workspace contains a reusable `signal-core` library and a thin `signal-cli` binary. The core owns configuration, collection, normalization, deduplication, ranking, Smart summaries, SQLite persistence, and briefing assembly; the CLI only parses commands and renders results. This slice establishes the contracts later plans will extend with GitHub, AI providers, UniFFI, and the macOS companion.

**Tech Stack:** Rust 1.98.0, Rust 2024 edition, Tokio, Reqwest 0.13, feed-rs 2.4, Rusqlite 0.40 with bundled SQLite, Clap 4.6, Serde, TOML 1.1, Chrono, SHA-256, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-08-29-ai-daily-signal-design.md`

## Global Constraints

- Support macOS, Linux, and Windows from the first CLI release.
- Use Rust 1.98.0 stable and `edition = "2024"` for every Rust crate.
- Require no account, hosted service, AI provider, or graphical application.
- Keep all domain logic and database access in `signal-core`; `signal-cli` may only call public core interfaces.
- Use SQLite WAL mode, transactional writes, a 5-second busy timeout, and schema migrations.
- Keep network access explicit: `signal today` reads cached data, while `signal refresh` and `signal today --refresh` perform network work.
- Never install or execute content from collected sources.
- Never write secrets to SQLite, TOML configuration, logs, or command output.
- Return stable JSON envelopes with `schema_version: 1` when `--json` is supplied.
- Use fixtures for tests; the test suite must not depend on live network services.
- Run `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace --all-features` before every task commit.

## Milestone sequence

This plan is milestone 1 of the approved design. It delivers a reviewable product on its own and locks the contracts needed by later plans:

1. **This plan:** Rust foundation, standard feed sources, deterministic briefing, cross-platform CLI.
2. **Discovery expansion:** GitHub radar, repository snapshots, changelog-page monitoring, richer source overrides.
3. **AI summaries:** secure model profiles, providers, budgets, caching, and fallback.
4. **Mac companion:** UniFFI bridge, SwiftUI menu bar, Liquid Glass reader, settings, notifications.
5. **Release engineering:** signed downloads, installers, updater, then package managers.

---

### Task 1: Rust workspace and core error boundary

**Files:**
- Create: `rust-toolchain.toml`
- Create: `Cargo.toml`
- Create: `crates/signal-core/Cargo.toml`
- Create: `crates/signal-core/src/lib.rs`
- Create: `crates/signal-core/src/error.rs`
- Create: `crates/signal-cli/Cargo.toml`
- Create: `crates/signal-cli/src/main.rs`
- Create: `tests/fixtures/.gitkeep`

**Interfaces:**
- Produces: `signal_core::Result<T>` and `signal_core::SignalError` for all later core APIs.
- Produces: a `signal` executable whose temporary behavior is to print its version.

- [ ] **Step 1: Provision and verify the required toolchain**

The current development machine does not have `rustup`, `rustc`, or `cargo`. Obtain explicit user approval before installing a global toolchain, then use the official Rust installer and pin this repository:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup toolchain install 1.98.0 --profile minimal --component clippy,rustfmt
rustc --version
cargo --version
```

Expected: `rustc 1.98.0` and the matching Cargo release. Do not continue if the exact toolchain cannot be selected.

- [ ] **Step 2: Write the workspace manifests**

Create `rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.98.0"
profile = "minimal"
components = ["clippy", "rustfmt"]
```

Create the root `Cargo.toml`:

```toml
[workspace]
members = ["crates/signal-core", "crates/signal-cli"]
resolver = "3"

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.98"
license = "MIT"

[workspace.dependencies]
anyhow = "1"
chrono = { version = "0.4", features = ["serde"] }
clap = { version = "4.6", features = ["derive"] }
directories = "6"
feed-rs = { version = "2.4", features = ["sanitize"] }
reqwest = { version = "0.13", default-features = false, features = ["json", "rustls", "gzip", "brotli"] }
rusqlite = { version = "0.40", features = ["bundled", "chrono"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
tempfile = "3"
thiserror = "2"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
toml = "1.1"
url = { version = "2", features = ["serde"] }
```

Create `crates/signal-core/Cargo.toml`:

```toml
[package]
name = "signal-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
chrono.workspace = true
directories.workspace = true
feed-rs.workspace = true
reqwest.workspace = true
rusqlite.workspace = true
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
thiserror.workspace = true
toml.workspace = true
url.workspace = true

[dev-dependencies]
tempfile.workspace = true
tokio.workspace = true
```

Create `crates/signal-cli/Cargo.toml`:

```toml
[package]
name = "signal-cli"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "signal"
path = "src/main.rs"

[dependencies]
anyhow.workspace = true
clap.workspace = true
signal-core = { path = "../signal-core" }
tokio.workspace = true
```

- [ ] **Step 3: Write the failing public-error test**

Add this test to `crates/signal-core/src/lib.rs` before exporting `error`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn signal_error_is_public() {
        let error = crate::SignalError::InvalidConfiguration("bad source".into());
        assert_eq!(error.to_string(), "invalid configuration: bad source");
    }
}
```

- [ ] **Step 4: Run the test and verify the red state**

Run:

```bash
cargo test -p signal-core signal_error_is_public
```

Expected: compilation fails because `SignalError` does not exist.

- [ ] **Step 5: Implement the minimal error boundary and CLI entry point**

Create `crates/signal-core/src/error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum SignalError {
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("feed parse error: {0}")]
    Feed(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, SignalError>;
```

Create `crates/signal-core/src/lib.rs`:

```rust
mod error;

pub use error::{Result, SignalError};

#[cfg(test)]
mod tests {
    #[test]
    fn signal_error_is_public() {
        let error = crate::SignalError::InvalidConfiguration("bad source".into());
        assert_eq!(error.to_string(), "invalid configuration: bad source");
    }
}
```

Create `crates/signal-cli/src/main.rs`:

```rust
fn main() {
    println!("signal {}", env!("CARGO_PKG_VERSION"));
}
```

- [ ] **Step 6: Verify the green state and workspace hygiene**

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run -p signal-cli
```

Expected: all checks pass and the final command prints `signal 0.1.0`.

- [ ] **Step 7: Commit the workspace foundation**

```bash
git add rust-toolchain.toml Cargo.toml Cargo.lock crates tests/fixtures/.gitkeep
git commit -m "build: initialize Rust workspace"
```

---

### Task 2: Domain models, platform paths, and standard source configuration

**Files:**
- Create: `crates/signal-core/src/domain.rs`
- Create: `crates/signal-core/src/paths.rs`
- Create: `crates/signal-core/src/config.rs`
- Create: `crates/signal-core/assets/standard-sources.toml`
- Create: `crates/signal-core/tests/config_test.rs`
- Modify: `crates/signal-core/src/lib.rs`

**Interfaces:**
- Produces: `Source`, `SourceKind`, `Candidate`, `Story`, `SummaryVariant`, `Briefing`, and `BriefingItem`.
- Produces: `AppPaths::discover()` and `AppPaths::for_root(root: &Path)`.
- Produces: `ConfigRepository::{load_or_create, save}` returning `AppConfig`.

- [ ] **Step 1: Write failing configuration tests**

Create `crates/signal-core/tests/config_test.rs`:

```rust
use signal_core::{AppPaths, ConfigRepository, SourceKind};

#[test]
fn first_load_writes_the_standard_source_pack() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_root(temp.path());
    let config = ConfigRepository::new(paths).load_or_create().unwrap();

    assert!(!config.sources.is_empty());
    assert!(config.sources.iter().all(|source| source.enabled));
    assert!(matches!(config.sources[0].kind, SourceKind::Feed { .. }));
}

#[test]
fn saved_source_overrides_survive_reload() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_root(temp.path());
    let repo = ConfigRepository::new(paths);
    let mut config = repo.load_or_create().unwrap();
    config.sources[0].enabled = false;
    repo.save(&config).unwrap();

    assert!(!repo.load_or_create().unwrap().sources[0].enabled);
}
```

- [ ] **Step 2: Run the tests and verify the red state**

Run:

```bash
cargo test -p signal-core --test config_test
```

Expected: compilation fails because `AppPaths`, `ConfigRepository`, and `SourceKind` are undefined.

- [ ] **Step 3: Define the domain contract**

Create `crates/signal-core/src/domain.rs` with these exact public types:

```rust
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceKind {
    Feed { url: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub category: String,
    pub enabled: bool,
    pub weight: f64,
    pub kind: SourceKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Candidate {
    pub source_id: String,
    pub external_id: String,
    pub canonical_url: String,
    pub title: String,
    pub excerpt: String,
    pub published_at: Option<DateTime<Utc>>,
    pub collected_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScoreBreakdown {
    pub recency: f64,
    pub source_weight: f64,
    pub corroboration: f64,
    pub total: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Story {
    pub id: String,
    pub title: String,
    pub canonical_url: String,
    pub excerpt: String,
    pub category: String,
    pub published_at: Option<DateTime<Utc>>,
    pub source_ids: Vec<String>,
    pub score: ScoreBreakdown,
    pub smart_summary: String,
    pub is_read: bool,
    pub is_saved: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BriefingItem {
    pub position: u32,
    pub section: String,
    pub story: Story,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Briefing {
    pub date: NaiveDate,
    pub generated_at: DateTime<Utc>,
    pub items: Vec<BriefingItem>,
}
```

- [ ] **Step 4: Implement platform paths and atomic configuration writes**

Create `AppPaths` in `paths.rs` with public `config_dir`, `data_dir`, and `cache_dir` fields. `discover()` must use `directories::ProjectDirs::from("com", "AIDailySignal", "AI Daily Signal")`; `for_root()` must create `config`, `data`, and `cache` children beneath the supplied root.

Create these types in `config.rs`:

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct BriefingConfig {
    pub max_items: usize,
    pub stale_after_minutes: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AppConfig {
    pub briefing: BriefingConfig,
    pub sources: Vec<crate::Source>,
}

pub struct ConfigRepository {
    paths: crate::AppPaths,
}
```

`ConfigRepository::load_or_create()` must parse `config.toml` when present; otherwise it must parse `include_str!("../assets/standard-sources.toml")`, create the configuration directory, and write the file. `save()` must write `config.toml.tmp`, flush it, and rename it over `config.toml`.

- [ ] **Step 5: Add the initial standard source pack**

Create `assets/standard-sources.toml` with a seven-item deterministic testable starter pack using official feed URLs:

```toml
[briefing]
max_items = 7
stale_after_minutes = 180

[[sources]]
id = "openai-news"
name = "OpenAI News"
category = "models_products"
enabled = true
weight = 1.0
kind = { type = "feed", url = "https://openai.com/news/rss.xml" }

[[sources]]
id = "deepmind-blog"
name = "Google DeepMind"
category = "research"
enabled = true
weight = 1.0
kind = { type = "feed", url = "https://deepmind.google/blog/rss.xml" }

[[sources]]
id = "google-ai"
name = "Google AI"
category = "models_products"
enabled = true
weight = 1.0
kind = { type = "feed", url = "https://blog.google/innovation-and-ai/technology/ai/rss/" }

[[sources]]
id = "hugging-face"
name = "Hugging Face"
category = "open_source"
enabled = true
weight = 0.9
kind = { type = "feed", url = "https://huggingface.co/blog/feed.xml" }

[[sources]]
id = "arxiv-ai"
name = "arXiv Artificial Intelligence"
category = "research"
enabled = true
weight = 0.8
kind = { type = "feed", url = "https://export.arxiv.org/rss/cs.AI" }

[[sources]]
id = "eu-digital"
name = "European Commission Digital Strategy"
category = "policy"
enabled = true
weight = 0.9
kind = { type = "feed", url = "https://digital-strategy.ec.europa.eu/en/rss.xml" }

[[sources]]
id = "github-blog"
name = "GitHub Blog"
category = "developer_tools"
enabled = true
weight = 0.8
kind = { type = "feed", url = "https://github.blog/feed/" }
```

- [ ] **Step 6: Export the modules and make the tests pass**

Update `lib.rs`:

```rust
mod config;
mod domain;
mod error;
mod paths;

pub use config::{AppConfig, BriefingConfig, ConfigRepository};
pub use domain::{Briefing, BriefingItem, Candidate, ScoreBreakdown, Source, SourceKind, Story};
pub use error::{Result, SignalError};
pub use paths::AppPaths;
```

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Expected: both configuration tests and the workspace suite pass.

- [ ] **Step 7: Commit the domain and configuration contract**

```bash
git add crates/signal-core
git commit -m "feat: add domain and source configuration"
```

---

### Task 3: SQLite migrations and story repository

**Files:**
- Create: `crates/signal-core/src/storage.rs`
- Create: `crates/signal-core/migrations/001_initial.sql`
- Create: `crates/signal-core/tests/storage_test.rs`
- Modify: `crates/signal-core/src/lib.rs`

**Interfaces:**
- Consumes: `Candidate`, `Story`, `Briefing`, and `AppPaths` from Task 2.
- Produces: `Store::open`, `upsert_stories`, `commit_refresh`, `load_briefing`, `list_latest`, `list_saved`, `find_story`, `set_saved`, and `status`.
- Produces: `StoreStatus { story_count, last_refresh_at, data_generation }`.

- [ ] **Step 1: Write failing storage behavior tests**

Create `storage_test.rs` with tests that:

```rust
#[test]
fn opens_in_wal_mode_and_applies_the_initial_migration() {
    let temp = tempfile::tempdir().unwrap();
    let store = signal_core::Store::open(temp.path().join("signal.sqlite3")).unwrap();
    let status = store.status().unwrap();
    assert_eq!(status.story_count, 0);
    assert_eq!(store.journal_mode().unwrap(), "wal");
}

#[test]
fn committing_a_refresh_is_atomic_and_round_trips() {
    let temp = tempfile::tempdir().unwrap();
    let store = signal_core::Store::open(temp.path().join("signal.sqlite3")).unwrap();
    let briefing = signal_core::test_support::briefing_fixture();
    let stories = briefing.items.iter().map(|item| item.story.clone()).collect::<Vec<_>>();
    store.commit_refresh(&stories, &briefing).unwrap();
    assert_eq!(store.load_briefing(briefing.date).unwrap(), Some(briefing));
}

#[test]
fn saved_state_survives_story_upsert() {
    let temp = tempfile::tempdir().unwrap();
    let store = signal_core::Store::open(temp.path().join("signal.sqlite3")).unwrap();
    let story = signal_core::test_support::story_fixture("story-1");
    store.upsert_stories(std::slice::from_ref(&story)).unwrap();
    store.set_saved("story-1", true).unwrap();
    store.upsert_stories(&[story]).unwrap();
    assert!(store.find_story("story-1").unwrap().unwrap().is_saved);
}
```

Add `#[cfg(any(test, feature = "test-support"))] pub mod test_support` fixtures in `lib.rs`, and declare an empty `test-support` feature in `signal-core/Cargo.toml` so integration tests can use deterministic public fixtures.

- [ ] **Step 2: Run storage tests and verify the red state**

Run:

```bash
cargo test -p signal-core --features test-support --test storage_test
```

Expected: compilation fails because `Store` and its methods are undefined.

- [ ] **Step 3: Create the initial schema**

Create `migrations/001_initial.sql` with these tables and constraints:

```sql
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS stories (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    canonical_url TEXT NOT NULL UNIQUE,
    excerpt TEXT NOT NULL,
    category TEXT NOT NULL,
    published_at TEXT,
    source_ids_json TEXT NOT NULL,
    score_json TEXT NOT NULL,
    smart_summary TEXT NOT NULL,
    is_read INTEGER NOT NULL DEFAULT 0,
    is_saved INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS briefings (
    date TEXT PRIMARY KEY,
    generated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS briefing_items (
    briefing_date TEXT NOT NULL REFERENCES briefings(date) ON DELETE CASCADE,
    story_id TEXT NOT NULL REFERENCES stories(id),
    position INTEGER NOT NULL,
    section TEXT NOT NULL,
    PRIMARY KEY (briefing_date, position)
);
CREATE TABLE IF NOT EXISTS refresh_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    successful_sources INTEGER NOT NULL DEFAULT 0,
    failed_sources INTEGER NOT NULL DEFAULT 0,
    error_json TEXT
);
INSERT OR IGNORE INTO metadata(key, value) VALUES ('data_generation', '0');
```

- [ ] **Step 4: Implement `Store` with transactional writes**

`Store::open(path)` must create the parent directory, open a new Rusqlite connection per method through a private `connect()` helper, set `busy_timeout(Duration::from_secs(5))`, enable foreign keys, set `journal_mode=WAL`, and apply the migration inside `TransactionBehavior::Immediate`.

`upsert_stories()` must preserve `is_read` and `is_saved` in its conflict update. `commit_refresh()` must upsert every collected story, replace the selected date and briefing items in one transaction, then increment `metadata.data_generation`. `list_latest()` returns all retained stories rather than only briefing selections. Serialization failures must become `SignalError::Serialization` with the underlying message.

- [ ] **Step 5: Run storage and full workspace verification**

Run:

```bash
cargo test -p signal-core --features test-support --test storage_test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Expected: WAL, round-trip, and saved-state tests pass; no warnings remain.

- [ ] **Step 6: Commit the repository layer**

```bash
git add crates/signal-core
git commit -m "feat: add SQLite story repository"
```

---

### Task 4: RSS, Atom, and JSON Feed collector

**Files:**
- Create: `crates/signal-core/src/collector.rs`
- Create: `crates/signal-core/tests/feed_collector_test.rs`
- Create: `tests/fixtures/sample-rss.xml`
- Create: `tests/fixtures/sample-atom.xml`
- Create: `tests/fixtures/malformed-feed.xml`
- Modify: `crates/signal-core/src/lib.rs`

**Interfaces:**
- Consumes: enabled `SourceKind::Feed` sources.
- Produces: `FeedCollector::parse(source, bytes, collected_at) -> Result<Vec<Candidate>>`.
- Produces: `FeedCollector::fetch(source) -> Future<Result<Vec<Candidate>>>` with a reusable Reqwest client and a 20-second total timeout.
- Produces: `CollectionReport { candidates, successful_source_ids, failures }` and `SourceFailure { source_id, message }`.

- [ ] **Step 1: Add deterministic feed fixtures**

Create RSS and Atom fixtures containing the same canonical story URL but different entry IDs and titles whose normalized forms match. Include published timestamps, HTML descriptions, and a second unique story. The malformed fixture must contain a truncated XML element.

- [ ] **Step 2: Write failing parser tests**

Create `feed_collector_test.rs`:

```rust
#[test]
fn parses_rss_into_normalized_candidates() {
    let source = signal_core::test_support::feed_source("fixture");
    let bytes = include_bytes!("../../../tests/fixtures/sample-rss.xml");
    let collected_at = chrono::DateTime::parse_from_rfc3339("2026-08-29T08:00:00Z")
        .unwrap().to_utc();
    let items = signal_core::FeedCollector::parse(&source, bytes, collected_at).unwrap();

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].source_id, "fixture");
    assert!(items[0].excerpt.chars().all(|character| character != '<'));
    assert_eq!(items[0].collected_at, collected_at);
}

#[test]
fn malformed_feed_returns_a_typed_error() {
    let source = signal_core::test_support::feed_source("broken");
    let bytes = include_bytes!("../../../tests/fixtures/malformed-feed.xml");
    let error = signal_core::FeedCollector::parse(&source, bytes, chrono::Utc::now())
        .unwrap_err();
    assert!(matches!(error, signal_core::SignalError::Feed(_)));
}
```

- [ ] **Step 3: Verify the parser tests fail**

Run:

```bash
cargo test -p signal-core --features test-support --test feed_collector_test
```

Expected: compilation fails because `FeedCollector` does not exist.

- [ ] **Step 4: Implement parsing and HTTP collection**

`FeedCollector::parse()` must pass raw response bytes to `feed_rs::parser::parse`, select the first alternate link or entry ID as the canonical URL, strip and collapse whitespace from sanitized feed text, preserve optional published timestamps, and reject entries without a title or usable URL.

`FeedCollector::new()` must build one reusable Reqwest client with:

```rust
reqwest::Client::builder()
    .user_agent(concat!("AI-Daily-Signal/", env!("CARGO_PKG_VERSION")))
    .timeout(std::time::Duration::from_secs(20))
    .redirect(reqwest::redirect::Policy::limited(5))
    .build()?
```

`collect_all()` must process each enabled feed independently, retain successful candidates, and append one redacted `SourceFailure` for each failure. It must not include response bodies, headers, or credentials in failure messages.

- [ ] **Step 5: Run parser and workspace verification**

Run:

```bash
cargo test -p signal-core --features test-support --test feed_collector_test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Expected: RSS and Atom fixtures parse, malformed input returns `SignalError::Feed`, and no live request occurs during tests.

- [ ] **Step 6: Commit feed collection**

```bash
git add crates/signal-core tests/fixtures
git commit -m "feat: collect RSS and Atom sources"
```

---

### Task 5: Deterministic deduplication, ranking, summaries, and briefing assembly

**Files:**
- Create: `crates/signal-core/src/pipeline.rs`
- Create: `crates/signal-core/tests/pipeline_test.rs`
- Modify: `crates/signal-core/src/lib.rs`

**Interfaces:**
- Consumes: `Vec<Candidate>`, `AppConfig`, and a fixed `DateTime<Utc>`.
- Produces: `normalize_title`, `deduplicate`, `score_story`, `smart_summary`, and `assemble_briefing`.
- Produces: `PipelineOutput { stories: Vec<Story>, briefing: Briefing }` and `Pipeline::build(candidates, config, now) -> PipelineOutput`.

- [ ] **Step 1: Write failing pipeline tests**

Create tests with fixed timestamps that prove:

```rust
#[test]
fn canonical_url_duplicates_become_one_story_with_two_sources() {
    let candidates = signal_core::test_support::duplicate_candidates();
    let output = signal_core::Pipeline::build(
        candidates,
        &signal_core::test_support::config_fixture(),
        signal_core::test_support::fixed_now(),
    );
    assert_eq!(output.stories.len(), 1);
    assert_eq!(output.briefing.items.len(), 1);
    assert_eq!(output.briefing.items[0].story.source_ids.len(), 2);
}

#[test]
fn newer_authoritative_story_ranks_above_old_low_weight_story() {
    let output = signal_core::Pipeline::build(
        signal_core::test_support::ranked_candidates(),
        &signal_core::test_support::config_fixture(),
        signal_core::test_support::fixed_now(),
    );
    assert_eq!(output.briefing.items[0].story.title, "New official release");
}

#[test]
fn briefing_never_pads_past_available_quality_items() {
    let config = signal_core::test_support::config_with_max_items(7);
    let output = signal_core::Pipeline::build(
        vec![signal_core::test_support::candidate_fixture("only")],
        &config,
        signal_core::test_support::fixed_now(),
    );
    assert_eq!(output.briefing.items.len(), 1);
}
```

- [ ] **Step 2: Verify the pipeline tests fail**

Run:

```bash
cargo test -p signal-core --features test-support --test pipeline_test
```

Expected: compilation fails because `Pipeline` is undefined.

- [ ] **Step 3: Implement deterministic normalization and deduplication**

Normalize URLs by removing fragments, lowercasing the host, removing default ports, sorting query pairs, and removing known tracking parameters beginning with `utm_` plus `fbclid` and `gclid`. Normalize titles by Unicode-lowercasing, retaining alphanumeric token boundaries, and collapsing whitespace.

Group candidates when canonical URLs match. For different URLs, group only when normalized title token Jaccard similarity is at least `0.9` and publication times are within 48 hours. The merged story keeps the highest-weight source's title and URL, the longest clean excerpt, and a sorted unique source list.

Generate stable story IDs as lowercase hex SHA-256 of the normalized canonical URL.

- [ ] **Step 4: Implement explainable ranking and Smart summaries**

Use this phase-one score, clamped to `0.0..=100.0`:

```text
recency = max(0, 60 - age_hours * 1.25)
source_weight = clamp(source.weight, 0, 1) * 30
corroboration = min((unique_sources - 1) * 5, 10)
total = recency + source_weight + corroboration
```

Items without a publication timestamp receive `recency = 10`. Exclude items older than seven days from the daily briefing, but retain them in Latest.

`smart_summary()` must decode sanitized plain text, collapse whitespace, keep complete sentences up to 360 Unicode scalar values, and fall back to the title when no excerpt exists. It must never invent facts or add `Why it matters` in phase one.

- [ ] **Step 5: Implement briefing assembly**

Return every normalized and scored story in `PipelineOutput.stories`. For the briefing, sort by score descending, then `published_at` descending, then story ID ascending. Take at most `config.briefing.max_items`. Assign section `top_signals` and one-based positions. The date is `now.date_naive()` and `generated_at` is the supplied `now`.

- [ ] **Step 6: Verify deterministic behavior**

Run the pipeline test twice and compare serialized output:

```bash
cargo test -p signal-core --features test-support --test pipeline_test
cargo test -p signal-core --features test-support --test pipeline_test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Expected: all runs pass with identical ordering.

- [ ] **Step 7: Commit the deterministic pipeline**

```bash
git add crates/signal-core
git commit -m "feat: build deterministic daily briefings"
```

---

### Task 6: Application service and complete phase-one CLI

**Files:**
- Create: `crates/signal-core/src/app.rs`
- Create: `crates/signal-cli/src/cli.rs`
- Create: `crates/signal-cli/src/output.rs`
- Create: `crates/signal-cli/tests/cli_test.rs`
- Modify: `crates/signal-core/src/lib.rs`
- Modify: `crates/signal-cli/src/main.rs`
- Modify: `crates/signal-cli/Cargo.toml`

**Interfaces:**
- Consumes: configuration, collector, pipeline, and store from Tasks 2–5.
- Produces: `SignalApp::{open, init, refresh, today, latest, show, set_saved, saved, status, list_sources, set_source_enabled}`.
- Produces CLI commands: `init`, `refresh`, `today`, `latest`, `show`, `save`, `saved`, `status`, and `sources list|enable|disable`.
- Produces `JsonEnvelope<T> { schema_version: 1, data: T }`.

- [ ] **Step 1: Add CLI test dependencies and write failing command tests**

Add to `signal-cli/Cargo.toml`:

```toml
[dev-dependencies]
assert_cmd = "2"
predicates = "3"
serde_json.workspace = true
tempfile.workspace = true
```

Create CLI tests that pass an isolated root with `SIGNAL_HOME`:

```rust
#[test]
fn init_then_status_json_returns_schema_version_one() {
    let home = tempfile::tempdir().unwrap();
    assert_cmd::Command::cargo_bin("signal").unwrap()
        .env("SIGNAL_HOME", home.path()).arg("init").assert().success();

    let output = assert_cmd::Command::cargo_bin("signal").unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--json", "status"])
        .output().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["story_count"], 0);
}

#[test]
fn today_does_not_refresh_without_the_flag() {
    let home = tempfile::tempdir().unwrap();
    let assertion = assert_cmd::Command::cargo_bin("signal").unwrap()
        .env("SIGNAL_HOME", home.path()).arg("today").assert();
    assertion.failure().code(4).stderr(predicates::str::contains("No briefing is stored"));
}
```

- [ ] **Step 2: Verify the CLI tests fail**

Run:

```bash
cargo test -p signal-cli --test cli_test
```

Expected: tests fail because argument parsing and commands are not implemented.

- [ ] **Step 3: Implement `SignalApp` orchestration**

`SignalApp::open()` must honor `SIGNAL_HOME` through `AppPaths::for_root`; otherwise it uses `AppPaths::discover`. It loads configuration and opens `data_dir/signal.sqlite3`.

`refresh(now)` must:

1. collect every enabled feed independently;
2. build a briefing from successful candidates;
3. commit all scored stories and today's selected briefing transactionally;
4. record successful and failed source counts;
5. return `RefreshReport { briefing, successful_sources, failures }`.

If every enabled source fails, do not replace the previous briefing. Return a typed error whose CLI exit code is `3`.

- [ ] **Step 4: Implement Clap command parsing**

Use this command shape in `cli.rs`:

```rust
#[derive(clap::Parser)]
#[command(name = "signal", version, about = "A calm daily AI briefing")]
pub struct Cli {
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true, conflicts_with = "json")]
    pub plain: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(clap::Subcommand)]
pub enum Command {
    Init,
    Refresh,
    Today { #[arg(long)] refresh: bool },
    Latest { #[arg(long, default_value_t = 20)] limit: usize },
    Show { id: String },
    Save { id: String, #[arg(long)] remove: bool },
    Saved,
    Status,
    Sources { #[command(subcommand)] command: SourceCommand },
}

#[derive(clap::Subcommand)]
pub enum SourceCommand {
    List,
    Enable { id: String },
    Disable { id: String },
}
```

- [ ] **Step 5: Implement human, plain, and JSON output**

`output.rs` must contain:

```rust
#[derive(serde::Serialize)]
pub struct JsonEnvelope<T> {
    pub schema_version: u32,
    pub data: T,
}

impl<T> JsonEnvelope<T> {
    pub fn new(data: T) -> Self {
        Self { schema_version: 1, data }
    }
}
```

Human briefing output must show the date, generated time, numbered title, Smart summary, source URL, and saved marker. Plain output contains no ANSI escapes. JSON output writes only the envelope to stdout; errors always go to stderr as concise text.

Map exit codes exactly:

```text
0 success
2 invalid command or configuration
3 refresh or network failure
4 requested briefing or story not found
5 storage failure
```

- [ ] **Step 6: Make command tests pass and add cached fixture coverage**

Extend `cli_test.rs` with a helper that writes a deterministic briefing through `signal_core::Store`, then assert:

- `signal today` prints its title and never invokes the collector;
- `signal --plain today` contains no `\u{1b}[` escape;
- `signal --json today` parses and has `schema_version = 1`;
- `signal save story-1` changes `signal --json show story-1` to `is_saved = true`;
- `signal sources disable openai-news` survives a new process.

Run:

```bash
cargo test -p signal-cli --test cli_test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Expected: every CLI path passes without live network access.

- [ ] **Step 7: Perform one explicit live smoke test**

Live smoke tests are manual and not part of `cargo test`:

```bash
signal_test_home="$(mktemp -d)"
SIGNAL_HOME="$signal_test_home" cargo run -p signal-cli -- init
SIGNAL_HOME="$signal_test_home" cargo run -p signal-cli -- refresh
SIGNAL_HOME="$signal_test_home" cargo run -p signal-cli -- today
```

Use one named shell variable consistently instead of two command substitutions. Expected: at least one configured feed succeeds, a briefing is stored, and `today` performs no new network work. If external feeds are unavailable, record the external failure and rely on deterministic fixture tests rather than weakening them.

- [ ] **Step 8: Commit the phase-one CLI**

```bash
git add crates/signal-core crates/signal-cli
git commit -m "feat: add cross-platform signal CLI"
```

---

### Task 7: Cross-platform CI, documentation, and milestone acceptance

**Files:**
- Create: `.github/workflows/ci.yml`
- Create: `README.md`
- Create: `docs/cli.md`
- Create: `LICENSE`
- Modify: `.gitignore`

**Interfaces:**
- Consumes: the complete milestone-one workspace.
- Produces: reproducible macOS, Linux, and Windows verification plus user-facing setup and command documentation.

- [ ] **Step 1: Write a failing documentation smoke check**

Add a shell-independent Rust integration test `crates/signal-cli/tests/help_test.rs`:

```rust
#[test]
fn documented_primary_commands_exist_in_help() {
    let output = assert_cmd::Command::cargo_bin("signal").unwrap()
        .arg("--help").output().unwrap();
    let help = String::from_utf8(output.stdout).unwrap();
    for command in ["init", "refresh", "today", "latest", "show", "save", "saved", "status", "sources"] {
        assert!(help.contains(command), "missing {command} from help");
    }
}
```

Run `cargo test -p signal-cli --test help_test`. Expected: PASS if Task 6 is complete; this becomes the executable documentation guard.

- [ ] **Step 2: Add the three-platform CI matrix**

Create `.github/workflows/ci.yml`:

```yaml
name: CI
on:
  push:
  pull_request:
jobs:
  test:
    strategy:
      fail-fast: false
      matrix:
        os: [macos-latest, ubuntu-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: 1.98.0
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --check
      - run: cargo clippy --workspace --all-targets --all-features -- -D warnings
      - run: cargo test --workspace --all-features
```

- [ ] **Step 3: Document installation and the phase-one boundary**

`README.md` must include:

- the one-sentence product purpose;
- the current CLI-only milestone status;
- Rust 1.98 build prerequisites;
- `cargo build --release -p signal-cli`;
- the first-run sequence `signal init`, `signal refresh`, `signal today`;
- macOS, Linux, and Windows data-location behavior;
- an explicit statement that Raw and Smart summaries do not call an LLM;
- a link to the approved design spec; and
- a roadmap naming GitHub, AI summaries, and the Mac companion as later milestones.

`docs/cli.md` must document every Task 6 command, the `SIGNAL_HOME` test override, output modes, exit codes, and examples that do not contain secrets.

Use the MIT license text in `LICENSE`. Add only build artifacts, editor files, platform detritus, and local databases to `.gitignore`; do not ignore `Cargo.lock` because the workspace ships an executable.

- [ ] **Step 4: Run milestone verification from a clean build state**

Run:

```bash
cargo clean
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release -p signal-cli
./target/release/signal --help
git diff --check
```

On Windows CI, the built artifact is `target\release\signal.exe`. Expected: all local checks pass and CI passes on macOS, Ubuntu, and Windows before the milestone is accepted.

- [ ] **Step 5: Verify milestone-one acceptance criteria line by line**

Record evidence in the implementation handoff that:

1. `signal init` creates the standard source configuration.
2. `signal refresh` tolerates individual source failures and stores partial success.
3. `signal today` is cache-only and finite.
4. `signal today --refresh` performs the explicit refresh path.
5. `--plain` and `--json` behave as documented.
6. Saved state and source enablement persist across processes.
7. The same test suite passes on macOS, Linux, and Windows.
8. No AI credentials or hosted service are required.

- [ ] **Step 6: Commit the accepted CLI foundation**

```bash
git add .github .gitignore README.md LICENSE docs/cli.md crates/signal-cli/tests/help_test.rs
git commit -m "docs: document cross-platform CLI foundation"
```

## Plan self-review record

- **Spec coverage:** This milestone covers the shared Rust boundary, local-first operation, standard feed sources, deterministic ranking, Smart summaries, SQLite, explicit network behavior, saved/read state, JSON output, and three-platform CLI verification. GitHub, changelog pages, model profiles, AI summaries, UniFFI, the Mac companion, signing, and installers remain intentionally assigned to milestones 2–5 above.
- **Type consistency:** `Source`, `Candidate`, `Story`, `Briefing`, `Store`, `FeedCollector`, `Pipeline`, and `SignalApp` are introduced once and consumed by name in later tasks.
- **Completeness check:** Every task names exact files, interfaces, commands, expected outcomes, and commit boundaries; deferred product scope is enumerated as later milestones rather than hidden behind vague implementation language.
