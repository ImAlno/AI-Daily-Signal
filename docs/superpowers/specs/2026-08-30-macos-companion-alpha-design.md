# AI Daily Signal — macOS Companion Alpha Design

**Date:** 2026-08-30
**Status:** Approved for implementation planning

## 1. Purpose

This milestone delivers the first personal, standalone macOS companion for AI Daily Signal. It turns the existing Rust core into a native menu-bar-first SwiftUI product without requiring the `signal` CLI to be installed.

The alpha is intentionally a complete foreground reading and configuration experience on top of the capabilities that exist today. It does not show placeholder product areas whose underlying collectors have not been implemented.

## 2. Product decisions

- Target macOS 26 on Apple Silicon only.
- Ship a standalone `.app` that bundles the Rust core. The CLI is optional.
- Use a persistent menu-bar extra with a full reading window.
- Do not keep a permanent Dock icon. First launch opens the reading window; later launches use the menu bar as the home base.
- Use one calm welcome screen. **Build My First Briefing** initializes the standard source pack and performs the first refresh. AI setup remains optional.
- Make ordinary setup self-contained: feed sources, model profiles, Keychain credentials, default model, consent, and budgets can all be managed in the app.
- Use native Liquid Glass selectively for navigation and controls. Reading content remains quiet, opaque enough for legibility, and finite.

## 3. Alpha scope

### Included

- Standalone app launch without the CLI.
- First-run initialization and refresh.
- Menu-bar status and quick actions.
- Full-window Today, Latest, Saved, Sources, and Settings destinations.
- Story reading, source opening, read state, saved state, selected summary provenance, summary-variant selection, and profile-based regeneration.
- Feed-source creation, enable/disable, and removal.
- Model-profile creation, removal, default selection, consent, limits, budgets, and secure Keychain credential entry.
- Manual foreground refresh with partial/offline failure handling.
- Shared state with an independently installed CLI through the same Rust core, configuration, database, and Keychain service.
- State-revision polling while the app is active so CLI changes appear without restarting.
- Accessibility behavior for Reduced Transparency, Increase Contrast, keyboard navigation, and VoiceOver.
- Deterministic preview and test fixtures that do not use paid APIs.
- A reproducible personal `.app` bundle assembled from command-line builds.

### Deferred

- GitHub, changelog-page, and announcement-page collectors and their dedicated UI.
- Search and story dismissal.
- Scheduled refresh, launch at login, and notifications.
- A background daemon.
- Intel and older-macOS support.
- Xcode UI automation and screenshot assertions.
- Signing, notarization, public downloads, automatic updates, and an **Install `signal` Command** action.

## 4. Architecture

```text
SwiftUI scenes and views
          |
          v
@MainActor AppModel
          |
          v
Swift BridgeClient protocol
          |
          v
generated UniFFI Swift bindings
          |
          v
signal-ffi (Rust facade)
          |
          v
signal-core
  configuration / SQLite / Keychain / collection / providers
```

### 4.1 Rust bridge

Add a `signal-ffi` workspace crate. It is a narrow application-facing facade, not a second implementation of the CLI.

The bridge:

- owns a `SignalApp` instance behind synchronization appropriate to the exported object;
- converts core domain values into explicit UniFFI records and enums;
- exposes typed, redacted errors rather than core error strings;
- provides async operations for network or provider work;
- provides synchronous, inexpensive snapshot and generation queries where safe;
- never accepts or returns database handles, SQL, raw provider responses, credential references, or filesystem paths; and
- contains the one Tokio runtime boundary needed by the Swift process.

Swift must not query SQLite, reconstruct provider requests, calculate budgets, manipulate credential references, or parse Rust JSON output.

UniFFI generates the Swift API, C header, and module map. Generated artifacts are build outputs rather than hand-edited source. UniFFI maps Rust records, enums, optional values, errors, and async functions into native Swift forms, but its Swift 6 support remains partial and async cancellation is not automatically mapped. The implementation therefore keeps generated values behind a small hand-written Swift protocol and implements cancellation explicitly in the Rust facade.

