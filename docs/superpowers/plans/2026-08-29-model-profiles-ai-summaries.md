# Model Profiles and AI Summaries Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add secure cross-platform model profiles and opt-in, budgeted AI summary variants to the shared Rust core and `signal` CLI without making deterministic briefings depend on an LLM.

**Architecture:** `signal-core` gains focused model, credential, provider, and summary-coordinator modules. Nonsecret profiles and immutable summary variants live in SQLite; credential values live only in the OS credential store or a named environment variable. Provider adapters translate their APIs into one typed result, while the coordinator performs cache lookup, atomic budget reservation, validation, persistence, and Smart fallback after deterministic ranking.

**Tech Stack:** Rust 1.98, Tokio, Reqwest, Rusqlite, Serde, Keyring 4 `v1`, Secrecy, Async-trait, Wiremock, Clap, Rpassword.

**Spec:** `docs/superpowers/specs/2026-08-29-model-profiles-ai-summaries-design.md`

## Global Constraints

- Rust remains pinned to `1.98.0`; formatting and Clippy warnings are CI failures.
- The CLI and `signal-core` must compile and test on macOS, Ubuntu, and Windows.
- Collection, ranking, Raw, and Smart behavior must remain functional without a profile or credential.
- AI runs only after deterministic briefing selection and never changes story scores or ranking.
- Secrets must never enter SQLite, TOML, logs, errors, JSON, command arguments, or generated shell commands.
- Automated tests must not contact paid provider APIs.
- Existing milestone-one JSON fields and database data must remain compatible.
- Monetary persistence and decisions use integer micro-US-dollars, never floating point.
- Provider redirects are disabled; custom endpoints require HTTPS except loopback HTTP.
- Automatic paid-profile fallback is disabled.

---

### Task 1: Model profile domain and persistence

**Files:**
- Create: `crates/signal-core/src/models.rs`
- Create: `crates/signal-core/migrations/003_model_profiles.sql`
- Create: `crates/signal-core/tests/model_profiles_test.rs`
- Modify: `Cargo.toml`
- Modify: `crates/signal-core/Cargo.toml`
- Modify: `crates/signal-core/src/lib.rs`
- Modify: `crates/signal-core/src/storage.rs`

**Interfaces:**
- Produces: `ProviderKind`, `ApiDialect`, `CredentialRef`, `ModelProfile`, `NewModelProfile`, `ProfileLimits`, and `MoneyMicros`.
- Produces: `Store::{create_model_profile, list_model_profiles, find_model_profile, find_model_profile_by_name, set_default_model_profile, default_model_profile, remove_model_profile}`.
- Preserves: all milestone-one migrations, store methods, and schema data.

- [ ] **Step 1: Add failing profile validation and persistence tests**

Create fixed-clock tests proving profile round trips, unique case-insensitive names, arbitrary model IDs, custom endpoint rules, exact decimal money parsing, default selection, and removal clearing only the default reference:

```rust
#[test]
fn multiple_profiles_round_trip_and_default_selection_is_persisted() {
    let store = signal_core::test_support::temporary_store();
    let openai = signal_core::test_support::model_profile("personal", ProviderKind::OpenAi);
    let anthropic = signal_core::test_support::model_profile("research", ProviderKind::Anthropic);
    store.create_model_profile(&openai).unwrap();
    store.create_model_profile(&anthropic).unwrap();
    store.set_default_model_profile(Some(anthropic.id)).unwrap();
    assert_eq!(store.list_model_profiles().unwrap().len(), 2);
    assert_eq!(store.default_model_profile().unwrap().unwrap().name, "research");
}

#[test]
fn custom_http_endpoint_is_allowed_only_for_loopback() {
    assert!(NewModelProfile::fixture_with_endpoint("http://127.0.0.1:8080/v1").validate().is_ok());
    assert!(NewModelProfile::fixture_with_endpoint("http://provider.example/v1").validate().is_err());
}

#[test]
fn usd_values_parse_without_floating_point() {
    assert_eq!(MoneyMicros::parse_usd("1.234567").unwrap().as_micros(), 1_234_567);
    assert!(MoneyMicros::parse_usd("0.0000001").is_err());
}
```

- [ ] **Step 2: Run the focused tests and record RED**

Run:

```bash
cargo test -p signal-core --features test-support --test model_profiles_test
```

Expected: compilation fails because the profile types and store methods do not exist.

- [ ] **Step 3: Add exact profile types and validation**

Implement serializable public types with these fields:

```rust
pub enum ProviderKind { OpenAi, Anthropic, Gemini, OpenAiCompatible }
pub enum ApiDialect { Responses, ChatCompletions }
pub enum CredentialRef {
    SystemStore { service: String, account: String },
    Environment { variable: String },
}
pub struct ProfileLimits {
    pub max_summaries_per_refresh: u32,
    pub max_daily_cost_microusd: Option<u64>,
    pub input_cost_microusd_per_million: Option<u64>,
    pub output_cost_microusd_per_million: Option<u64>,
    pub max_output_tokens: u32,
    pub timeout_seconds: u64,
    pub max_retries: u32,
}
pub struct ModelProfile {
    pub id: uuid::Uuid,
    pub name: String,
    pub provider: ProviderKind,
    pub model: String,
    pub endpoint: Option<url::Url>,
    pub dialect: Option<ApiDialect>,
    pub credential: CredentialRef,
    pub consented_at: Option<chrono::DateTime<chrono::Utc>>,
    pub enabled: bool,
    pub limits: ProfileLimits,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
```

