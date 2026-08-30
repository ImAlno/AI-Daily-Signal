# Task 12 report: accessible Liquid Glass and deterministic visual states

## Outcome

Task 12 adds a semantic visual policy, restrained macOS 26 Liquid Glass wrappers,
deterministic preview fixtures, and accessibility metadata to the existing native
SwiftUI hierarchy. The reading surface remains opaque and editorial; glass is
limited to the two related custom actions in the compact menu-bar popover.

## TDD evidence

The first focused run was:

```text
SIGNAL_SWIFT_CLT_TESTING=1 swift test --package-path apps/macos --filter AccessibilityPolicyTests
```

It failed at compile time for the intended missing production surface:
`VisualPolicy`, `SemanticPalette`, accessibility and icon-control descriptors,
keyboard command descriptors, `PreviewFixtures`, `AppPhaseKind`, and the fixture
security audit did not exist. No production implementation was edited before this
RED run.

After the minimal policy, fixture, and command implementations were added, the
macOS 26 SDK exposed one API spelling correction: SwiftUI's environment value is
`colorSchemeContrast`, not `accessibilityContrast`. The corrected implementation
then built cleanly.

SwiftPM's Command Line Tools helper compiled the filtered test bundle but emitted
no discovered or executed test cases. Per the task requirement, a direct Swift
Testing runner was linked from the built test objects and passed explicit
`Testing.__CommandLineArguments_v0` values to the framework entry point. That
runner produced real case-level evidence:

- `AccessibilityPolicyTests`: 8 tests passed.
- `PreviewFixtureTests`: 7 tests passed.
- Full `SignalAppKitTests`: 113 tests passed across 7 suites.

Plain `swift test --package-path apps/macos` remains unsupported in this CLT-only
environment because `Testing` is unavailable without the repository's explicit
`SIGNAL_SWIFT_CLT_TESTING=1` opt-in. It fails while importing `Testing` and is not
reported as passing evidence. The opt-in mode builds the test bundle; direct
Swift Testing is the truthful execution evidence.

## Implementation

### Visual policy and glass

- `VisualPolicy` defines adaptive semantic system-color tokens, an always-opaque
  reading surface, Reduce Transparency glass policy, stronger Increase Contrast
  separator width, a 28-point macOS control floor, visible native focus, and stable
  VoiceOver sort priorities.
- `SignalGlass` uses macOS 26 `.glassEffect(.regular.interactive())` for custom
  interactive elements and an opaque `NSColor.windowBackgroundColor` fallback
  with a visible separator when Reduce Transparency is active.
- `SignalGlassControlGroup` uses one `GlassEffectContainer` only for the related
  Refresh and Open Briefing controls.
- `SignalReadingSurface` keeps story prose on `NSColor.textBackgroundColor` and
  strengthens its meaningful leading edge under Increase Contrast.

### Product hierarchy

- Native SwiftUI typography, SF Symbols, system colors, standard controls, and
  grouped forms remain authoritative.
- Story prose stays quiet and opaque. There are no gradients, glow borders,
  generic AI imagery, translucent prose, repeated glass rows, or card grids.
- The existing compact provenance capsule remains the single restrained accent
  and provenance signal.
- Glass is contextual to the compact popover controls instead of being repeated
  through the feed or settings rows.

### Accessibility and keyboard behavior

- Story and welcome titles are headings; reading-section and list-section
  headings retain explicit heading traits.
- Story identity, status/provenance, content, and actions consume stable tested
  accessibility sort priorities.
- Statuses retain distinct SF Symbols and plain-language labels including
  `Refreshing` and `Partially stale`, so color is never the only signal.
- The two icon-only controls have explicit labels, help or hints, and minimum
  target dimensions. Hidden source-toggle labels remain semantic Toggle labels.
- The real toolbar consumes tested descriptors for Command-R, Command-O,
  Command-S, and Command-comma, including visible help strings.
- Native controls and focus behavior are retained rather than replacing them with
  custom gesture surfaces.

### Deterministic fixtures and secret absence

Stable fixtures cover welcome, empty, populated, selected AI, Smart fallback,
stale partial refresh, offline cached briefing, provider failure, dark appearance,
Reduced Transparency, and Increase Contrast. Supporting fixtures cover loading,
building the first briefing, startup failure, and refreshing so every `AppPhase`
is represented.

All fixture URLs and timestamps are fixed and valid. Visual environment variants
reuse the exact populated snapshot so only the named environment changes. The
fixture audit recursively rejects secret-bearing property names and recognizable
API-key, bearer-authorization, and private-key markers. Its test also injects two
sentinels and proves that the detector catches both, making the absence assertion
meaningful rather than vacuous.

## Verification

- Focused direct Swift Testing: 15/15 Task 12 tests passed.
- Full direct Swift Testing: 113/113 tests passed across 7 suites.
- `swift build --package-path apps/macos`: passed.
- `scripts/test-swift-package-modes.sh`: passed; normal and CLT test modes remain
  isolated.
- `swift format lint --recursive apps/macos/Sources apps/macos/Tests`: passed with
  no diagnostics after formatting the Task 12 files.
- `git diff --check`: passed.
- `cargo fmt --all --check`: passed.
- `cargo test --workspace`: 236 passed, 0 failed, 1 ignored. The ignored test is
  the existing unlocked ephemeral OS credential-store contract.

## Constraints and concerns

- Full Xcode is not installed. Xcode UI automation and screenshot baselines remain
  unavailable and were not fabricated.
- The direct Swift Testing runner is a temporary verification harness outside the
  repository; no test-only runner or CLT path is shipped in the application.
- No paid provider API was called.
- No Rust, bridge, generated binding, package manifest, SDD ledger, or unrelated
  file was changed.

## Review fix round 1

Review found three semantic gaps. Regression tests were added before production
changes and the first focused build failed for the intended missing interfaces:
`AppModel.saveSelectedStory()` and `StorySaveTogglePresentation` did not exist.
The fixture regressions also encoded the incorrect fully stale and duplicate
selection states before implementation changed them.

- The stale partial-refresh fixture now keeps the briefing itself fresh, marks
  only the carried first item stale, retains a fresh second item, and explicitly
  presents `.stale` as `Partially stale`. Tests assert both stored item flags and
  the resulting `TodayPresentation` row flags are `[true, false]`.
- Command-S now calls an idempotent `saveSelectedStory()` path. It does nothing
  when the selected story is already saved, while a new test proves no bridge
  unsave request is emitted. The detail-pane control remains a true toggle and
  uses dynamic `Save this story` or `Remove this story from Saved` help.
- The populated fixture now contains stories without selecting one. The selected
  AI fixture reuses the same snapshot but selects the AI-summary story. Tests
  assert these semantic differences in addition to their stable identifiers.

Focused direct Swift Testing passed 29 tests across
`AccessibilityPolicyTests`, `PreviewFixtureTests`, and `AppPresentationTests`.
The post-fix full direct Swift Testing run passed 116 tests across 7 suites. The
Swift package build, package-mode isolation, Swift and Rust formatting, diff
check, and Rust workspace regression (236 passed, 1 existing Keychain test
ignored) also passed.
Plain CLT SwiftPM discovery remains unchanged and the direct runner remains the
truthful case-level evidence.
