# AI Daily Signal — Product and Architecture Design

**Date:** 2026-08-29
**Status:** Approved for implementation planning

## 1. Purpose

AI Daily Signal is a personal briefing product for people who want to stay informed about meaningful AI developments without monitoring social media or manually preparing a newsletter.

The system collects from a maintained source registry, removes duplicates, ranks items, creates a concise daily briefing, and presents the same information through:

- a cross-platform `signal` CLI on macOS, Linux, and Windows; and
- a native Liquid Glass companion on macOS.

The system does the editorial work automatically. The user reads, saves, dismisses, searches, changes sources, and chooses how summaries are produced. There is no publishing workflow.

## 2. Product principles

1. **Useful by default.** A standard source pack and deterministic summaries make a first run immediately useful.
2. **User-controlled.** Users can change sources, ranking preferences, summary modes, providers, models, API keys, refresh schedules, notification behavior, and spending limits.
3. **AI is optional.** Collection, deduplication, ranking, Raw summaries, and Smart summaries work without an LLM.
4. **Primary sources first.** Official announcements, APIs, changelogs, repositories, and research or policy sources receive priority.
5. **Calm, finite consumption.** The primary product is a five-minute briefing, not an infinite engagement feed.
6. **Local first.** Version one requires no account, hosted backend, or cloud synchronization.
7. **One engine, multiple surfaces.** The CLI and Mac companion use the same Rust implementation and the same on-disk state on macOS.

## 3. Version-one scope

### Included

- Rust core library.
- `signal` CLI for macOS, Linux, and Windows.
- Native SwiftUI menu-bar companion and reading window for macOS.
- RSS and Atom collection.
- GitHub repository, organization, topic, and search monitoring.
- Configured changelog and announcement-page monitoring.
- Standard source pack plus user additions and overrides.
- Deterministic deduplication, categorization, ranking, and GitHub momentum tracking.
- Raw, Smart, and AI summary modes.
- Multiple named model profiles with separate credentials and budgets.
- OpenAI, Anthropic, Google Gemini, and custom OpenAI-compatible provider adapters.
- SQLite persistence, saved/read state, and cached summaries.
- Scheduled refresh in the Mac companion and manual or externally scheduled refresh from the CLI.
- Restrained, optional notifications on macOS.

### Deferred

- Accounts and cloud synchronization.
- iPhone, iPad, Windows, or Linux graphical applications.
- Newsletter creation or publishing.
- Multi-user collaboration or social recommendations.
- Hosted collection infrastructure.
- A dedicated local background daemon.
- Package-manager distribution until standalone release artifacts are stable.

## 4. Platform architecture

```text
Sources and provider APIs
          |
          v
+-----------------------------+
| signal-core (Rust)          |
|                             |
| collectors                  |
| normalization/deduplication |
| ranking                     |
| summary providers           |
| briefing assembly           |
| configuration               |
| SQLite persistence          |
+-----------------------------+
       |                 |
       v                 v
signal CLI          UniFFI Swift bindings
macOS/Linux/Windows       |
                         v
                Native SwiftUI Mac app
```

`signal-core` owns domain logic and all database access. Swift does not query or mutate SQLite directly. Typed Swift bindings are generated with UniFFI and expose a deliberately narrow application-facing API.

On macOS, the app and CLI open the same SQLite database. SQLite uses WAL mode, transactional writes, a busy timeout, and a monotonically increasing data-generation value. The Mac app checks the generation value while active so CLI writes appear without restarting the app.

There is no daemon in version one. The Mac app can launch at login and refresh while running. On headless systems, users run `signal refresh` manually or schedule it with the operating system's existing scheduler.

## 5. Core components

### 5.1 Domain and pipeline

The domain layer defines sources, collected items, normalized stories, repository snapshots, scores, summary variants, briefings, model profiles, and user state. The pipeline coordinates collectors and transforms their output into a daily briefing.

### 5.2 Collectors

Each collector implements a common interface and returns normalized candidate data plus provenance. Collectors are isolated: one failure never cancels successful work from other sources.

Version-one collector families are:

- RSS and Atom feeds;
- GitHub REST API queries for repositories, organizations, topics, and searches; and
- configured changelog or announcement pages, using canonical links and page-change detection.

Collectors never install or execute discovered repositories. GitHub inspection is limited to API metadata, repository trees, manifests, and selected text files.

### 5.3 Deduplication and classification

Deduplication uses canonical URLs, redirect targets, normalized titles, token-based title similarity, shared product or company identifiers, and content fingerprints such as SimHash. Suspected duplicates become one story with multiple sources rather than multiple briefing entries.

Classification uses source categories, dictionaries, GitHub topics, URL patterns, and repository file signatures. Structural detection identifies relevant project types such as Agent Skills, Claude Code plugins, Codex skills, MCP servers, agent frameworks, evaluation tools, and inference or serving tools.