Add `uuid = { version = "1", features = ["serde", "v4"] }` to workspace dependencies. Official-provider profiles require `endpoint = None` and `dialect = None`; custom profiles require both a base endpoint and a dialect. Validation rejects blank names/models, invalid environment variable names, other provider/dialect mismatches, zero limits, incomplete price pairs, monetary budgets without rates, credential service names outside `com.AIDailySignal.signal`, endpoint URL user-info, and non-HTTPS non-loopback custom endpoints.

Extend `test_support` with `temporary_store()` and `model_profile(name, provider)` fixtures using fixed timestamps and deterministic UUIDs derived from the fixture name.

- [ ] **Step 4: Add migration 003 and version-selected execution**

Create `model_profiles` and `app_settings` tables. Store enum fields as stable snake-case strings, nested limits as scalar columns, UUIDs as lowercase strings, timestamps as RFC 3339, and the default profile under `app_settings.key = 'default_model_profile_id'`.

Refactor migration selection into a registry:

```rust
const MIGRATIONS: &[(i64, &str)] = &[
    (2, include_str!("../migrations/002_briefing_item_staleness.sql")),
    (3, include_str!("../migrations/003_model_profiles.sql")),
];
```

Execute only versions missing from `schema_migrations`, inside the existing immediate migration transaction. Keep migration 001's idempotent bootstrap unchanged.

- [ ] **Step 5: Implement profile store methods and row conversion**

Use immediate transactions for create/default/remove. Enforce `lower(name)` uniqueness in the schema and convert invalid database enum strings into `SignalError::Serialization`. Removing the default profile clears the setting in the same transaction.

- [ ] **Step 6: Verify Task 1**

Run:

```bash
cargo test -p signal-core --features test-support --test model_profiles_test
cargo test -p signal-core --features test-support --test storage_test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

- [ ] **Step 7: Commit Task 1**

```bash
git add Cargo.toml Cargo.lock crates/signal-core
git commit -m "feat: persist model profiles"
```

---

### Task 2: Cross-platform credential boundary

**Files:**
- Create: `crates/signal-core/src/credentials.rs`
- Create: `crates/signal-core/tests/credentials_test.rs`
- Modify: `Cargo.toml`
- Modify: `crates/signal-core/Cargo.toml`
- Modify: `crates/signal-core/src/lib.rs`
- Modify: `crates/signal-core/src/error.rs`

**Interfaces:**
- Consumes: `CredentialRef` and immutable profile IDs from Task 1.
- Produces: `CredentialStore`, `SystemCredentialStore`, `MemoryCredentialStore` under `test-support`, `CredentialResolver`, and `ResolvedCredential`.
- Produces: `SignalError::Credential(String)` whose display never contains backend details or secret values.

- [ ] **Step 1: Write failing credential isolation tests**

```rust
#[test]
fn profiles_resolve_separate_system_credentials() {
    let vault = MemoryCredentialStore::default();
    let first = CredentialRef::for_profile(uuid::Uuid::new_v4());
    let second = CredentialRef::for_profile(uuid::Uuid::new_v4());
    vault.set(&first, SecretString::from("alpha".to_owned())).unwrap();
    vault.set(&second, SecretString::from("beta".to_owned())).unwrap();
    assert_eq!(vault.expose_for_test(&first), "alpha");
    assert_eq!(vault.expose_for_test(&second), "beta");
}

#[test]
fn sentinel_secret_never_appears_in_debug_or_serialized_reference() {
    let secret = "SENTINEL-DO-NOT-LEAK";
    let resolved = ResolvedCredential::new(secret.to_owned());
    assert!(!format!("{resolved:?}").contains(secret));
    assert!(!serde_json::to_string(&CredentialRef::for_profile(uuid::Uuid::new_v4())).unwrap().contains(secret));
}
```

Add environment tests for missing, empty, non-Unicode, and present values using a test-only injected environment reader rather than mutating the process environment in parallel tests.

- [ ] **Step 2: Run the focused test and record RED**

```bash
cargo test -p signal-core --features test-support --test credentials_test
```

- [ ] **Step 3: Add secret dependencies and the credential traits**

Add:

```toml
keyring = "4.1.6"
secrecy = "0.10.3"
```

Implement:

```rust
pub trait CredentialStore: Send + Sync {
    fn set(&self, reference: &CredentialRef, secret: secrecy::SecretString) -> Result<()>;
    fn get(&self, reference: &CredentialRef) -> Result<secrecy::SecretString>;
    fn delete(&self, reference: &CredentialRef) -> Result<()>;
}

