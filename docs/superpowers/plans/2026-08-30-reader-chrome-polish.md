# Reader and Chrome Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Polish the responsive macOS companion into a clearer editorial reader with quiet native chrome, visible story affordances, top-positioned summary provenance, and calmer Sources/Models/Preferences hierarchy.

**Architecture:** Preserve `AppModel`, `NavigationSplitView`, the existing responsive breakpoints, and every Rust/FFI contract. Implement the polish through presentation values consumed by real SwiftUI views, one shared story header, native toolbar/list/form controls, and semantic AppKit colors; keep view-local hover/motion state out of `AppModel`.

**Tech Stack:** Swift 6.2, SwiftUI/AppKit on macOS, Swift Testing, Rust workspace regression tests, shell-based standalone app packaging.

**Spec:** `docs/superpowers/specs/2026-08-30-reader-chrome-polish-design.md`

## Global Constraints

- The supported window minimum remains exactly 420 by 520 points.
- Responsive thresholds remain expanded at 820 points and wider, rail from 560 through 819 points, and compact below 560 points.
- The reading column maximum changes from 720 to exactly 680 points; expanded/rail/compact horizontal padding becomes 28/24/18 points.
- Use adaptive macOS semantic colors and system San Francisco fonts; do not hardcode light-only palette values.
- Material is limited to navigation and native title-bar/status chrome; reading and settings content remain opaque.
- Do not add gradients, glow, glass content cards, persistent story cards, custom fonts, chat UI, dashboards, scheduling, or fake preference controls.
- Refresh is the sole direct toolbar action in every layout mode. Open Source, Save, and Preferences remain in one native overflow menu with Command-O, Command-S, and Command-comma; Command-R remains Refresh.
- `AppModel` remains the source of truth for destination, selected story, summary selection, operations, and mutations. Hover and Reduce Motion presentation stay view-local.
- Preserve bridge-confirmed source, model, and story mutations, credential redaction, the standalone macOS app contract, menu-bar behavior, and independent macOS/Linux/Windows CLI behavior.
- No Rust, UniFFI, database, generated binding, CLI, menu-bar view, or packaging-script changes are permitted unless a compile failure proves a source-compatible adapter is required.
- Do not add marker-only styling booleans or tests. Presentation values must be consumed by production views; test observable action order, copy, state, accessibility, layout dimensions, or hosted view structure.
- Do not change global macOS appearance or accessibility settings. Reject incomplete/placeholder screenshots as visual evidence.

---

### Task 1: Quiet shell, toolbar, and navigation selection

**Files:**
- Modify: `apps/macos/Sources/SignalAppKit/Views/ReadingWindowView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/AppNavigationView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Design/AppLayoutPolicy.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AppLayoutPolicyTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AppPresentationTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AdaptiveShellRegressionTests.swift`

**Interfaces:**
- Consumes: `AppLayoutPolicy.mode(for:)`, `ReadingCommand`, `Destination`, and the existing immutable compact column-visibility binding.
- Produces: `ReadingToolbarPresentation.directCommands`, `ReadingToolbarPresentation.overflowCommands(storyCommandsAvailable:)`, a product-level `windowTitle`, 680-point `ReadingColumnMetrics`, and visible rail selection consumed by every destination.

- [ ] **Step 1: Write failing policy and shell tests**

Add expectations equivalent to:

```swift
@Test
func readingMetricsMatchTheApprovedEditorialColumn() {
  #expect(ReadingColumnMetrics.maximumWidth == 680)
  #expect(ReadingColumnMetrics.horizontalPadding(for: 1_100) == 28)
  #expect(ReadingColumnMetrics.horizontalPadding(for: 760) == 24)
  #expect(ReadingColumnMetrics.horizontalPadding(for: 480) == 18)
  #expect(ReadingColumnMetrics.minimumWindowWidth == 420)
  #expect(ReadingColumnMetrics.minimumWindowHeight == 520)
}

@Test
func toolbarKeepsOnlyRefreshDirectAndPlacesContextInOverflow() {
  let presentation = ReadingToolbarPresentation(phase: .ready, refreshInProgress: false)
  #expect(presentation.windowTitle == "AI Daily Signal")
  #expect(presentation.directCommands == [.refresh])
  #expect(
    presentation.overflowCommands(storyCommandsAvailable: true)
      == [.openSource, .save, .settings]
  )
  #expect(
    presentation.overflowCommands(storyCommandsAvailable: false)
      == [.settings]
  )
}
```

