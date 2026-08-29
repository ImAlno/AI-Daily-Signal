# Model Profiles and AI Summaries Design

**Status:** Approved for implementation planning  
**Date:** 2026-08-29  
**Parent design:** `docs/superpowers/specs/2026-08-29-ai-daily-signal-design.md`

## 1. Purpose

Milestone two adds opt-in AI explanations to the existing local-first briefing engine. Users can configure multiple provider and model profiles, keep separate credentials in operating-system secret stores, select a default profile, and generate structured AI summaries without making collection or ranking dependent on an LLM.

The deterministic pipeline remains authoritative. Raw and Smart summaries continue to work without credentials, network access to a model provider, or a successful AI request.

## 2. Scope

### Included

- Named model profiles stored as nonsecret local application data.
- OpenAI, Anthropic, Google Gemini, and custom OpenAI-compatible adapters.
- Arbitrary user-selected model identifiers.
- OpenAI Responses support for official OpenAI profiles.
- Selectable Responses or Chat Completions dialects for custom OpenAI-compatible profiles.
- Credentials stored in macOS Keychain, Windows Credential Manager, or Linux Secret Service.
- Environment-variable credential references for headless and automated use.
- One-time provider data-sharing consent before automatic AI generation.
- Automatic AI generation after deterministic ranking when a default profile is enabled.
- Per-refresh summary caps and conservative daily spending limits.
- Immutable summary variants, cache identity, usage accounting, and Smart fallback.
- CLI profile management, provider testing, one-run AI suppression, and manual story regeneration.
- Stable application interfaces suitable for a later UniFFI boundary.
- Deterministic, paid-API-free tests on macOS, Linux, and Windows.

### Excluded

- GitHub and page-change collectors.
- Swift bindings and the native macOS companion.
- Background daemons or hosted services.
- Bundled provider price catalogs or automatic price discovery.
- Package-manager distribution and signed release artifacts.
- Paid live-provider requests in automated tests.

## 3. Principles

1. **AI explains; it never ranks.** Collection, deduplication, scoring, and briefing selection finish before any provider request.
2. **Failure is additive, not destructive.** Missing credentials, provider errors, invalid output, and exhausted budgets keep the Smart summary readable and do not fail the refresh.
3. **Secrets never enter application data.** SQLite stores only credential references. Secret values never enter SQLite, TOML, logs, errors, JSON output, generated shell commands, or process arguments.
4. **Profiles are explicit.** Provider, model, endpoint, credential source, consent, limits, and prices are user-controlled and inspectable.
5. **Variants are immutable.** A new content hash, model, endpoint, prompt version, or settings combination creates a new summary variant.
6. **Budgets are conservative.** The application reserves estimated cost before a request and never assumes an unreported request was free.
7. **The core owns behavior.** The CLI remains a thin surface over `signal-core`; the later Mac app will use the same application interface and database.

## 4. Architecture

Milestone two extends `signal-core` with four focused subsystems:

```text
ranked briefing stories
          |
          v
+-------------------------+
| AI summary coordinator  |
| cache + budget + retry  |
+-------------------------+
      |             |
      v             v
credentials      provider adapters
keyring/env      OpenAI / Anthropic
                 Gemini / custom
      |             |
      +------v------+
             |
             v
 immutable summary variants
 usage ledger + briefing selection
```

- `models` owns profile validation and profile persistence operations.
- `credentials` owns secret-store and environment-variable resolution.
- `providers` owns HTTP request/response translation behind one internal interface.
- `summaries` owns prompt construction, cache keys, output validation, budget reservations, generation orchestration, and fallback reporting.

`SignalApp` composes these subsystems and exposes application-shaped methods. Provider-specific request and response types do not cross the `signal-core` public boundary.

## 5. Domain model

### 5.1 Model profile

A profile contains:

- immutable profile ID;
- unique user-facing name;
- provider kind: OpenAI, Anthropic, Gemini, or custom OpenAI-compatible;
- user-selected model identifier;
- optional custom endpoint;
- API dialect where applicable: Responses or Chat Completions;
- credential reference;
- one-time data-sharing consent timestamp;
- enabled/default state;
- maximum summaries per refresh;
- optional maximum daily spend in integer micro-US-dollars;
- optional input and output rates in integer micros per million tokens;
- request timeout;
- maximum retry count; and
- created and updated timestamps.

The default profile is a nullable application setting referencing a profile ID. Profile names can change without changing the credential entry or historical summary identity.

If a daily spending limit is configured, both input and output rates are required. This prevents a profile from claiming to enforce a monetary budget without enough information to estimate cost.