pub trait EnvironmentReader: Send + Sync {
    fn read(&self, variable: &str) -> Result<Option<String>>;
}
```

`CredentialResolver` dispatches explicitly by `CredentialRef`; it never falls through from keyring to environment. `ResolvedCredential` wraps `SecretString`, exposes the value only to provider request construction, and has a redacted `Debug` implementation.

`CredentialRef::for_profile(id)` always returns service `com.AIDailySignal.signal` and account `model-profile/{lowercase-uuid}`. Validation accepts only that exact service/account derivation for system-store profiles, so a profile cannot address unrelated keyring entries.

- [ ] **Step 4: Implement the production system store**

Use `keyring::v1::Entry` with the profile reference's service/account. Map `NoEntry` to a generic missing-credential category, make delete idempotent, and map every backend error to a redacted `SignalError::Credential` message.

Never include `keyring::Error` debug/display text in public errors because platform backends may include account or service details.

- [ ] **Step 5: Add compensated credential creation helper**

Add a helper used later by `SignalApp`:

```rust
pub fn persist_system_credential_then<T>(
    store: &dyn CredentialStore,
    reference: &CredentialRef,
    secret: SecretString,
    persist: impl FnOnce() -> Result<T>,
) -> Result<T>
```

If `persist()` fails, delete the newly stored credential best-effort and return the original persistence error.

- [ ] **Step 6: Verify Task 2 on host and Windows target**

```bash
cargo test -p signal-core --features test-support --test credentials_test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo check --workspace --all-targets --all-features --target x86_64-pc-windows-gnu
```

- [ ] **Step 7: Commit Task 2**

```bash
git add Cargo.toml Cargo.lock crates/signal-core
git commit -m "feat: add secure credential resolution"
```

---

### Task 3: Immutable summary variants and atomic budgets

**Files:**
- Create: `crates/signal-core/src/summaries.rs`
- Create: `crates/signal-core/migrations/004_ai_summaries.sql`
- Create: `crates/signal-core/tests/summary_storage_test.rs`
- Modify: `crates/signal-core/src/domain.rs`
- Modify: `crates/signal-core/src/lib.rs`
- Modify: `crates/signal-core/src/storage.rs`

**Interfaces:**
- Produces: `AiSummaryFields`, `SummarySettings`, `SummaryVariant`, `GenerationAttempt`, `GenerationStatus`, `GenerationFailureKind`, `AttemptOutcome`, `BudgetReservation`, `BudgetDecision`, and `GenerationReport`.
- Produces: `Store::{find_cached_summary, insert_summary_variant, list_summary_variants, reserve_generation, finalize_generation, select_story_summary}`.
- Extends: `BriefingItem` with `#[serde(default)] pub selected_summary: Option<SummaryVariant>` while preserving existing serialized fields.

- [ ] **Step 1: Add failing cache, migration, and concurrency tests**

Tests must prove:

```rust
#[test]
fn forced_variants_can_share_a_cache_key_and_newest_is_selected() {
    let store = signal_core::test_support::temporary_store();
    let older = signal_core::test_support::summary_variant(
        "variant-old", "same-cache-key", signal_core::test_support::fixed_now()
    );
    let newer = signal_core::test_support::summary_variant(
        "variant-new", "same-cache-key", signal_core::test_support::fixed_now() + chrono::Duration::seconds(1)
    );
    store.insert_summary_variant(&older).unwrap();
    store.insert_summary_variant(&newer).unwrap();
    assert_eq!(store.find_cached_summary("same-cache-key").unwrap().unwrap().id, newer.id);
}

#[test]
fn cache_identity_changes_for_every_specified_input() {
    let fixture = signal_core::test_support::cache_identity_fixture();
    let baseline = signal_core::summary_cache_key(
        &fixture.story,
        &fixture.profile,
        &fixture.prompt_version,
        &fixture.settings,
    ).unwrap();
    for changed in fixture.each_single_field_changed() {
        assert_ne!(
            baseline,
            signal_core::summary_cache_key(
                &changed.story,
                &changed.profile,
                &changed.prompt_version,
                &changed.settings,
            ).unwrap()
        );
    }
}

#[test]
fn two_connections_cannot_reserve_past_the_daily_budget() {
    let fixture = signal_core::test_support::shared_budget_store(1_000_000);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let handles = [uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2)].map(|attempt_id| {
        let store = fixture.store.clone();
        let profile = fixture.profile.clone();
        let barrier = barrier.clone();
        let now = fixture.now;
        let expires_at = fixture.expires_at;
        std::thread::spawn(move || {
            barrier.wait();
            store.reserve_generation(
                &profile,
                attempt_id,
                now,
                750_000,
                expires_at,
            ).unwrap()
        })
    });
    barrier.wait();
    let decisions = handles.map(|handle| handle.join().unwrap());
    assert_eq!(decisions.iter().filter(|value| matches!(value, &BudgetDecision::Reserved(_))).count(), 1);
    assert_eq!(decisions.iter().filter(|value| matches!(value, &BudgetDecision::Exhausted)).count(), 1);
}

#[test]
fn milestone_one_database_migrates_without_losing_briefings_or_saved_state() {
    let fixture = signal_core::test_support::version_two_database();
    let store = Store::open(&fixture.path).unwrap();
    assert_eq!(store.load_latest_briefing().unwrap().unwrap().items[0].story.id, "story-1");
    assert!(store.find_story("story-1").unwrap().unwrap().is_saved);
    assert!(store.list_model_profiles().unwrap().is_empty());
}
```