Extend the hosted compact-shell regression to assert the default sidebar toggle remains absent, and add a rail-host assertion that the selected destination exposes `.isSelected` while its button has a non-clear selection background or native selected cell state.

- [ ] **Step 2: Run the focused tests and confirm RED**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: FAIL because the metrics remain 720/36/28/20, the toolbar presentation does not expose the command plan/window title, and the rail has no visible selection surface.

- [ ] **Step 3: Implement the shell and toolbar policy**

Change the real metrics:

```swift
public enum ReadingColumnMetrics {
  public static let maximumWidth: CGFloat = 680
  public static let minimumWindowWidth: CGFloat = 420
  public static let minimumWindowHeight: CGFloat = 520

  public static func horizontalPadding(for availableWidth: CGFloat) -> CGFloat {
    if availableWidth >= 820 { return 28 }
    if availableWidth >= 560 { return 24 }
    return 18
  }
}
```

Extend the existing toolbar presentation and consume it from `ReadingWindowView`:

```swift
public struct ReadingToolbarPresentation: Sendable, Equatable {
  public let refreshControl: RefreshControlPresentation
  public let windowTitle = "AI Daily Signal"
  public let directCommands: [ReadingCommand] = [.refresh]

  public func overflowCommands(storyCommandsAvailable: Bool) -> [ReadingCommand] {
    storyCommandsAvailable ? [.openSource, .save, .settings] : [.settings]
  }
}
```

Set the detail navigation title to `presentation.windowTitle` rather than `model.destination.title`. Remove the expanded-mode direct Open Source/Save/Preferences group and render one standard overflow `Menu` in every mode. Preserve all current disabled conditions, labels, help, and keyboard shortcuts. Refresh remains the only direct primary action.

- [ ] **Step 4: Implement visible rail selection without permanent button containers**

Keep the existing `.isSelected` trait. For the rail only, make each target exactly 36 by 36 points and place a seven-point rounded native selection surface behind the active destination:

```swift
.frame(width: 36, height: 36)
.background {
  if presentation.isSelected {
    RoundedRectangle(cornerRadius: 7, style: .continuous)
      .fill(Color(nsColor: .unemphasizedSelectedContentBackgroundColor))
  }
}
```

Inactive items stay container-free. Preserve label, help, focus, and compact picker accessibility value. Let `NavigationSplitView`/the native sidebar provide chrome material; do not add material to the opaque detail surface.

- [ ] **Step 5: Run focused/full Swift verification**

Run:

```bash
scripts/test-swift-testing.sh
scripts/test-swift-package-modes.sh
git diff --check
```

Expected: all commands pass; the settled compact shell has no default sidebar toggle, toolbar commands remain reachable, and all breakpoint/minimum tests stay green.

- [ ] **Step 6: Commit**

```bash
git add apps/macos/Sources/SignalAppKit/Views/ReadingWindowView.swift apps/macos/Sources/SignalAppKit/Views/AppNavigationView.swift apps/macos/Sources/SignalAppKit/Design/AppLayoutPolicy.swift apps/macos/Tests/SignalAppKitTests/AppLayoutPolicyTests.swift apps/macos/Tests/SignalAppKitTests/AppPresentationTests.swift apps/macos/Tests/SignalAppKitTests/AdaptiveShellRegressionTests.swift
git commit -m "feat: polish macos shell and navigation"
```

### Task 2: Continuous story header, signal line, and editorial body