### 5.4 Ranking

Ranking is deterministic and explainable. Every story records the factors that affected its score.

Ordinary stories combine:

- source authority and independence;
- recency;
- category relevance;
- corroboration by other monitored sources;
- novelty compared with previously covered items; and
- configured user source or category weights.

GitHub candidates have separate Momentum, Relevance, and Health scores. Momentum emphasizes recent star and fork growth, acceleration, relative growth, releases, commits, and monitored-source mentions. Repositories are compared within age cohorts so established projects do not permanently dominate new ones. Relevance requires more than a matching keyword. Health rejects archived, disabled, empty, misleading, or inactive candidates.

GitHub momentum is derived from locally stored snapshots rather than historical stargazer-list access.

### 5.5 Briefing assembly

The default daily briefing targets a five-minute read:

- up to three top signals;
- one GitHub Breakout;
- up to three Release Radar items;
- one strongest research, policy, or business development; and
- an optional Watchlist item that is important but not yet conclusive.

Sections may contain fewer items on quiet days. The system does not fill quotas with weak stories.

## 6. Summary system

Every story may have multiple immutable summary variants.

### Raw

Uses the original title, source excerpt, and structured metadata. It requires no AI and performs no interpretive generation.

### Smart

Uses deterministic templates and extracted structured facts. It requires no AI. Templates are specialized for known item types such as GitHub repositories, releases, research papers, policy announcements, and pricing changes.

### AI

Uses the selected model profile to produce concise `What happened`, `Why it matters`, and optional `Caveat` fields. AI does not decide what is news; it explains candidates already selected by deterministic ranking.

AI generation occurs only after ranking. Cache identity includes the story content hash, provider, endpoint, model identifier, prompt version, and summary settings. Changing the model creates another variant instead of overwriting previous output.

If generation fails or exceeds a budget, the briefing uses the Smart variant. A provider failure cannot block collection or briefing assembly.

## 7. Model profiles and credentials

A named model profile contains:

- profile name;
- provider adapter;
- model identifier;
- optional custom API endpoint;
- secure credential reference;
- maximum summaries per refresh;
- maximum daily spend; and
- timeout and retry policy.

Users can keep multiple profiles, choose a default, and override the profile for a single regeneration. Paid-profile fallback is disabled by default to prevent unexpected cost.

Version one provides adapters for OpenAI, Anthropic, Google Gemini, and custom OpenAI-compatible endpoints. Model identifiers remain user-selectable so new models do not require an application release.

Credentials are stored through:

- macOS Keychain;
- Linux Secret Service when available;
- Windows Credential Manager; or
- explicitly configured environment variables for headless and automated environments.

Credentials are never written to SQLite, source configuration, logs, CLI output, or shell commands generated by the application.

Before enabling AI summaries, the product explains that relevant source content is transmitted to the chosen provider.

## 8. Source registry

The effective source registry is an overlay:

```text
bundled Standard Source Pack
              +
user additions and overrides
              |
              v
effective source registry
```

The Standard Source Pack includes official AI company announcements, model and API changelogs, selected independent publications, research feeds, European and Swedish policy sources, and maintained GitHub searches. It is versioned with the application.

Users can:

- enable or disable a bundled source;
- add RSS or Atom feeds;
- add changelog pages;
- monitor GitHub repositories, organizations, topics, or searches;
- assign categories and importance weights;
- test a source before saving it; and
- import or export source configuration.

Bundled definitions are never edited in place. User overrides are stored separately, so application updates cannot erase personal choices.

## 9. macOS companion

### 9.1 Interaction model

The companion uses a hybrid structure:

- a persistent menu-bar extra for status and quick scanning; and
- a full reading window for the complete briefing, discovery, configuration, and saved items.

The menu-bar popover contains only:

- collection status and last refresh time;
- the top signal;
- GitHub Breakout;
- Refresh; and
- Open Briefing.

It is not a miniature infinite feed.

The full window contains:

- **Today:** the finite five-minute briefing;
- **Latest:** newly collected items in chronological order;
- **GitHub:** breakout repositories and momentum detail;
- **Saved:** bookmarked stories;
- **Sources:** the standard pack and personal sources; and
- **Settings:** models, credentials, budgets, refresh, appearance, and notifications.

Each story exposes its source, publication time, category, summary mode and model, `What happened`, `Why it matters`, caveat or confidence information, and actions to save, mark read, open the source, switch summary variants, or regenerate with a chosen profile.

### 9.2 Visual direction

The app uses Apple's Liquid Glass design principles. Glass is a functional layer for the sidebar, menu popover, toolbars, and important interactive controls. Story content uses quieter standard material and high contrast rather than turning every story into a translucent card.

Ambient color and underlying content inform the glass. Edge highlights and depth communicate elevation. System components are preferred over custom effects. Reduced Transparency, Increase Contrast, light and dark appearances, keyboard navigation, and VoiceOver are first-class requirements.