### 5.2 Credential reference

A credential reference is one of:

- `SystemStore { service, account }`; or
- `Environment { variable }`.

The service is application-owned and stable. The account incorporates the immutable profile ID rather than the display name. Environment references store only the variable name.

### 5.3 Structured AI summary

An AI summary contains:

- `what_happened`: required concise factual explanation;
- `why_it_matters`: required concise significance grounded in supplied story content;
- `caveat`: optional uncertainty or limitation;
- provider kind;
- model identifier;
- prompt version;
- generated timestamp;
- input and output token counts when reported;
- accounted cost; and
- cache key.

The existing `Story.smart_summary` remains intact. Briefing items gain an optional selected AI variant while preserving the existing Raw/Smart JSON fields.

### 5.4 Generation report

Refresh and manual generation return aggregate, nonsecret counts:

- eligible;
- generated;
- cache hits;
- skipped by per-refresh cap;
- skipped by daily budget;
- missing credentials;
- provider failures;
- malformed outputs; and
- Smart fallbacks.

No report contains raw provider response bodies or secret material.

## 6. Credential handling

The default production credential implementation uses Rust's cross-platform keyring interface:

- macOS Keychain Services;
- Windows Credential Manager; and
- Secret Service on Linux and other supported Unix desktops.

Headless profiles explicitly reference an environment variable such as `OPENAI_API_KEY`. A profile does not silently fall through from one credential source to another.

`signal models add` never accepts a secret as an option or positional argument. For system-store credentials it prompts on the terminal using hidden input. For automation it accepts a nonsecret `--credential-env <VARIABLE>` reference.

Profile creation follows a compensating transaction:

1. validate all nonsecret profile fields;
2. obtain or resolve the credential;
3. store a system credential when selected;
4. insert the profile; and
5. best-effort delete the just-created credential if profile persistence fails.

Profile removal deletes only system credentials owned by the application. It never modifies environment variables. Historical summary variants remain available after profile removal.

Tests use an in-memory credential implementation. Automated tests never write to a developer's real credential store.

## 7. Provider adapters

All adapters implement one typed asynchronous generation interface that accepts normalized instructions, story input, model profile, credential, timeout, and output limit. It returns structured text plus provider-reported usage where available.

### 7.1 OpenAI

Official OpenAI profiles call `POST /v1/responses` with:

- bearer authentication;
- `store: false`;
- the user-selected model ID;
- explicit instructions and input;
- a bounded output size; and
- structured JSON output instructions.

The adapter extracts output text without assuming that the first output item contains the final answer.

Reference: <https://developers.openai.com/api/reference/cli/resources/responses/methods/create>

### 7.2 Anthropic

Anthropic profiles call `POST /v1/messages` with:

- `x-api-key` authentication;
- the required `anthropic-version` header;
- the user-selected model ID;
- a system instruction;
- one user message; and
- a bounded `max_tokens` value.

Reference: <https://platform.claude.com/docs/en/api/messages/create>

### 7.3 Google Gemini

Gemini profiles call `models/{model}:generateContent` and send the credential in the `x-goog-api-key` header, never in the URL query string. Requests identify the application through the appropriate client header and use system instructions plus text content.

Reference: <https://ai.google.dev/api>

### 7.4 Custom OpenAI-compatible

Custom profiles require a user-supplied base endpoint and an explicit dialect:

- Responses; or
- Chat Completions.

HTTPS is mandatory except for `localhost`, `127.0.0.1`, and `[::1]`, where HTTP is allowed for development. The model ID remains an opaque user-controlled string.

Provider clients disable redirects. This prevents credentials from being forwarded to a different host. Response bodies are size-limited before parsing.

## 8. Prompt and output contract

AI receives only the already-selected story's normalized title, clean excerpt, canonical source URL, publication time, category, and source identifiers. It does not receive unrelated local history, saved-state metadata, filesystem paths, or credentials.

Prompt version `ai-summary-v1` requires one JSON object:

```json
{
  "what_happened": "string",
  "why_it_matters": "string",
  "caveat": "string or null"
}
```

Validation requires:

- exactly one object;
- nonempty `what_happened` and `why_it_matters` fields;
- an optional string or null caveat;
- maximum scalar lengths for every field;
- no HTML or Markdown links; and
- no unknown top-level fields.

The provider is instructed to use only supplied facts, state uncertainty in `caveat`, and avoid investment, medical, or legal advice. Invalid output is recorded as malformed and falls back to Smart. The application does not issue a paid repair request automatically.