**Files:**
- Create: `apps/macos/Sources/SignalAppKit/Views/StoryHeaderView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/SignalDisclosureView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/StoryRowView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/ExpandedStoryView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/StoryDetailView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/SummaryVariantPicker.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/BriefingHeaderView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Design/VisualPolicy.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/ReadingFlowTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift`

**Interfaces:**
- Consumes: Task 1's 680-point reading metrics, `StoryRowPresentation`, `StoryDetailPresentation`, `SummaryVariantPickerPresentation`, `selectedStoryID`, and existing story operations.
- Produces: `StoryHeaderPresentation`, `StoryHeaderView`, `StoryDetailPresentation.bodyElements`, and `ReaderMotionPresentation` used by the complete collapsed/expanded reader.

- [ ] **Step 1: Write failing presentation tests for continuity, accessibility, and motion**

Add tests equivalent to:

```swift
@Test
func storyHeaderKeepsIdentityAndDisclosesItsState() {
  let row = StoryRowPresentation(
    story: story(id: "story", title: "A signal"),
    primarySource: "Signal Research",
    relativeTime: "now",
    isStale: false,
    rank: 3,
    summarySelection: .smart
  )
  let collapsed = StoryHeaderPresentation(
    row: row,
    isExpanded: false,
    isHovered: false,
    dynamicTypeSize: .large
  )
  let expanded = StoryHeaderPresentation(
    row: row,
    isExpanded: true,
    isHovered: false,
    dynamicTypeSize: .large
  )

  #expect(collapsed.title == expanded.title)
  #expect(collapsed.chevronSystemImage == "chevron.right")
  #expect(expanded.chevronSystemImage == "chevron.down")
  #expect(collapsed.accessibilityValue == "Collapsed")
  #expect(expanded.accessibilityValue == "Expanded")
  #expect(!collapsed.emphasizesSignalLine)
  #expect(expanded.emphasizesSignalLine)
  #expect(collapsed.titleLineLimit == 3)
}

@Test
func accessibilityTextAndReduceMotionRemainUnrestricted() {
  let row = StoryRowPresentation(
    story: story(id: "story", title: "A signal"),
    primarySource: "Signal Research",
    relativeTime: "now",
    isStale: false,
    rank: 3,
    summarySelection: .smart
  )
  let accessible = StoryHeaderPresentation(
    row: row,
    isExpanded: false,
    isHovered: false,
    dynamicTypeSize: .accessibility1
  )
  #expect(accessible.titleLineLimit == nil)
  #expect(ReaderMotionPresentation(reduceMotion: false).duration == 0.17)
  #expect(ReaderMotionPresentation(reduceMotion: true).duration == nil)
}

@Test
func expandedBodyExcludesIdentityAndKeepsSemanticOrder() {
  let detail = StoryDetailPresentation(
    story: story(id: "story", title: "A signal"),
    sourceNames: ["Signal Research"],
    isStale: false,
    selection: .ai(variantID: "variant-old")
  )
  #expect(
    detail.bodyElements
      == [.whatHappened, .whyItMatters, .caveat, .scoreAndSources, .actions]
  )
  #expect(!detail.bodyElements.contains(.metadata))
  #expect(!detail.bodyElements.contains(.title))
  #expect(!detail.bodyElements.contains(.provenance))
}
```

Extend the summary-picker test to require visible label/value copy derived from the selected option:

```swift
#expect(picker.accessibilityLabel == "Summary version")
#expect(picker.selectedValue == "Smart · local algorithmic summary")
```

- [ ] **Step 2: Run the Swift suite and confirm RED**

Run `scripts/test-swift-testing.sh`.

Expected: FAIL because `StoryHeaderPresentation`, `ReaderMotionPresentation`, `bodyElements`, and the picker label/value do not exist.

- [ ] **Step 3: Add real motion and story-header presentations**

In `VisualPolicy.swift`, add a value consumed by the view:

```swift
public struct ReaderMotionPresentation: Sendable, Equatable {
  public let duration: Double?

  public init(reduceMotion: Bool) {
    duration = reduceMotion ? nil : 0.17
  }
}
```

Create `StoryHeaderView.swift` with a `StoryHeaderPresentation` that carries the actual row copy, expansion value, chevron symbol, signal-line emphasis, hover/selection-surface state, and Dynamic Type title limit. `StoryHeaderView` must render metadata, title, provenance, saved/stale state, optional rank rail, and trailing chevron in both collapsed and expanded states. The whole header is a plain full-width button with keyboard focus and `Expanded`/`Collapsed` accessibility value.

Use system font sizes from the spec: 15-point collapsed story title, 21-point expanded story title, 12-point metadata, and 11-point monospaced rank. Build the rank connector only when `row.rank != nil`; Latest and Saved therefore remain unranked.

- [ ] **Step 4: Make `SignalDisclosureView` own one continuous interaction surface**

Replace the mutually exclusive `StoryRowView`/`ExpandedStoryView` branch with this order:

```swift
VStack(alignment: .leading, spacing: 0) {
  StoryHeaderView(presentation: header) {
    model.selectedStoryID = disclosure.isExpanded ? nil : presentation.storyID
  }

  if disclosure.isExpanded, let story = model.story(id: presentation.storyID) {
    SummaryVariantPicker(story: story, model: model)
      .padding(.horizontal, 12)
      .padding(.top, 8)
    ExpandedStoryView(story: story, model: model)
      .padding(.horizontal, 12)
      .padding(.top, 18)
      .padding(.bottom, 22)
  }
  Divider()
}
```

Track hover with local `@State`, read `accessibilityReduceMotion` and `dynamicTypeSize` from the environment, and animate only the expansion/hover surface using the 0.17-second policy. If duration is `nil`, apply no animation. Preserve the existing context menu and all bridge-backed actions.

Map the policy to SwiftUI only at the view boundary:

```swift
let transition = ReaderMotionPresentation(reduceMotion: reduceMotion).duration.map {
  Animation.easeOut(duration: $0)
}
```

Keep `StoryRowView` as a compatibility wrapper around `StoryHeaderView` only if unchanged call sites require it; otherwise remove it from production and update package sources. Do not leave two independent row renderers.

- [ ] **Step 5: Move summary selection above the body and refine the editorial hierarchy**

Add `StoryDetailPresentation.bodyElements` by filtering identity elements from `elements`; retain `elements` for compatibility and existing semantic tests. Make `ExpandedStoryView` iterate `bodyElements`, remove its bottom `SummaryVariantPicker`, and keep score/actions once.

Update `SummaryVariantPickerPresentation`:

```swift
public let accessibilityLabel = "Summary version"
public let selectedValue: String
```

Derive `selectedValue` from the selected option's provenance/display label and show it in the menu label, not the generic word `Summary`. Preserve Raw/Smart/AI ordering and immutable cached-variant selection behavior.

Use 13-point semibold sentence-case section labels, 15-point body, five-point line spacing, and 20–24 points between sections. Keep text selection, source disclosure, save/read/open/regeneration behavior, and story-action error placement.

Reduce `BriefingHeaderView`'s bottom padding from 22 to 16 points and express its page title as a 30-point semibold system font. Do not add reading-time copy.

- [ ] **Step 6: Verify the complete reader task**

Run:

```bash
scripts/test-swift-testing.sh
scripts/test-swift-package-modes.sh
xcrun swift-format lint apps/macos/Sources/SignalAppKit/Views/StoryHeaderView.swift apps/macos/Sources/SignalAppKit/Views/SignalDisclosureView.swift apps/macos/Sources/SignalAppKit/Views/ExpandedStoryView.swift apps/macos/Sources/SignalAppKit/Views/SummaryVariantPicker.swift
git diff --check
```

Expected: all tests pass; mutating `selectedStoryID` still expands only one story; summary mutation tests and context-menu actions remain green.