Use fixed UUIDs and fixed `DateTime<Utc>` values. Assert that a sentinel credential never appears in any table by querying `sqlite_schema` plus every text column introduced by migrations 003 and 004.

- [ ] **Step 2: Run focused tests and record RED**

```bash
cargo test -p signal-core --features test-support --test summary_storage_test
```

- [ ] **Step 3: Implement the summary domain and canonical cache key**

`AiSummaryFields` has required `what_happened`/`why_it_matters` and optional `caveat`. Validation rejects blank/oversized values, HTML, Markdown links, and unknown JSON fields by deserializing with `#[serde(deny_unknown_fields)]`.

Use stable snake-case persistence values for `GenerationFailureKind::{CredentialMissing, Authentication, RateLimited, Timeout, Transport, ProviderRejected, ProviderUnavailable, MalformedOutput}`. Budget exhaustion and missing consent are report-only skip reasons because no provider attempt is started.

Define the cache-related API exactly:

```rust
pub struct SummarySettings {
    pub what_happened_max_chars: u32,
    pub why_it_matters_max_chars: u32,
    pub caveat_max_chars: u32,
}

pub fn summary_cache_key(
    story: &Story,
    profile: &ModelProfile,
    prompt_version: &str,
    settings: &SummarySettings,
) -> Result<String>;
```

Implement `summary_cache_key(story, profile, prompt_version, settings)` as lowercase SHA-256 over canonical JSON containing the exact inputs from the spec. Endpoint identity is normalized but credentials are excluded.

Extend `test_support` with the fixed `summary_variant`, `cache_identity_fixture`, `shared_budget_store`, and `version_two_database` builders used above. `each_single_field_changed()` returns one mutation each for normalized title, excerpt, canonical URL, publication time, category, sorted source IDs, provider, endpoint, model, dialect, prompt version, output limit, and every summary-setting scalar.

- [ ] **Step 4: Add migration 004**

Create `summary_variants` and `generation_attempts`, then add nullable `selected_summary_variant_id` to `briefing_items`. Both history tables snapshot provider/model metadata and use nullable profile foreign keys with `ON DELETE SET NULL`; profile removal must never cascade historical variants or attempts. Index `summary_variants(cache_key, generated_at DESC, id ASC)` without making the cache key unique. Add indexes for attempt date/profile/status and story variants.

Store costs as checked nonnegative integers. Store error categories only, never provider bodies.

- [ ] **Step 5: Implement atomic reservations and finalization**

`reserve_generation(profile, attempt_id, now, estimate, expires_at)` derives the UTC usage date from `now`, starts an immediate transaction, sums completed actual cost plus unexpired active reservation estimates for that profile/date, and inserts a reserved attempt only when the total stays within the profile limit. Profiles without a daily monetary limit still receive attempts for usage accounting:

```rust
pub fn reserve_generation(
    &self,
    profile: &ModelProfile,
    attempt_id: uuid::Uuid,
    now: chrono::DateTime<chrono::Utc>,
    estimated_cost_microusd: u64,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> Result<BudgetDecision>;
```

`finalize_generation` accepts one typed outcome:

```rust
pub enum AttemptOutcome {
    Completed { input_tokens: Option<u64>, output_tokens: Option<u64>, cost_microusd: u64 },
    FailedCharged { category: GenerationFailureKind, cost_microusd: u64 },
    FailedUncharged { category: GenerationFailureKind },
}
```

Finalization is idempotent only when the existing final row is byte-for-byte equivalent; conflicting second finalization is a storage error.

- [ ] **Step 6: Round-trip selected variants through briefing reads/writes**

Extend briefing insert/load queries to persist the selected variant ID and hydrate the full variant. A deleted profile must not delete its variants. Story deletion may cascade variants because stories are application-owned records.

- [ ] **Step 7: Verify and commit Task 3**

```bash
cargo test -p signal-core --features test-support --test summary_storage_test
cargo test -p signal-core --features test-support --test storage_test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
git add crates/signal-core
git commit -m "feat: persist AI summary variants and budgets"
```

---

### Task 4: Provider contract, prompt, retries, and redaction

**Files:**
- Create: `crates/signal-core/src/providers/mod.rs`
- Create: `crates/signal-core/src/providers/retry.rs`
- Create: `crates/signal-core/src/providers/parse.rs`
- Create: `crates/signal-core/tests/provider_contract_test.rs`
- Modify: `Cargo.toml`
- Modify: `crates/signal-core/Cargo.toml`
- Modify: `crates/signal-core/src/lib.rs`
- Modify: `crates/signal-core/src/error.rs`

**Interfaces:**
- Consumes: profiles, `ResolvedCredential`, story content, and summary types.
- Produces: `SummaryProvider`, `ProviderRequest`, `ProviderResponse`, `ProviderUsage`, `ProviderFailure`, `ProviderFailureKind`, `RequestChargeStatus`, `ProviderRegistry`, `build_ai_summary_prompt`, and `parse_ai_summary`.

- [ ] **Step 1: Write failing provider-neutral contract tests**

Test strict structured parsing, scalar limits, cache-independent prompt determinism, retry classification, bounded `Retry-After`, response-size rejection, and redacted formatting:

```rust
#[test]
fn provider_failure_debug_never_contains_response_or_credential() {
    let failure = ProviderFailure::for_test("SENTINEL-SECRET", "SENTINEL-BODY");
    let rendered = format!("{failure:?} {failure}");
    assert!(!rendered.contains("SENTINEL"));
}
```

- [ ] **Step 2: Run focused tests and record RED**

```bash
cargo test -p signal-core --features test-support --test provider_contract_test
```

- [ ] **Step 3: Add provider dependencies and typed contract**

Add `async-trait = "0.1"`, `fastrand = "2"`, and dev dependency `wiremock = "0.6.5"`.

```rust
#[async_trait::async_trait]
pub trait SummaryProvider: Send + Sync {
    async fn generate(
        &self,
        request: &ProviderRequest,
        credential: &ResolvedCredential,
    ) -> std::result::Result<ProviderResponse, ProviderFailure>;
}
```

The request owns `story_id`, normalized model, endpoint, system text, user text, timeout, and maximum output tokens. The response owns strict summary fields and optional usage. `story_id` is internal correlation metadata and is never serialized into a provider request body.

`ProviderFailure` contains only a typed `ProviderFailureKind` and `RequestChargeStatus::{NotSent, PossiblySent}`. It contains no raw HTTP body, credential, URL user-info, or backend error text. Adapters mark pre-send construction/connection failures `NotSent`; timeouts or other failures after transmission may have begun are `PossiblySent`. The coordinator uses that value to choose `AttemptOutcome::FailedUncharged` or `AttemptOutcome::FailedCharged` with the reservation estimate.

Define a total, tested mapping from every `ProviderFailureKind` to the matching persisted `GenerationFailureKind`; no adapter may supply a free-form persisted category.

- [ ] **Step 4: Build the hardened shared HTTP client**

Create one client with redirects disabled, compression support retained, a fixed application user agent, and no default authorization header. Read response bytes through a 256 KiB cap before JSON parsing. Provider-specific code adds sensitive headers per request.

- [ ] **Step 5: Implement retry policy**

Retry only timeouts, `429`, and `5xx`. Do not retry authentication or other `4xx`. Clamp `Retry-After` to the profile's complete retry horizon. Production jitter is bounded; tests inject a no-sleep recorder through a `RetrySleeper` trait.

- [ ] **Step 6: Implement prompt and strict parser**

Use prompt version constant `AI_SUMMARY_PROMPT_VERSION = "ai-summary-v1"`. Canonically serialize the approved story fields and require the exact JSON object from the spec. Reject code fences, leading commentary, unknown fields, and trailing content.

- [ ] **Step 7: Verify and commit Task 4**

```bash
cargo test -p signal-core --features test-support --test provider_contract_test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
git add Cargo.toml Cargo.lock crates/signal-core
git commit -m "feat: define AI provider contract"
```

---

### Task 5: OpenAI and custom OpenAI-compatible adapters

**Files:**
- Create: `crates/signal-core/src/providers/openai.rs`
- Create: `crates/signal-core/tests/openai_provider_test.rs`
- Modify: `crates/signal-core/src/providers/mod.rs`

**Interfaces:**
- Produces: `OpenAiProvider` supporting official Responses plus custom Responses and Chat Completions dialects.
- Preserves: opaque user-selected model IDs and normalized custom endpoints.

- [ ] **Step 1: Write failing Wiremock adapter tests**

Cover:

- official `POST /v1/responses`, bearer header, `store: false`, model preservation, and output extraction across multiple response items;
- custom Responses path construction;
- custom Chat Completions request/response mapping;
- usage parsing;
- redirect refusal;
- `401` without retry;
- `429` then success with one retry; and
- malformed/oversized body redaction.

Inspect captured requests to assert the sentinel credential appears only in the authorization header.

- [ ] **Step 2: Run tests and record RED**

```bash
cargo test -p signal-core --features test-support --test openai_provider_test
```

- [ ] **Step 3: Implement Responses mapping**

Send `model`, `instructions`, `input`, `max_output_tokens`, `store: false`, and strict JSON-format instructions. Parse all message/output-text blocks rather than indexing the first item. Treat incomplete/failed statuses as typed provider failures.

- [ ] **Step 4: Implement Chat Completions mapping**

For custom profiles, treat the configured endpoint as a base URL: remove trailing slashes and append `/responses` for the Responses dialect or `/chat/completions` for Chat Completions. Send system and user messages, selected model, output limit, and JSON response-format request where compatible. Parse exactly one assistant text result; missing or multiple conflicting text choices are malformed.

- [ ] **Step 5: Verify and commit Task 5**

```bash
cargo test -p signal-core --features test-support --test openai_provider_test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
git add crates/signal-core
git commit -m "feat: add OpenAI summary adapters"
```

---

### Task 6: Anthropic adapter

**Files:**
- Create: `crates/signal-core/src/providers/anthropic.rs`
- Create: `crates/signal-core/tests/anthropic_provider_test.rs`
- Modify: `crates/signal-core/src/providers/mod.rs`

**Interfaces:**
- Produces: `AnthropicProvider` for `POST /v1/messages` using `x-api-key` and `anthropic-version: 2023-06-01`.

- [ ] **Step 1: Write failing Wiremock tests**