## 9. Cache identity

The cache key is lowercase SHA-256 over a canonical serialized structure containing:

- story content hash;
- provider kind;
- normalized endpoint identity;
- model identifier;
- API dialect;
- prompt version;
- output limit; and
- summary settings.

The story content hash includes normalized title, clean excerpt, canonical URL, publication time, category, and sorted source IDs. Credential references and secret values are excluded.

A normal cache lookup selects the newest valid immutable variant for the cache key without making a provider request or consuming the per-refresh generation cap. Cache keys are indexed but not unique because an explicit `--force` regeneration may create another immutable variant for the same identity.

## 10. Budgeting and usage

Before a paid request, the coordinator estimates input tokens conservatively from the canonical prompt and reserves the configured maximum output tokens. It converts the estimate with the profile's user-supplied input and output rates.

The reservation and budget check occur in one immediate SQLite transaction. The transaction rejects a request when completed usage plus active reservations would exceed the daily limit.

After the request:

- reported provider usage replaces the estimate when available;
- an unreported successful request keeps the conservative estimate;
- a connection failure proven to occur before request transmission records zero actual cost;
- a timeout or other ambiguous transport failure keeps the conservative reservation because the provider may have processed the request;
- a successful HTTP response with malformed output records reported usage or the conservative estimate; and
- stale reservations expire only after the profile's complete timeout and retry horizon plus a fixed safety margin.

Integer micro-US-dollars are used for all monetary values. Floating-point currency values are never persisted or used for budget decisions.

The maximum-summaries-per-refresh cap counts outbound provider requests. Cache hits do not count. Automatic paid-profile fallback is disabled.

## 11. Generation flow

For `signal refresh`:

1. collect and normalize sources;
2. deduplicate and rank deterministically;
3. assemble the finite Smart briefing;
4. load the enabled default profile;
5. confirm stored data-sharing consent and resolve its credential;
6. inspect selected stories in briefing order;
7. use immutable cache hits first;
8. reserve budget and call the selected provider for remaining eligible items;
9. validate and persist each successful variant;
10. select successful or cached variants on briefing items;
11. retain Smart for every skipped or failed item; and
12. commit the briefing, refresh bookkeeping, and summary selections without erasing earlier variants.

AI failure never changes a successful source refresh into a failed refresh. `signal refresh --no-ai` skips steps 4 through 10 for that invocation.

Manual `signal summarize <story-id>` uses the same cache, budget, validation, and persistence path. A profile override changes only that invocation. Regeneration without `--force` can return a cache hit; `--force` creates a request but still produces an immutable variant with a distinct generation identity.

## 12. Persistence

Backward-compatible migrations add:

### `model_profiles`

Stores profile metadata, credential references, consent, limits, rates, retry settings, and timestamps. It never stores credential values.

### `app_settings`

Stores the nullable default profile ID and future nonsecret application-wide settings.

### `summary_variants`

Stores immutable structured summaries, content/cache identity, provider/model snapshots, usage, accounted cost, and generation timestamps. Multiple forced variants may share a cache key; normal lookup selects the newest valid variant deterministically.

### `generation_attempts`

Stores budget reservations and their final status, estimated cost, actual cost, usage, error category, and timestamps. Error details and provider response bodies are excluded.

### `briefing_items`

Adds a nullable selected-summary-variant reference. Existing item staleness and Smart summary behavior remain unchanged.

All migrations are transactional and version-selected. Opening an existing milestone-one database preserves stories, briefings, saved/read state, source settings, and refresh history.

## 13. CLI contract

### Profile management

```text
signal models list
signal models add
signal models use <profile>
signal models test <profile>
signal models remove <profile>
```

`models add` is interactive by default. Nonsecret flags support automation, but a secret is never accepted on the command line. System-store mode prompts for hidden input; environment mode records only a variable name. The data-sharing disclosure requires explicit confirmation before the profile can be enabled for automatic generation.

`models list` shows names, provider, model, endpoint host where applicable, credential source type, default/enabled state, consent state, and limits. It never shows credential values.

`models test` makes one explicit bounded request using synthetic nonprivate text, reports a redacted result and usage, and is subject to the profile's budget. It warns that the request may incur provider cost.

`models remove` requires confirmation in an interactive terminal. A noninteractive caller must provide an explicit confirmation flag. Historical variants are retained.

### Reading and generation

```text
signal refresh [--no-ai]
signal summarize <story-id> [--profile <name>] [--force]
signal today
signal show <story-id>
```