- [ ] **Step 7: Commit**

```bash
git add apps/macos/Sources/SignalAppKit/Views/StoryHeaderView.swift apps/macos/Sources/SignalAppKit/Views/SignalDisclosureView.swift apps/macos/Sources/SignalAppKit/Views/StoryRowView.swift apps/macos/Sources/SignalAppKit/Views/ExpandedStoryView.swift apps/macos/Sources/SignalAppKit/Views/StoryDetailView.swift apps/macos/Sources/SignalAppKit/Views/SummaryVariantPicker.swift apps/macos/Sources/SignalAppKit/Views/BriefingHeaderView.swift apps/macos/Sources/SignalAppKit/Design/VisualPolicy.swift apps/macos/Tests/SignalAppKitTests/ReadingFlowTests.swift apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift
git commit -m "feat: polish continuous signal reading"
```

### Task 3: Source hierarchy and inline editor guidance

**Files:**
- Create: `apps/macos/Sources/SignalAppKit/Views/SettingsPageHeaderView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/SourcesView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/SourceEditorView.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/SourceSettingsTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift`

**Interfaces:**
- Consumes: Task 1 reading metrics, current `SourceRowPresentation`, `SourceEditorPresentation`, and bridge-confirmed source operations.
- Produces: reusable `SettingsPageHeaderView`, `SourceRowPresentation.secondaryText`, `tertiaryText`, and a compact source action hierarchy.

- [ ] **Step 1: Write failing source presentation tests**

Add expectations equivalent to:

```swift
@Test
func sourceRowsPrioritizeIdentityStatusThenTertiaryMetadata() {
  let source = Source(
    id: "personal-1",
    name: "Personal feed",
    category: "Research",
    enabled: true,
    weight: 0.8,
    feedURL: "https://example.com/feed.xml",
    origin: .personal
  )
  let value = SourceRowPresentation(source: source)
  #expect(value.secondaryText == "Research · example.com")
  #expect(value.tertiaryText == "Weight 0.8 · Personal source")
  #expect(value.directActions == [.toggleEnabled])
  #expect(value.overflowActions == [.remove])
}

@Test
func sourceEditorExplainsTheEffectOfAddingAFeed() {
  #expect(SourceEditorCopy.title == "Add Personal Source")
  #expect(SourceEditorCopy.guidance.contains("future briefings"))
}
```

Keep existing Dynamic Type tests and add a hosted 420-by-520 editor check confirming exactly one `NSScrollView` after layout settles.

- [ ] **Step 2: Run `scripts/test-swift-testing.sh` and confirm RED**

Expected: FAIL because the new copy and row action/text hierarchy do not exist.

- [ ] **Step 3: Implement the shared settings header and source list hierarchy**

Create a small reusable view:

```swift
struct SettingsPageHeaderView: View {
  let title: String
  let message: String

  var body: some View {
    VStack(alignment: .leading, spacing: 5) {
      Text(title).font(.system(size: 24, weight: .semibold))
      Text(message).font(.callout).foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}
```

Place `Sources` plus `Choose which feeds contribute to future briefings.` above the native list, on the opaque detail surface. Keep Standard and Personal sections.

Extend `SourceRowPresentation` with formatted secondary/tertiary text and real action enums consumed by `SourcesView`. Keep the enable switch directly visible. Replace the personal-source trash button with a standard trailing `Menu` containing `Remove Source`; retain its destructive confirmation, busy disabling, help, accessibility label, and bridge-confirmed behavior. Do not place standard sources in containers.

Define the action type used by both presentation and view:

```swift
public enum SourceSettingsAction: Sendable, Equatable {
  case toggleEnabled
  case remove
}
```

- [ ] **Step 4: Add editor guidance without adding a second scroll owner**

Introduce:

```swift
public enum SourceEditorCopy {
  public static let title = "Add Personal Source"
  public static let guidance = "Add an RSS or Atom feed to include it in future briefings."
}
```