Assert exact headers, preserved model, system/user mapping, `max_tokens`, nonstreaming response parsing, usage parsing, multi-text-block concatenation, redacted errors, no auth retry, and transient retry behavior.

- [ ] **Step 2: Run tests and record RED**

```bash
cargo test -p signal-core --features test-support --test anthropic_provider_test
```

- [ ] **Step 3: Implement the adapter**

Use the official Messages structure and shared retry/client/parser boundary. Ignore non-text content blocks, but reject a response with no text. Never include Anthropic error-body text in `ProviderFailure`.

- [ ] **Step 4: Verify and commit Task 6**

```bash
cargo test -p signal-core --features test-support --test anthropic_provider_test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
git add crates/signal-core
git commit -m "feat: add Anthropic summary adapter"
```

---

### Task 7: Gemini adapter

**Files:**
- Create: `crates/signal-core/src/providers/gemini.rs`
- Create: `crates/signal-core/tests/gemini_provider_test.rs`
- Modify: `crates/signal-core/src/providers/mod.rs`

**Interfaces:**
- Produces: `GeminiProvider` for `models/{model}:generateContent` using `x-goog-api-key` and `x-goog-api-client` headers.
- Preserves: the opaque profile model string while normalizing one optional leading `models/` prefix for URL construction.

- [ ] **Step 1: Write failing Wiremock tests**

Assert the key is a header and never a query parameter, model path percent-encoding is safe, system instruction and contents mapping are exact, response candidates and usage parse correctly, blocked/empty candidates become typed failures, and retry/redaction behavior matches the shared policy.

- [ ] **Step 2: Run tests and record RED**

```bash
cargo test -p signal-core --features test-support --test gemini_provider_test
```

- [ ] **Step 3: Implement the adapter**

Accept either a bare Gemini model ID or one leading `models/` prefix. Strip that prefix only for path construction, reject only an empty value or control characters, and percent-encode the complete remaining identifier as one path segment so embedded slashes, query delimiters, and fragments cannot change the route. Preserve the original opaque profile model string in persistence, cache identity, and reports. Send the credential only as a sensitive `x-goog-api-key` header. Include `x-goog-api-client: ai-daily-signal/0.1.0`.

- [ ] **Step 4: Verify and commit Task 7**

```bash
cargo test -p signal-core --features test-support --test gemini_provider_test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
git add crates/signal-core
git commit -m "feat: add Gemini summary adapter"
```

---

### Task 8: AI coordinator and application integration

**Files:**
- Create: `crates/signal-core/src/generator.rs`
- Create: `crates/signal-core/tests/ai_generation_test.rs`
- Modify: `crates/signal-core/src/app.rs`
- Modify: `crates/signal-core/src/lib.rs`
- Modify: `crates/signal-core/src/storage.rs`

**Interfaces:**
- Consumes: deterministic `PipelineOutput`, profiles, credentials, providers, cache, and budget storage.
- Produces: `AiGenerationCoordinator`, `RefreshOptions`, `SummarizeOptions`, `SummarizeReport`, `RemoveModelReport`, and `CredentialWarningKind`.
- Extends: `RefreshReport` with `#[serde(default)] pub generation: GenerationReport`.
- Produces: `SignalApp::{list_models, add_model, use_model, test_model, remove_model, summarize_story, refresh_with_options}` while `refresh(now)` delegates to automatic default options.

Use these option/report shapes so callers and tests agree:

```rust
pub struct RefreshOptions {
    pub ai: bool,
}

impl Default for RefreshOptions {
    fn default() -> Self { Self { ai: true } }
}

pub struct RemoveModelReport {
    pub removed_profile_id: uuid::Uuid,
    pub credential_deleted: bool,
    pub warning: Option<CredentialWarningKind>,
}
```

`GenerationReport` count fields, including `skipped_budget`, use `usize`.

- [ ] **Step 1: Write failing orchestration tests with injected fakes**

Tests prove:

```rust
#[tokio::test]
async fn ranking_finishes_before_only_selected_items_are_generated() {
    let fixture = signal_core::test_support::ai_app_fixture().with_max_items(1);
    let report = fixture.app.refresh(fixture.now).await.unwrap();
    assert_eq!(report.briefing.items.len(), 1);
    assert_eq!(fixture.provider.requested_story_ids(), vec![report.briefing.items[0].story.id.clone()]);
}

#[tokio::test]
async fn cache_hits_make_no_request_and_do_not_consume_refresh_cap() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(2)
        .with_refresh_cap(1)
        .with_cached_story_at(0);
    let report = fixture.app.refresh(fixture.now).await.unwrap();
    assert_eq!(fixture.provider.request_count(), 1);
    assert_eq!(report.generation.cache_hits, 1);
    assert_eq!(report.generation.generated, 1);
    assert_eq!(report.generation.skipped_cap, 0);
}

#[tokio::test]
async fn provider_failure_keeps_smart_and_refresh_succeeds() {
    let fixture = signal_core::test_support::ai_app_fixture().with_provider_failure(ProviderFailureKind::RateLimited);
    let report = fixture.app.refresh(fixture.now).await.unwrap();
    assert!(report.briefing.items[0].selected_summary.is_none());
    assert_eq!(report.generation.provider_failures, 1);
    assert_eq!(report.generation.smart_fallbacks, 1);
}

#[tokio::test]
async fn budget_exhaustion_stops_calls_in_briefing_order() {
    let fixture = signal_core::test_support::ai_app_fixture().with_budget_for_one_request();
    let report = fixture.app.refresh(fixture.now).await.unwrap();
    assert_eq!(fixture.provider.request_count(), 1);
    assert_eq!(report.generation.generated, 1);
    assert_eq!(report.generation.skipped_budget, report.briefing.items.len() - 1);
}

#[tokio::test]
async fn no_ai_option_and_missing_default_make_no_provider_request() {
    let disabled = signal_core::test_support::ai_app_fixture();
    disabled.app.refresh_with_options(disabled.now, RefreshOptions { ai: false }).await.unwrap();
    assert_eq!(disabled.provider.request_count(), 0);
    let no_default = signal_core::test_support::ai_app_fixture().without_default_profile();
    no_default.app.refresh(no_default.now).await.unwrap();
    assert_eq!(no_default.provider.request_count(), 0);
}
```