### 4.2 Swift package and app bundle

Add a Swift package under `apps/macos` containing:

- `SignalMacApp`, the executable target;
- `SignalAppKit`, the hand-written state, bridge adapter, formatting, and views;
- `SignalAppKitTests`, which use a fake `BridgeClient`; and
- generated binding/module outputs under a build-only directory.

The executable embeds the compiled arm64 Rust dynamic library and generated Swift bindings. A repository script builds Rust, generates bindings, builds Swift, assembles `AI Daily Signal.app`, writes a minimal `Info.plist`, places the native library in `Contents/Frameworks`, and fixes its loader path. The bundle is ad-hoc signed only when local launching requires it; public signing is deferred.

The alpha must launch from Finder or `open` without the CLI binary and without repository-relative runtime paths.

### 4.3 Shared local state

Both surfaces resolve the existing macOS application identifier and Application Support paths. They use the same:

- source configuration;
- SQLite database and migrations;
- Keychain service and per-profile accounts; and
- cached AI summary variants.

SQLite remains the authority. WAL mode, busy timeout, immediate budget transactions, and atomic configuration writes stay in Rust. The app does not keep a second persistent product store.

Swift-only presentation preferences—welcome completion, selected navigation destination, and last window geometry—use `UserDefaults`. They are not synchronized with the CLI.

## 5. Bridge contract

The exact UniFFI spelling is set by the implementation plan, but the semantic surface is fixed here.

### 5.1 Snapshot records

`CompanionSnapshot` contains:

- initialization state;
- a `StateRevision` containing the SQLite data-generation number and a deterministic source-configuration fingerprint;
- collection status and last refresh metadata;
- today briefing, latest stories, and saved stories;
- effective feed sources;
- safe model profiles and current default-profile identifier; and
- a boolean indicating whether any usable AI profile exists.

Story and briefing records expose only stable UI data: identifiers, title, canonical URL, clean excerpt, category, publication time, source identifiers, score components needed for explanation, read/saved state, staleness, Smart summary, selected structured AI summary, provider/model provenance, and available immutable summary variants.

No record contains a credential value, credential reference, raw provider body, local path, or free-form backend diagnostic.

### 5.2 Operations

The bridge supports:

- initialize standard sources;
- load a complete snapshot;
- read the current state revision;
- start, observe, and cancel one foreground refresh;
- mark a story read or unread;
- save or unsave a story;
- select an existing summary variant;
- regenerate a story with an optional profile override and force flag;
- add, update, enable, disable, and remove a feed source;
- add a model profile using either a Keychain secret or environment-variable reference;
- select a default model profile;
- test and remove a model profile; and
- return safe validation information needed by forms.

Mutating operations return the changed safe record plus the resulting state revision so Swift can update immediately without guessing.

SQLite mutations increment `data_generation` transactionally. Source configuration is an atomically written file rather than a SQLite table, so its canonical serialized bytes are hashed separately. Comparing the composite revision detects both database and source changes made by an optional CLI without pretending that a cross-file-and-database transaction exists.

### 5.3 Errors and cancellation

Bridge errors are a finite enum with safe context:

- not initialized;
- invalid input;
- not found;
- credential unavailable;
- consent required;
- budget exhausted;
- provider unavailable;
- offline;
- refresh already running;
- cancelled; and
- storage unavailable.

User-facing messages are owned by Swift and keyed from these categories. Rust error bodies are never forwarded.

UniFFI does not supply native cancellation automatically. The bridge therefore gives each long-running operation an opaque identifier and exposes `cancel_operation(id)`. Rust checks cancellation between source requests, before provider calls, and before committing a cancelled refresh. Cancelling cannot roll back a provider request that may already have been sent; existing conservative accounting remains authoritative.

## 6. Swift state model

`AppModel` is `@MainActor` and owns:

- the current immutable snapshot;
- selected destination and story;
- welcome, loading, empty, refreshing, stale, offline, and failure presentation state;
- current sheets for source and model editing; and
- one active operation identifier.

`BridgeClient` is a protocol. Production code adapts generated UniFFI APIs; tests and previews inject deterministic actors/fakes.

Bridge work runs outside the main actor. Only completed records cross back to replace Swift state. Views do not call generated bindings directly.

While the app is active, a low-frequency task reads only the composite state revision. A change triggers one coalesced snapshot reload. Polling pauses when the app is inactive and while a local mutation is already loading a newer snapshot.

## 7. Interaction design

### 7.1 App lifecycle

The bundle is an accessory app with no permanent Dock icon. On the very first launch, it opens the welcome window. After initialization it remains available through the menu-bar item. **Open Briefing** activates the full window and brings it forward.

Closing the reading window leaves the menu-bar companion running. **Quit AI Daily Signal** in the popover terminates it.

### 7.2 Welcome

The welcome view is a quiet, centered composition with:

- product mark and one-sentence purpose;
- a short local-first and AI-optional explanation;
- **Build My First Briefing** as the only primary action; and
- a compact disclosure that refreshing contacts enabled source websites.

The action initializes the standard source pack and refreshes. If the network is unavailable, initialization remains complete and the empty state offers retry. AI is not mentioned as a prerequisite.

### 7.3 Menu-bar popover

The popover contains only:

- current status and last refresh time;
- top signal title, source, and summary provenance when available;
- one compact partial/offline/failure explanation when needed;
- **Refresh** or **Cancel Refresh**;
- **Open Briefing**; and
- a restrained menu containing Settings and Quit.

It is fixed-height around its content and never becomes a scrollable miniature feed. The status icon distinguishes current, refreshing, partially stale, offline, and failed states without relying on color alone.

### 7.4 Full reading window

Use `NavigationSplitView` with Today, Latest, Saved, Sources, and Settings.

Today is a finite briefing. The content column presents editorial sections and stable ordering. A selected story opens a spacious detail pane with:

- source, time, category, read/saved state, and staleness;
- selected summary provenance;
- **What happened**, **Why it matters**, and caveat;
- open source, save, and read actions;
- available Raw, Smart, and AI variants; and
- regeneration with a chosen profile.

Latest is chronological and finite for the loaded dataset. Saved shows bookmarked stories. Sources and Settings use native grouped forms rather than dashboard cards.

### 7.5 Sources

The source screen lists standard and personal feeds together, clearly labeling origin. Users can:

- enable or disable a feed;
- add a personal RSS or Atom URL with name, category, and weight;
- validate the nonsecret form before saving; and
- remove only personal sources after confirmation.

Bundled definitions are never edited or removed; their enabled state and allowed overrides remain user configuration.

### 7.6 Models and credentials

Settings lists safe profile metadata only. The profile form supports provider, opaque model identifier, optional compatible endpoint/dialect, credential mode, limits, token prices, daily budget, and explicit provider-data-sharing consent.

Keychain mode presents a native secure field. The value exists in Swift only for the duration of the save call, is never stored in `AppModel`, and is cleared when the call completes. Environment mode stores only a variable name.

Testing a profile is an explicit paid-operation surface with the same warning and bounded synthetic request as the CLI. Removal retains historical summary variants.

## 8. Visual system

The alpha follows Apple's current Liquid Glass guidance by adopting standard SwiftUI structure and controls first. Custom glass is used only when it communicates elevation or interaction.

- Sidebar, toolbar, menu popover, selection, and primary controls may use system glass/material treatment.
- Story prose uses a calm reading surface with strong contrast and generous spacing.
- Repeated translucent story cards are prohibited.
- Accent color is restrained and semantic status never depends on it alone.
- SF Symbols and native typography carry the visual language.
- Grouped forms use standard controls so they inherit platform updates.
- Related custom glass elements, if any, share a `GlassEffectContainer` rather than stacking independent effects.