### 9.3 Refresh and notifications

The companion can launch at login and refresh on a user-configurable schedule. The menu-bar state distinguishes current, refreshing, partially stale, offline, and failed states.

Notifications are opt-in and reserved for exceptionally important developments or completion of a requested refresh. There are no engagement reminders or unread-count pressure.

## 10. CLI contract

The CLI is a complete product surface and never requires the Mac app.

Primary reading commands:

```text
signal today
signal latest
signal show <id>
signal github
signal refresh
signal save <id>
signal saved
signal status
signal doctor
```

Configuration commands:

```text
signal sources list
signal sources add-rss <url>
signal sources add-github-org <name>
signal sources enable <id>
signal sources disable <id>
signal sources test <source>
signal models list
signal models add
signal models use <profile>
signal config
```

Human-readable terminal output is the default. `--plain` disables decoration for piping and accessibility. `--json` emits versioned machine-readable output with stable field names. Compatible terminals receive clickable source links. Failures return documented nonzero exit codes.

`signal today` reads the latest stored briefing and reports if it is stale. It does not silently perform network work. `signal today --refresh` refreshes first. This keeps command latency and network behavior predictable.

## 11. Storage and portability

SQLite stores sources, candidates, normalized stories, provenance, repository snapshots, ranking factors, briefings, summary variants, refresh history, reading state, and saved state.

Configuration and data use platform conventions:

| Platform | Configuration and data |
| --- | --- |
| macOS | Application Support and Preferences-compatible locations |
| Linux | XDG config, data, and cache directories |
| Windows | Local AppData and roaming configuration where appropriate |

The CLI can export source and nonsecret configuration. Secrets are referenced symbolically and must be configured again on another machine.

## 12. Failure handling

- Failed sources retain the last successful content and display a stale state.
- Partial refresh results are committed and usable.
- Network and rate-limit failures use bounded exponential backoff with jitter.
- Provider failures fall back to Smart summaries.
- Database migrations are transactional and run before normal reads or writes.
- Corrupt or incompatible configuration produces actionable diagnostics without deleting user data.
- CLI diagnostics redact credentials, raw authorization headers, and unnecessary personal paths.
- The Mac reading interface shows short, actionable status; detailed diagnostics remain in `signal status`, `signal doctor`, and local logs.

## 13. Testing and verification

### Rust core

- Unit tests for normalization, fingerprints, deduplication, classification, ranking, budgeting, and briefing assembly.
- Recorded RSS, Atom, GitHub, page-change, and malformed-input fixtures.
- Mock provider adapters for success, malformed output, timeouts, rate limits, and budget exhaustion.
- Cache-key and fallback tests across model and prompt changes.
- SQLite migration, WAL concurrency, busy-timeout, and transaction rollback tests.
- End-to-end refresh tests using a temporary database and deterministic fixtures.

### CLI

- CI on macOS, Linux, and Windows.
- Human-output snapshots and versioned JSON-schema tests.
- Exit-code and signal-interruption tests.
- Platform path and credential-adapter tests.
- Headless Linux and Windows terminal behavior checks.

### macOS companion

- UniFFI binding contract tests.
- Concurrent app/CLI database access tests.
- SwiftUI visual checks for light, dark, increased contrast, and reduced transparency.
- Menu-bar, launch-at-login, offline recovery, Keychain, keyboard, and VoiceOver checks.
- Signed application and bundled-core smoke tests before release.

## 14. Distribution

The first public artifacts are standalone signed or checksummed downloads:

- universal macOS CLI binary;
- Linux x86-64 and ARM64 CLI binaries;
- Windows x86-64 CLI executable;
- signed and notarized macOS application containing the Rust core and an installable `signal` command.

The Mac app exposes **Settings → Command Line Tool → Install `signal` Command** for direct-download users. The CLI can also be installed independently.

Homebrew, WinGet or Scoop, and native Linux packages are added only after the standalone installation, upgrade, and rollback process is stable.

## 15. Acceptance criteria

Version one is acceptable when:

1. A new user can install the CLI on any supported operating system, initialize the standard source pack, run a refresh, and read a useful briefing without configuring AI.
2. A user can add or disable a source and observe the change in the next refresh.
3. A user can configure multiple providers and model profiles without storing credentials in application data or logs.
4. Raw and Smart modes remain fully functional when no AI credentials exist or every provider is unavailable.
5. `signal today --json` returns stable machine-readable briefing output on macOS, Linux, and Windows.
6. The Mac companion and CLI share briefing, source, model, read, and saved state without database corruption.
7. The Mac companion provides the approved hybrid menu-bar and full-window Liquid Glass experience, including accessibility fallbacks.
8. A failed source, provider, or partial refresh does not erase the last successful briefing or prevent other sources from updating.