Also cover missing consent, missing/empty credentials, malformed output, forced regeneration, and a manual regeneration selecting the new variant on the newest briefing item containing that story.

- [ ] **Step 2: Run focused tests and record RED**

```bash
cargo test -p signal-core --features test-support --test ai_generation_test
```

- [ ] **Step 3: Add injectable application services**

Production `SignalApp::open()` constructs system credentials and the real provider registry. Under `test-support`, add `SignalApp::open_with_services(paths, credential_store, environment_reader, provider_registry)` so tests use memory credentials and fake providers without global state.

Extend `test_support` with `AiAppFixture` and `RecordingProvider`; the fixture owns the temporary paths for the lifetime of the app and provides every builder method used in the tests above (`with_max_items`, `with_refresh_cap`, `with_cached_story_at`, `with_provider_failure`, `with_budget_for_one_request`, and `without_default_profile`).

- [ ] **Step 4: Implement automatic generation**

After `Pipeline::build` and stale carry-forward, process briefing items in position order. Check cache before cap/budget. For outbound requests: resolve credential, estimate tokens/cost, reserve, call with retry, validate, finalize usage, insert immutable variant, and attach it to the item. Every provider-side error increments a redacted report counter and leaves Smart selected.

Preserve the existing source refresh result and refresh bookkeeping. Storage failures remain fatal; provider failures do not.

- [ ] **Step 5: Implement manual model test and story summarization**

`test_model` uses fixed synthetic public text, warns through its returned report that cost may occur, enforces the daily budget, and stores a generation attempt but no story variant.

`summarize_story` uses the same path. Without force it may select a cache hit. With force it bypasses lookup, creates another immutable variant with the same cache key, and selects it on the newest briefing containing the story.

- [ ] **Step 6: Implement profile lifecycle compensation**

`add_model` validates consent/profile fields, uses the Task 2 compensated credential helper for system-store mode, then persists the profile. `remove_model` removes the profile transactionally and deletes an app-owned system credential afterward; a credential-delete failure returns a redacted warning result without restoring the removed profile. Environment references are never modified.

- [ ] **Step 7: Verify and commit Task 8**

```bash
cargo test -p signal-core --features test-support --test ai_generation_test
cargo test -p signal-core --all-features
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
git add crates/signal-core
git commit -m "feat: generate budgeted AI summaries"
```

---

### Task 9: Complete model and summary CLI

**Files:**
- Modify: `crates/signal-cli/Cargo.toml`
- Modify: `crates/signal-cli/src/cli.rs`
- Modify: `crates/signal-cli/src/main.rs`
- Modify: `crates/signal-cli/src/output.rs`
- Modify: `crates/signal-cli/tests/cli_test.rs`
- Modify: `crates/signal-cli/tests/help_test.rs`

**Interfaces:**
- Produces commands: `models list|add|use|test|remove`, `summarize`, and `refresh --no-ai`.
- Preserves: existing global `--json`/`--plain`, milestone-one fields, outputs, and exit codes.
- Adds: exit code `6` for explicit credential/provider/budget generation failures.

- [ ] **Step 1: Add failing CLI process tests**

Use `SIGNAL_HOME`, local Wiremock-compatible test servers, and environment credential references. Cover:

- two profiles with separate environment variables persist across processes;
- `models list --json` never contains sentinel keys;
- `models use` changes the default;
- automatic refresh produces structured AI fields with a local custom endpoint;
- `refresh --no-ai` makes zero provider requests;
- provider failure exits refresh successfully with Smart fallback counts;
- `summarize` cache hit and `--force` behavior;
- `models test` explicit failure uses exit 6 and redacted stderr;
- `models remove --yes` retains historical variants; and
- help lists every new command.

- [ ] **Step 2: Run CLI tests and record RED**

```bash
cargo test -p signal-cli --test cli_test
cargo test -p signal-cli --test help_test
```

- [ ] **Step 3: Add Clap command types and exact nonsecret flags**

Add `rpassword = "7"`. `models add` supports:

```text
--name
--provider open-ai|anthropic|gemini|open-ai-compatible
--model
--endpoint
--dialect responses|chat-completions
--credential-env
--max-summaries
--daily-budget-usd
--input-usd-per-million
--output-usd-per-million
--max-output-tokens
--timeout-seconds
--max-retries
--consent-provider-data-sharing
```