When Reduce Transparency is enabled, glass becomes an opaque system background with visible separators. Increase Contrast strengthens boundaries. Keyboard focus remains visible, all actions have labels and shortcuts where appropriate, and every status symbol has an equivalent VoiceOver label.

## 9. Refresh and failure behavior

Only one app-initiated refresh may run at once. The popover and window share that operation state.

- A complete refresh replaces the briefing snapshot.
- A partial refresh preserves carried items and marks them stale through existing core semantics.
- Total source failure preserves the last briefing and displays offline/failed state.
- Provider failure preserves Smart summaries and reports fallback counts.
- Cancellation preserves the last committed briefing.
- Storage failure is blocking and never masquerades as an empty briefing.

The app does not generate engagement reminders or unread-count pressure.

## 10. Security and privacy

- Credentials are stored only through the existing Keychain adapter or referenced environment variables.
- Swift logging must use privacy-redacted fields and must never interpolate bridge objects containing user content.
- Generated binding `Debug`/description behavior is not used for user-facing errors.
- Provider consent is explicit before a profile is enabled.
- AI receives only the canonical approved story fields already enforced by `signal-core`.
- Redirect refusal, endpoint validation, response caps, retry accounting, budgets, and cache identity remain Rust responsibilities.
- Test sentinels scan app output, bundle contents, generated artifacts, configuration, and SQLite text for credential and provider-body leakage.

## 11. Build and verification

The current development Mac has Swift 6.3 command-line tools and the macOS 26 SDK but not full Xcode. The alpha is therefore structured so core, bindings, Swift package, bundle assembly, and launch smoke tests run from the command line.

Local verification includes:

- Rust formatting and strict Clippy;
- all Rust workspace tests;
- UniFFI binding generation reproducibility;
- bridge contract and shared app/CLI database tests;
- `swift build` and `swift test`;
- app-bundle structure and dynamic-library loader checks;
- app launch/termination smoke test;
- credential and provider-body sentinel scans; and
- the existing macOS Keychain contract.

CI retains the three-platform Rust/CLI matrix and adds a macOS-only bridge/Swift/package job. No paid provider APIs are called.

Xcode UI automation, screenshot baselines, signing, and notarization remain pending until full Xcode and release credentials are available. Their absence is reported explicitly and is not replaced with fabricated evidence.

## 12. Acceptance criteria

The alpha is acceptable when:

1. `AI Daily Signal.app` launches on the development Apple Silicon Mac without an installed CLI or repository-relative runtime dependency.
2. A first-time user can initialize standard feeds and attempt the first refresh from the welcome screen without configuring AI.
3. The menu-bar popover reports status, shows the top signal when present, refreshes or cancels, and opens the reading window.
4. Today, Latest, Saved, Sources, and Settings render real Rust-owned state with no Swift SQLite access.
5. A user can read, save, mark read, open, switch summary variants, and regenerate a story.
6. A user can add, toggle, and remove personal feed sources without editing files or using the CLI.
7. A user can create, test, select, and remove model profiles while credentials remain outside application files, SQLite, logs, and generated artifacts.
8. CLI mutations become visible while the app is active, and concurrent app/CLI access does not corrupt data.
9. Partial, offline, provider, cancellation, and storage failures preserve the last successful briefing and present distinct redacted states.
10. Light, dark, Reduced Transparency, Increase Contrast, keyboard, and VoiceOver paths have deterministic checks appropriate to the command-line toolchain.
11. Rust, UniFFI, Swift, bundle, launch, and secret-scan verification passes locally without paid API calls.

## 13. References

- [UniFFI Swift bindings](https://mozilla.github.io/uniffi-rs/next/swift/overview.html)
- [UniFFI async and cancellation behavior](https://mozilla.github.io/uniffi-rs/next/futures.html)
- [Apple: Adopting Liquid Glass](https://developer.apple.com/documentation/TechnologyOverviews/adopting-liquid-glass)
- [Apple: MenuBarExtra](https://developer.apple.com/documentation/swiftui/menubarextra)