Human output identifies AI versus Smart, the selected provider/model for AI, and fallback counts after refresh. JSON keeps all milestone-one fields and adds versioned structured summary and generation-report fields.

## 14. Failure handling

Retries apply only to:

- connection and request timeouts;
- HTTP `429`; and
- transient HTTP `5xx` responses.

Authentication failures and other permanent `4xx` responses are not retried. Retry delays use bounded exponential backoff with jitter and honor a valid, bounded `Retry-After` value.

Errors are classified as:

- missing credential;
- credential-store unavailable;
- invalid profile;
- authentication;
- rate limited;
- timeout;
- provider unavailable;
- malformed provider response;
- budget exhausted; or
- storage failure.

Automatic generation converts every provider-side category into a counted Smart fallback. Storage and migration failures remain fatal because safe local state cannot be guaranteed. Manual `models test` and `summarize` return documented nonzero exit codes while keeping diagnostics redacted.

## 15. Security and privacy

- Authorization headers and API-key headers are marked sensitive and never debug-formatted.
- Provider URLs are validated before credentials are resolved.
- Redirects are disabled.
- Response sizes and request timeouts are bounded.
- Provider error bodies are not included in user-facing or persisted diagnostics.
- Gemini keys use `x-goog-api-key`, not URL query parameters.
- OpenAI requests set `store: false`.
- Consent text names the selected provider and explains which story fields are transmitted.
- Custom endpoints display their destination host during consent.
- Credential values are zeroized where practical after request construction and never cloned into domain objects.

## 16. Testing

### Domain and persistence

- Profile validation, uniqueness, default selection, and removal.
- Backward migration from the milestone-one schema.
- Credential references serialize without values.
- Summary variants are immutable and cache keys invalidate on every identity input.
- Briefing selection retains Smart when no AI variant exists.
- Budget reservations are atomic across concurrent store connections.
- Expired reservations reconcile deterministically with a fixed clock.

### Credentials

- In-memory credential implementation for normal tests.
- System-store set/get/delete contract behind explicit platform test gates.
- Environment lookup with missing, empty, and present values.
- Serialization, logs, errors, and JSON scans proving a sentinel secret is absent.

### Providers

Each adapter uses a local mock HTTP server to verify:

- exact endpoint, method, authentication placement, and content type;
- request mapping and model preservation;
- valid response and usage parsing;
- malformed and oversized responses;
- authentication failure without retry;
- timeout, `429`, and `5xx` retry behavior;
- redirect refusal; and
- redacted diagnostics.

No automated test contacts a paid provider.

### Orchestration and CLI

- Ranking and briefing selection complete before AI.
- Cache hits avoid provider requests.
- Per-refresh and daily caps stop further calls.
- Provider failure preserves Smart and refresh success.
- Consent is required before automatic generation.
- `refresh --no-ai` makes no provider request.
- Multiple profiles with separate credentials remain isolated.
- Profile settings and selected variants persist across CLI processes.
- Existing milestone-one JSON paths remain compatible.

CI runs formatting, strict Clippy, and the complete suite on macOS, Ubuntu, and Windows.

## 17. Acceptance criteria

Milestone two is acceptable when:

1. A user can create multiple profiles with different providers, models, and credential references.
2. System-store credentials work through macOS Keychain, Windows Credential Manager, and Linux Secret Service integrations, with environment references available for headless use.
3. Credential values do not appear in SQLite, TOML, logs, errors, JSON, command arguments, or generated shell commands.
4. OpenAI, Anthropic, Gemini, and both custom OpenAI-compatible dialects pass deterministic adapter contract tests.
5. A consented default profile automatically summarizes only deterministically selected briefing items.
6. Per-refresh caps and atomic daily budget reservations prevent requests beyond configured limits.
7. Cached variants avoid repeat requests and invalidate when any cache identity input changes.
8. Provider, credential, budget, and malformed-output failures preserve a readable Smart briefing and do not fail source refresh.
9. Manual profile testing and story regeneration use the same credential, budget, validation, cache, and persistence paths.
10. Existing milestone-one databases migrate without losing stories, briefings, source settings, refresh history, read state, or saved state.
11. Existing CLI JSON fields remain available while structured summary fields are added.
12. Formatting, strict Clippy, and the complete test suite pass on macOS, Ubuntu, and Windows without paid API calls.

## 18. Next milestone

Milestone three adds UniFFI bindings and the native SwiftUI macOS companion. It consumes the model-profile, briefing, generation-report, saved-state, source, and data-generation application interfaces established here. The Mac app does not reimplement provider calls, credential policy, budgeting, caching, or database access in Swift.