A credential value is never a flag. If `--credential-env` is absent, interactive mode calls `rpassword::prompt_password`; noninteractive stdin without `--credential-env` returns a configuration error rather than reading a visible secret.

- [ ] **Step 4: Implement command dispatch and output**

Human output shows profile metadata, credential source type, consent/default state, limits, selected summary mode, provider/model, and redacted generation counts. It never shows complete custom endpoint paths when they contain user info; validation rejects URL user-info entirely.

Keep JSON envelope schema version `1` and add fields without renaming/removing existing ones. Extend `display_error` and `exit_code` for redacted AI failures.

- [ ] **Step 5: Verify and commit Task 9**

```bash
cargo test -p signal-cli --test cli_test
cargo test -p signal-cli --test help_test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release -p signal-cli
./target/release/signal --help
git add Cargo.toml Cargo.lock crates/signal-cli
git commit -m "feat: add model and AI summary CLI"
```

---

### Task 10: Documentation, credential contracts, and milestone acceptance

**Files:**
- Create: `crates/signal-core/tests/system_credential_contract.rs`
- Modify: `.github/workflows/ci.yml`
- Modify: `README.md`
- Modify: `docs/cli.md`
- Modify: `.gitignore` only if a new local test artifact requires it

**Interfaces:**
- Consumes: the complete milestone-two core and CLI.
- Produces: three-platform normal verification, gated real OS credential-store contracts, and user-facing setup/privacy/budget documentation.

- [ ] **Step 1: Add ignored real credential-store contract test**

The test generates a unique profile UUID, writes one sentinel credential under the application service, reads it, deletes it, verifies `NoEntry`, and uses a drop guard for cleanup. Mark it ignored with the reason `requires an unlocked ephemeral OS credential store` so ordinary local tests never touch a real keyring.

- [ ] **Step 2: Run the contract on the development Mac**

```bash
cargo test -p signal-core --test system_credential_contract -- --ignored --nocapture
```

Expected: set/get/delete succeeds and the sentinel never appears in output.

- [ ] **Step 3: Update three-platform CI**

Upgrade every checkout step to `actions/checkout@v5`. Keep the existing fmt/Clippy/full-test matrix.

Add a separate credential contract matrix. On Windows and macOS, run the ignored test directly in the ephemeral runner account. On Ubuntu, install and run the isolated keyring contract with these nonsecret commands:

```bash
sudo apt-get update
sudo apt-get install -y dbus-x11 gnome-keyring
dbus-run-session -- bash -lc '
  printf "\n" | gnome-keyring-daemon --unlock >/dev/null
  cargo test -p signal-core --test system_credential_contract -- --ignored
'
```

Do not enable `set -x` in the credential job and do not print the sentinel credential.

- [ ] **Step 4: Document profiles, consent, budgets, and privacy**

Update README and CLI docs with:

- all model commands and examples using environment variable names rather than values;
- OS credential-store behavior and Linux Secret Service prerequisites;
- the exact fields transmitted to providers;
- automatic post-ranking generation and `--no-ai`;
- Smart fallback behavior;
- immutable caching and force regeneration;
- per-refresh caps, user-supplied USD token rates, and conservative daily budgets;
- custom HTTPS/loopback endpoint rules;
- exit code 6; and
- confirmation that provider IDs and prices are not hardcoded catalogs.

- [ ] **Step 5: Run clean milestone verification**

```bash
cargo clean
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release -p signal-cli
./target/release/signal --help
git diff --check
```

Cross-compile from macOS when the toolchain is available:

```bash
cargo check --workspace --all-targets --all-features --target x86_64-pc-windows-gnu
```

- [ ] **Step 6: Verify acceptance criteria line by line**

Record in the task report:

1. multiple profile and separate credential evidence;
2. real ephemeral OS credential-store contract results on all matrix systems;
3. sentinel scans across database/config/output/error surfaces;
4. every provider/dialect contract-test result;
5. post-ranking automatic generation evidence;
6. per-refresh and concurrent daily budget evidence;
7. cache hit/invalidation/force evidence;
8. provider failure preserving Smart and refresh success;
9. manual test/regeneration evidence;
10. milestone-one migration preservation evidence;
11. JSON compatibility evidence; and
12. native macOS, Ubuntu, and Windows CI links.

- [ ] **Step 7: Commit Task 10**

```bash
git add .github README.md docs/cli.md crates/signal-core/tests/system_credential_contract.rs .gitignore
git commit -m "docs: document secure AI summaries"
```

---

## Final verification and review

After every task has passed its task review and fix loop:

1. Generate a whole-branch review package from the milestone-two merge base.
2. Dispatch a fresh high-reasoning reviewer against this plan, the approved spec, every task report, and the exact diff.
3. Resolve every Critical and Important finding and any Minor finding that represents incorrect behavior or missing required evidence.
4. Run fresh formatting, strict Clippy, the complete workspace suite, release build, help smoke, diff hygiene, and native three-platform CI.
5. Use `superpowers:finishing-a-development-branch` and let the user choose local merge, PR, or branch preservation.