Use `SettingsPageHeaderView` above the existing `Form`, keep the Form as the only scrolling owner, and keep Cancel/Add actions compact and keyboard reachable. Preserve validation, focus, locale-aware weight parsing, secret-free diagnostics, and pending-operation behavior.

- [ ] **Step 5: Verify and commit**

Run:

```bash
scripts/test-swift-testing.sh
scripts/test-swift-package-modes.sh
git diff --check
```

Then commit:

```bash
git add apps/macos/Sources/SignalAppKit/Views/SettingsPageHeaderView.swift apps/macos/Sources/SignalAppKit/Views/SourcesView.swift apps/macos/Sources/SignalAppKit/Views/SourceEditorView.swift apps/macos/Tests/SignalAppKitTests/SourceSettingsTests.swift apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift
git commit -m "feat: clarify source management hierarchy"
```

### Task 4: Model hierarchy, advanced disclosure, and calmer actions

**Files:**
- Modify: `apps/macos/Sources/SignalAppKit/Views/ModelsSettingsView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/ModelProfileEditorView.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/ModelSettingsTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift`

**Interfaces:**
- Consumes: Task 3's `SettingsPageHeaderView`, `ModelsSettingsAction`, current model confirmation flows, and bridge-confirmed model operations.
- Produces: an expanded `ModelProfileRowPresentation` with `secondaryText`, `readinessText`, `advancedMetadata`, `directActions`, and `overflowActions` consumed by the list row.

- [ ] **Step 1: Write failing model hierarchy tests**

Add expectations equivalent to:

```swift
@Test
func modelRowsKeepIdentityAndReadinessVisibleWhileDeferringAdvancedMetadata() {
  let value = ModelProfileRowPresentation(profile: .fixture, isDefault: false)
  #expect(value.secondaryText == "OpenAI · gpt-signal")
  #expect(value.readinessText.contains("Keychain"))
  #expect(value.advancedMetadata.contains("summaries"))
  #expect(value.advancedMetadata.contains("tokens"))
  #expect(value.directActions == [.setDefault])
  #expect(value.overflowActions == [.test, .remove])
}

@Test
func defaultModelHasNoRedundantDirectAction() {
  let value = ModelProfileRowPresentation(profile: .fixture, isDefault: true)
  #expect(value.directActions.isEmpty)
  #expect(value.overflowActions == [.test, .remove])
}
```

Add a 420-by-520 hosted editor check confirming the `Form` remains the only vertical scroll owner.

- [ ] **Step 2: Run the Swift suite and confirm RED**

Run `scripts/test-swift-testing.sh`.

Expected: FAIL because the presentation does not expose the approved hierarchy/action arrays.

- [ ] **Step 3: Implement the model page and row hierarchy**

Add `Models` plus `Choose which provider creates optional AI summaries.` using `SettingsPageHeaderView` above the native list.

Extend `ModelProfileRowPresentation` with actual formatted values used by the row. Show profile name/default status, provider/model, and readiness by default. Show `Use as Default` directly only when `canSetDefault` is true. Put Test and Remove in one native trailing overflow menu; preserve paid-network test confirmation, removal confirmation, Keychain cleanup warning, disabled/busy states, and accessibility labels.

Render endpoint and request budget inside a native `DisclosureGroup("Connection and limits")`, collapsed initially. Its body uses secondary callout text and contains no action controls or card background.

- [ ] **Step 4: Align the inline model editor with the source editor**

Introduce exact copy:

```swift
public enum ModelEditorCopy {
  public static let title = "Add Model Profile"
  public static let guidance =
    "Configure an optional provider for future AI summaries. Raw and Smart remain local."
}
```

Use `SettingsPageHeaderView`, retain the existing Form as the only scroll owner, and keep Cancel/Add actions, credential clearing, provider-specific fields, budgets, consent, and validation behavior unchanged.

- [ ] **Step 5: Verify and commit**

Run:

```bash
scripts/test-swift-testing.sh
scripts/test-swift-package-modes.sh
git diff --check
```

Then commit:

```bash
git add apps/macos/Sources/SignalAppKit/Views/ModelsSettingsView.swift apps/macos/Sources/SignalAppKit/Views/ModelProfileEditorView.swift apps/macos/Tests/SignalAppKitTests/ModelSettingsTests.swift apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift
git commit -m "feat: clarify model profile hierarchy"
```

### Task 5: Honest Preferences and shared welcome typography

**Files:**
- Modify: `apps/macos/Sources/SignalAppKit/Views/SettingsView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/WelcomeView.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AppPresentationTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift`

**Interfaces:**
- Consumes: Task 3's `SettingsPageHeaderView`, Task 1's reading metrics, and existing `PreferencesPresentation`/`WelcomeContent` behavior.
- Produces: page-level Preferences title/help copy and shared 30-point product-title typography without adding settings or onboarding steps.

- [ ] **Step 1: Write failing copy/hierarchy tests**

Add expectations equivalent to:

```swift
@Test
func preferencesExplainThatDisplayedValuesAreCurrentStatus() {
  let value = PreferencesPresentation(hasUsableAIProfile: false)
  #expect(value.title == "Preferences")
  #expect(value.guidance == "Review how the companion stores data and works with the CLI.")
  #expect(value.storage == "On this Mac")
  #expect(value.aiSummaries == "Optional")
}
```

Retain the exact welcome action and disclosures in their current tests. Add a source scan/host assertion that Preferences contains native `LabeledContent` rows and no newly enabled `Toggle`, `Picker`, or decorative container.

- [ ] **Step 2: Run `scripts/test-swift-testing.sh` and confirm RED**

Expected: FAIL because Preferences lacks title/guidance presentation values.

- [ ] **Step 3: Implement the restrained status pages**

Put `SettingsPageHeaderView(title: "Preferences", message: presentation.guidance)` above the existing grouped Form. Keep Briefing and Companion sections, existing labels/values, and opaque content surface. Do not add interactive controls.

Align the Welcome product title with the same 30-point semibold system display treatment. Preserve the single `Build My First Briefing` button, local-first explanation, network disclosure, progress state, 520-point focused welcome width, and no extra setup steps.

- [ ] **Step 4: Verify and commit**

Run:

```bash
scripts/test-swift-testing.sh
scripts/test-swift-package-modes.sh
git diff --check
```

Then commit:

```bash
git add apps/macos/Sources/SignalAppKit/Views/SettingsView.swift apps/macos/Sources/SignalAppKit/Views/WelcomeView.swift apps/macos/Tests/SignalAppKitTests/AppPresentationTests.swift apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift
git commit -m "refactor: align preferences and welcome typography"
```

### Task 6: Accessibility, visual-size, and standalone release verification

**Files:**
- Test: `apps/macos/Tests/SignalAppKitTests/AdaptiveShellRegressionTests.swift`
- Test: `apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift`
- Test: `apps/macos/Tests/SignalAppKitTests/AppLayoutPolicyTests.swift`
- Test: `apps/macos/Tests/SignalAppKitTests/AppPresentationTests.swift`
- Test: `apps/macos/Tests/SignalAppKitTests/ReadingFlowTests.swift`
- Test: `apps/macos/Tests/SignalAppKitTests/SourceSettingsTests.swift`
- Test: `apps/macos/Tests/SignalAppKitTests/ModelSettingsTests.swift`
- Modify: `docs/superpowers/specs/2026-08-30-reader-chrome-polish-design.md`

**Interfaces:**
- Consumes: the complete Tasks 1–5 UI, existing deterministic `PreviewFixtures`, the in-memory `FakeBridgeClient`, and all package scripts.
- Produces: verified expanded/rail/compact behavior, accessibility evidence, exact standalone app evidence, and an implementation-verification record in the approved spec.

- [ ] **Step 1: Audit acceptance criteria before adding tests**

Map each spec criterion to the Task 1–5 tests or a real hosted-view assertion. The plan's required coverage is listed below; if one of those named tests was omitted by its owning task, return to that task and add the specified behavior test before running the matrix. Do not add `usesCard`, `usesGlass`, `showsHover`, constant-only acceptance duplicates, or source-text mirror booleans.

At minimum, confirm the suite covers:

```text
420×520 shown-window minimum
820/560 responsive transitions
680-point reading maximum and 28/24/18 padding
Refresh-only direct toolbar plus overflow commands/shortcuts
rail visible selection plus isSelected semantics
compact current-destination value and absent sidebar toggle
one selected story and continuous header expansion
summary version before body with selected provenance
accessibility title wrapping and Reduce Motion policy
single-scroll-owner source/model editors
source/model action and confirmation behavior
Preferences status-only content
```

- [ ] **Step 2: Run the complete automated verification matrix**

Run each command independently and record exit status/output:

```bash
git diff --check
scripts/test-swift-testing.sh
scripts/test-swift-package-modes.sh
cargo test --workspace --all-features
scripts/build-macos-app.sh
scripts/verify-macos-app.sh
scripts/smoke-test-macos-app.sh
scripts/test-macos-packaging-hardening.sh
scripts/test-macos-smoke-ownership.sh
scripts/test-macos-verifier-adversarial.sh
```

Expected: Swift tests report nonzero execution including acceptance tests; Rust passes with the existing OS credential-store test ignored by design; the app assembles, verifies, and launches with an isolated HOME/PATH and no installed CLI dependency; all adversarial scripts pass.

- [ ] **Step 3: Run structural checks**

The following must produce no matches in the polished main-window sources/tests:

```bash
rg -n "HSplitView|LinearGradient|sparkles|\.sheet|frame\(minWidth: (860|500|460|440|320)|uses(Card|Glass|Gradient|Hover)" apps/macos/Sources apps/macos/Tests
```

Inspect the final diff and confirm there is exactly one summary picker in the expanded-story flow, no direct wide toolbar story/settings group, no outer ScrollView around source/model editor Forms, and no Rust/CLI/FFI/package contract diff.

- [ ] **Step 4: Perform safe native visual inspection**

Use deterministic fixture data and inspect:

```text
1100×720 expanded, light and dark
760×640 rail, light and dark
480×620 compact, light and dark
420×520 minimum on Today, Sources editor, Models editor, and Preferences
accessibility Dynamic Type, keyboard focus, Reduce Motion
Reduce Transparency and Increase Contrast only through safe environment/native inspection
```

Check title-bar duplication, toolbar crowding, rail selection, hover/expanded continuity, rank line, summary-selector placement, article measure, validation wrapping, editor scrolling, and opaque content surfaces. Never change global settings. If ScreenCaptureKit/TCC or off-screen SwiftUI still blocks complete frames, reject the output, document the exact limitation, and require human/native inspection rather than claiming success.

- [ ] **Step 5: Update verification evidence and commit**

Append only evidence actually observed to `docs/superpowers/specs/2026-08-30-reader-chrome-polish-design.md`. Include commands, test counts, package evidence, structural result, inspected sizes, and any native visual limitation.

Run final `git status --short` and `git diff --check`, then commit any real test/spec changes:

```bash
git add docs/superpowers/specs/2026-08-30-reader-chrome-polish-design.md
git commit -m "test: verify reader and chrome polish"
```

Do not commit screenshots, local capture harnesses, generated bindings, app bundles, or process reports.

## Final Whole-Branch Gate

After Task 6, run an independent whole-branch review against the merge base using `superpowers:requesting-code-review`. Any Critical or Important findings receive one complete fix wave and one scoped re-review. Then use `superpowers:verification-before-completion`, rerun the full green matrix from the exact reviewed commit, and use `superpowers:finishing-a-development-branch` to merge locally into `main` because the user has already selected local integration. Do not push.
