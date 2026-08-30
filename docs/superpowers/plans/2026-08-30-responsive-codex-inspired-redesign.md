# Responsive Codex-Inspired macOS Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the macOS companion around one clean, responsive Codex-inspired shell with a continuous inline reading experience, first-class Models and Preferences destinations, and inline source/model creation while preserving all existing Rust-backed behavior.

**Architecture:** Keep `AppModel` as the only observable state owner and add pure Swift presentation policies for breakpoints, navigation, reading headers, and settings status. Replace the nested reading split with one adaptive `NavigationSplitView`; render selected story detail inline inside a single 720-point reading column, and route add forms into Sources or Models through one explicit editor route. The Rust core, FFI contract, menu-bar popover, storage, packaging, and cross-platform CLI remain unchanged.

**Tech Stack:** Swift 6.2, SwiftUI, Observation, AppKit, Swift Testing, Swift Package Manager, macOS 26 arm64; existing Rust 2024 workspace and UniFFI bindings remain behaviorally unchanged.

**Spec:** `docs/superpowers/specs/2026-08-30-responsive-codex-inspired-redesign-design.md`

## Global Constraints

- Target macOS 26 on Apple Silicon only; retain the current Swift package and standalone app packaging contract.
- Keep the Rust crates, database migrations, FFI behavior, generated bindings, and cross-platform CLI unchanged.
- Keep the menu-bar popover behavior and fixed-content presentation unchanged.
- Use exactly six app destinations in this order: Today, Latest, Saved, Sources, Models, Preferences.
- Use one app-level `NavigationSplitView`; do not introduce an `HSplitView` or a permanent story-detail pane.
- Use layout thresholds exactly as specified: expanded at 820 points and wider, rail from 560 through 819 points, compact below 560 points.
- Use a 228-point expanded sidebar, a 58-point rail, a 420-by-520-point minimum window, and a 720-point maximum reading column.
- Keep the main content surface opaque; material belongs only to native navigation and window chrome.
- Do not add custom cards, gradients, sparkle icons, AI badges, chat controls, or inoperative Search, schedule, summary-depth, appearance, or model-editing controls.
- Preserve `selectedStoryID` as the source of truth for story actions and one-at-a-time inline expansion.
- Preserve Command-R, Command-O, Command-S, and Command-comma behavior.
- Preserve bridge-confirmed source/model mutations, credential redaction, paid-test confirmation, removal confirmation, cached-content failures, and blocking startup failure.
- Honor Reduce Transparency, Increase Contrast, Reduce Motion, light/dark appearance, Dynamic Type, keyboard focus, and VoiceOver labels.
- Generated UniFFI files are build outputs and must not be edited or committed.
- Use `scripts/test-swift-testing.sh` for Swift test verification because it also verifies nonzero Swift Testing execution under Command Line Tools.

## Design Implementation Notes

- **Subject and audience:** a personal daily AI briefing for someone who wants to understand what changed, not operate an agent or complete work.
- **Single job:** let the reader scan a finite ranked set of signals and expand one into enough context to feel informed.
- **Color system:** use semantic macOS text, window, accent, and separator colors from `VisualPolicy`; fixed hex palettes would weaken native light/dark and accessibility behavior.
- **Type system:** use San Francisco through SwiftUI system styles—`largeTitle` for the destination, `title2` for the expanded signal, `body` for prose, and `caption`/`callout` for metadata. Do not introduce a decorative display face.
- **Layout signature:** the ranked signal itself unfolds inline into the reading surface while every neighboring signal collapses to a quiet row. This is the one distinctive interaction; the rest of the shell stays visually restrained.
- **Motion:** do not add custom animation. Native disclosure, focus, and window transitions are sufficient and automatically respect Reduce Motion.

---

### Task 1: Add the adaptive layout and six-destination presentation contract

**Files:**
- Create: `apps/macos/Sources/SignalAppKit/Design/AppLayoutPolicy.swift`
- Create: `apps/macos/Tests/SignalAppKitTests/AppLayoutPolicyTests.swift`
- Modify: `apps/macos/Sources/SignalAppKit/State/AppPreferences.swift`
- Modify: `apps/macos/Sources/SignalAppKit/State/AppModel.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/ReadingWindowView.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AppPresentationTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift`

**Interfaces:**
- Consumes: existing `Destination`, `ReadingCommand`, `VisualPolicy`, and `AppModel.selectedStory` behavior.
- Produces: `AppLayoutMode`, `AppLayoutPolicy.mode(for:)`, `AppLayoutPolicy.navigationWidth(for:)`, `ReadingColumnMetrics`, the `.models` destination, public destination title/icon metadata, and truthful Preferences command copy.

- [ ] **Step 1: Write failing breakpoint and reading-column tests**

Create `AppLayoutPolicyTests.swift`:

```swift
import Testing

@testable import SignalAppKit

struct AppLayoutPolicyTests {
  @Test
  func exactLayoutBreakpointsAreStable() {
    #expect(AppLayoutPolicy.mode(for: 559) == .compact)
    #expect(AppLayoutPolicy.mode(for: 560) == .rail)
    #expect(AppLayoutPolicy.mode(for: 819) == .rail)
    #expect(AppLayoutPolicy.mode(for: 820) == .expanded)
  }

  @Test
  func navigationAndReadingMetricsMatchTheApprovedShell() {
    #expect(AppLayoutPolicy.navigationWidth(for: .expanded) == 228)
    #expect(AppLayoutPolicy.navigationWidth(for: .rail) == 58)
    #expect(AppLayoutPolicy.navigationWidth(for: .compact) == nil)
    #expect(ReadingColumnMetrics.maximumWidth == 720)
    #expect(ReadingColumnMetrics.minimumWindowWidth == 420)
    #expect(ReadingColumnMetrics.minimumWindowHeight == 520)
    #expect(ReadingColumnMetrics.horizontalPadding(for: 480) == 20)
    #expect(ReadingColumnMetrics.horizontalPadding(for: 760) == 28)
    #expect(ReadingColumnMetrics.horizontalPadding(for: 1_100) == 36)
  }
}
```

- [ ] **Step 2: Update destination and keyboard-copy tests to the approved contract**

In `AppPresentationTests.swift`, change the destination assertion to:

```swift
#expect(
  Destination.allCases == [.today, .latest, .saved, .sources, .models, .settings]
)
#expect(Destination.models.title == "Models")
#expect(Destination.settings.title == "Preferences")
```

In `AccessibilityPolicyTests.swift`, change the Command-comma expectation to:

```swift
(.settings, ",", "Open Preferences (⌘,)"),
```

- [ ] **Step 3: Run the Swift suite to verify RED**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: compilation fails because `AppLayoutPolicy`, `ReadingColumnMetrics`, and `Destination.models` do not exist, and the old settings descriptor still says `Open Settings`.

- [ ] **Step 4: Implement the pure layout policy**

Create `AppLayoutPolicy.swift`:

```swift
import Foundation

public enum AppLayoutMode: String, Sendable, Equatable {
  case expanded
  case rail
  case compact
}

public enum AppLayoutPolicy {
  public static let expandedMinimumWidth: CGFloat = 820
  public static let railMinimumWidth: CGFloat = 560

  public static func mode(for availableWidth: CGFloat) -> AppLayoutMode {
    if availableWidth >= expandedMinimumWidth { return .expanded }
    if availableWidth >= railMinimumWidth { return .rail }
    return .compact
  }

  public static func navigationWidth(for mode: AppLayoutMode) -> CGFloat? {
    switch mode {
    case .expanded: 228
    case .rail: 58
    case .compact: nil
    }
  }
}

public enum ReadingColumnMetrics {
  public static let maximumWidth: CGFloat = 720
  public static let minimumWindowWidth: CGFloat = 420
  public static let minimumWindowHeight: CGFloat = 520

  public static func horizontalPadding(for availableWidth: CGFloat) -> CGFloat {
    if availableWidth >= 820 { return 36 }
    if availableWidth >= 560 { return 28 }
    return 20
  }
}
```

- [ ] **Step 5: Add Models and make destination metadata reusable**

In `AppPreferences.swift`, add `.models` between `.sources` and `.settings`:

```swift
public enum Destination: String, CaseIterable, Sendable, Equatable {
  case today
  case latest
  case saved
  case sources
  case models
  case settings
}
```

Move or update the destination extension in `ReadingWindowView.swift` so both metadata properties are internal to the module and exhaustive:

```swift
extension Destination {
  public var title: String {
    switch self {
    case .today: "Today"
    case .latest: "Latest"
    case .saved: "Saved"
    case .sources: "Sources"
    case .models: "Models"
    case .settings: "Preferences"
    }
  }

  var systemImage: String {
    switch self {
    case .today: "sun.max"
    case .latest: "clock"
    case .saved: "bookmark"
    case .sources: "dot.radiowaves.left.and.right"
    case .models: "cpu"
    case .settings: "gearshape"
    }
  }
}
```

Update every `switch` over `Destination` in `AppModel.swift` and `ReadingWindowView.swift`. In `selectedStory`, the non-reading branch becomes:

```swift
case .sources, .models, .settings:
  return nil
```

Route `.models` to `ModelsSettingsView(model:)` and retain `.settings` for `SettingsView(model:)`. Change `ReadingCommand.settings.descriptor.help` to `Open Preferences (⌘,)` and visible button/title copy from Settings to Preferences.

- [ ] **Step 6: Run the Swift suite to verify GREEN**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: all Swift suites pass with six destinations, exact boundary behavior, and updated command copy.

- [ ] **Step 7: Commit**

```bash
git add apps/macos/Sources/SignalAppKit/Design/AppLayoutPolicy.swift \
  apps/macos/Sources/SignalAppKit/State/AppPreferences.swift \
  apps/macos/Sources/SignalAppKit/State/AppModel.swift \
  apps/macos/Sources/SignalAppKit/Views/ReadingWindowView.swift \
  apps/macos/Tests/SignalAppKitTests/AppLayoutPolicyTests.swift \
  apps/macos/Tests/SignalAppKitTests/AppPresentationTests.swift \
  apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift
git commit -m "feat: define adaptive macos shell policy"
```

---

### Task 2: Make inline editors and deterministic inline selection explicit state

**Files:**
- Modify: `apps/macos/Sources/SignalAppKit/State/AppModel.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AppModelTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/ReadingFlowTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/SourceSettingsTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/ModelSettingsTests.swift`

**Interfaces:**
- Consumes: `Destination`, `AppSnapshot`, the existing `story(id:)` lookup, and bridge-confirmed snapshot replacement.
- Produces: `InlineEditorRoute`, `AppModel.inlineEditorRoute`, existing `presentSourceEditor`/`dismissSourceEditor` and `presentModelEditor`/`dismissModelEditor` methods backed by the route, and `AppModel.ensureValidStorySelection()`.

- [ ] **Step 1: Replace sheet-boolean expectations with route expectations**

In `SourceSettingsTests.swift`, replace the editor boolean assertions with:

```swift
  model.presentSourceEditor()
  #expect(model.inlineEditorRoute == .addSource)

  model.destination = .models
  #expect(model.inlineEditorRoute == nil)

  model.dismissSourceEditor()
#expect(model.inlineEditorRoute == nil)
#expect(model.sourceEditorError == nil)
```

In `ModelSettingsTests.swift`, add:

```swift
@Test @MainActor
func modelCreationUsesTheSharedInlineEditorRoute() async {
  let model = AppModel(
    bridge: FakeBridgeClient(snapshot: .fixture),
    preferences: MemoryAppPreferences(welcomeCompleted: true)
  )
  await model.start()

  model.presentModelEditor()
  #expect(model.inlineEditorRoute == .addModel)

  model.dismissModelEditor()
  #expect(model.inlineEditorRoute == nil)
  #expect(model.modelEditorError == nil)
}
```

- [ ] **Step 2: Write failing selection-policy tests**

Add to `ReadingFlowTests.swift`:

```swift
@Test @MainActor
func firstValidStoryExpandsAndValidSelectionPersists() async {
  let initial = snapshot(
    todayStories: [story(id: "first", title: "First"), story(id: "second", title: "Second")],
    latest: [story(id: "first", title: "First"), story(id: "second", title: "Second")],
    saved: []
  )
  let model = AppModel(
    bridge: FakeBridgeClient(snapshot: initial),
    preferences: MemoryAppPreferences(welcomeCompleted: true)
  )

  await model.start()
  #expect(model.selectedStoryID == "first")

  model.selectedStoryID = "second"
  model.destination = .latest
  #expect(model.selectedStoryID == "second")
}

@Test @MainActor
func invalidSelectionFallsBackToFirstAndConfigurationClearsIt() async {
  let first = story(id: "first", title: "First")
  let model = AppModel(
    bridge: FakeBridgeClient(
      snapshot: snapshot(todayStories: [first], latest: [first], saved: [])
    ),
    preferences: MemoryAppPreferences(welcomeCompleted: true)
  )

  await model.start()
  model.selectedStoryID = "missing"
  model.destination = .latest
  #expect(model.selectedStoryID == "first")

  model.destination = .sources
  #expect(model.selectedStoryID == nil)
}
```

Update `destinationChangeAndSnapshotReplacementPruneSelectionByVisibleMembership()` so switching from Latest to Saved chooses the first saved story:

```swift
model.destination = .saved
#expect(model.selectedStoryID == "saved")

await model.reloadSnapshot()
#expect(model.selectedStoryID == nil)
```

Keep the existing empty-destination assertions: when a reading destination has no stories, its fallback remains `nil`; Sources, Models, and Preferences always clear selection.

- [ ] **Step 3: Run the Swift suite to verify RED**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: compilation fails for `InlineEditorRoute` and `inlineEditorRoute`; selection assertions fail because the current validator only clears invalid IDs and never chooses the first story.

- [ ] **Step 4: Implement the shared inline editor route**

Add above `AppModel` in `AppModel.swift`:

```swift
public enum InlineEditorRoute: Sendable, Equatable {
  case addSource
  case addModel
}
```

Replace both sheet booleans with:

```swift
public private(set) var inlineEditorRoute: InlineEditorRoute?
```

Keep the existing method names so view call sites and mutation behavior remain stable:

```swift
public func presentSourceEditor() {
  sourceEditorError = nil
  modelEditorError = nil
  inlineEditorRoute = .addSource
}

public func dismissSourceEditor() {
  sourceEditorError = nil
  if inlineEditorRoute == .addSource { inlineEditorRoute = nil }
}

public func presentModelEditor() {
  modelEditorError = nil
  sourceEditorError = nil
  inlineEditorRoute = .addModel
}

public func dismissModelEditor() {
  modelEditorError = nil
  if inlineEditorRoute == .addModel { inlineEditorRoute = nil }
}
```

In `destination.didSet`, clear an editor that does not belong to the new destination before validating story selection:

```swift
private func dismissIncompatibleInlineEditor() {
  switch (destination, inlineEditorRoute) {
  case (.sources, .addSource), (.models, .addModel), (_, nil):
    return
  default:
    inlineEditorRoute = nil
    sourceEditorError = nil
    modelEditorError = nil
  }
}
```

Call `dismissIncompatibleInlineEditor()` from `destination.didSet`. This prevents a hidden editor from reappearing after navigating away and back.

- [ ] **Step 5: Implement first-valid and persistence selection behavior**

Replace `validateSelectedStoryForDestination()` with:

```swift
public func ensureValidStorySelection() {
  let validIDs: [String]
  switch destination {
  case .today:
    validIDs = snapshot?.today?.items.map { $0.story.id } ?? []
  case .latest:
    validIDs = snapshot?.latest.map(\.id) ?? []
  case .saved:
    validIDs = snapshot?.saved.map(\.id) ?? []
  case .sources, .models, .settings:
    selectedStoryID = nil
    return
  }

  if let selectedStoryID, validIDs.contains(selectedStoryID) { return }
  selectedStoryID = validIDs.first
}
```

Call `ensureValidStorySelection()` from `destination.didSet` and after `snapshot` replacement. Preserve a valid ID so an already selected story remains expanded when it exists in the next reading destination.

- [ ] **Step 6: Run the Swift suite to verify GREEN**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: all tests pass; editor errors still clear on dismissal, the first story is selected after initial load, and configuration destinations have no selected story.

- [ ] **Step 7: Commit**

```bash
git add apps/macos/Sources/SignalAppKit/State/AppModel.swift \
  apps/macos/Tests/SignalAppKitTests/AppModelTests.swift \
  apps/macos/Tests/SignalAppKitTests/ReadingFlowTests.swift \
  apps/macos/Tests/SignalAppKitTests/SourceSettingsTests.swift \
  apps/macos/Tests/SignalAppKitTests/ModelSettingsTests.swift
git commit -m "refactor: model inline editor and story selection state"
```

---

### Task 3: Extract card-free expanded story content

**Files:**
- Create: `apps/macos/Sources/SignalAppKit/Views/ExpandedStoryView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/StoryDetailView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Design/SignalGlass.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/ReadingFlowTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift`

**Interfaces:**
- Consumes: `StoryDetailPresentation`, `StoryDetailElement`, `SummaryVariantPicker`, `GenerationPopover`, `AppModel` story-action APIs, and `ReadingColumnMetrics.maximumWidth`.
- Produces: `ExpandedStoryView.init(story:model:)` and `ExpandedStoryVisualPolicy`; `StoryDetailView` becomes a compatibility wrapper until the split shell is removed in Task 5.

- [ ] **Step 1: Write failing expanded-story visual-policy tests**

Add to `ReadingFlowTests.swift`:

```swift
@Test
func expandedStoryUsesPlainContentInsteadOfAProvenanceCard() {
  #expect(ExpandedStoryVisualPolicy.maximumWidth == 720)
  #expect(ExpandedStoryVisualPolicy.provenanceTreatment == .plainText)
  #expect(!ExpandedStoryVisualPolicy.usesDecorativeContainer)
  #expect(ExpandedStoryVisualPolicy.titleTextStyle == .title2)
}
```

Add these public presentation enums in the planned implementation rather than asserting private SwiftUI view structure:

```swift
public enum ExpandedStoryProvenanceTreatment: Sendable, Equatable { case plainText }
public enum ExpandedStoryTitleTextStyle: Sendable, Equatable { case title2 }
```

- [ ] **Step 2: Run the Swift suite to verify RED**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: compilation fails because `ExpandedStoryVisualPolicy` and its presentation enums do not exist.

- [ ] **Step 3: Create the reusable expanded content view**

Create `ExpandedStoryView.swift` with the extracted element rendering from `StoryDetailView`:

```swift
import AppKit
import SwiftUI

public enum ExpandedStoryProvenanceTreatment: Sendable, Equatable { case plainText }
public enum ExpandedStoryTitleTextStyle: Sendable, Equatable { case title2 }

public enum ExpandedStoryVisualPolicy {
  public static let maximumWidth = ReadingColumnMetrics.maximumWidth
  public static let provenanceTreatment = ExpandedStoryProvenanceTreatment.plainText
  public static let usesDecorativeContainer = false
  public static let titleTextStyle = ExpandedStoryTitleTextStyle.title2
}

public struct ExpandedStoryView: View {
  private let story: Story
  @Bindable private var model: AppModel

  public init(story: Story, model: AppModel) {
    self.story = story
    self.model = model
  }

  public var body: some View {
    let presentation = StoryDetailPresentation(
      story: story,
      sourceNames: ReaderPresentationSupport.sourceNames(
        for: story,
        sources: model.snapshot?.sources ?? []
      ),
      isStale: model.isStoryStale(id: story.id),
      selection: model.summarySelection(for: story.id)
    )

    VStack(alignment: .leading, spacing: 0) {
      if let error = model.storyActionError {
        Label(error, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
          .padding(.bottom, 16)
          .accessibilityLabel("Story action failed. \(error)")
      }
      ForEach(presentation.elements, id: \.self) { element in
        detailElement(element, presentation: presentation)
      }
    }
    .frame(maxWidth: ExpandedStoryVisualPolicy.maximumWidth, alignment: .leading)
  }
}
```

Move `detailElement`, metadata, reading-section, and action helpers into this file. Preserve their existing action closures and accessibility priorities, but change the title and provenance branches to:

```swift
case .title:
  Text(presentation.title)
    .font(.title2.weight(.semibold))
    .textSelection(.enabled)
    .accessibilityAddTraits(.isHeader)
    .padding(.bottom, 14)
case .provenance:
  Text(presentation.provenance.shortLabel)
    .font(.caption)
    .foregroundStyle(.secondary)
    .accessibilityLabel(presentation.provenance.accessibilityLabel)
    .padding(.bottom, 24)
```

Do not add a background shape, capsule, overlay, glass effect, or internal scroll view to `ExpandedStoryView`.

- [ ] **Step 4: Reduce `StoryDetailView` to a compatibility wrapper**

Replace its body implementation with:

```swift
public struct StoryDetailView: View {
  @Bindable private var model: AppModel

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    ScrollView {
      if let story = model.selectedStory {
        ExpandedStoryView(story: story, model: model)
          .padding(.horizontal, 36)
          .padding(.vertical, 32)
          .frame(maxWidth: .infinity)
      } else {
        ContentUnavailableView(
          "Select a story",
          systemImage: "text.page",
          description: Text("Choose a signal from the list to read it here.")
        )
        .frame(maxWidth: .infinity, minHeight: 360)
      }
    }
    .background(Color(nsColor: .textBackgroundColor))
  }
}
```

Keep `StoryDetailPresentation`, URL validation, and save-toggle presentation in `StoryDetailView.swift`; they remain pure presentation logic used by tests and `ExpandedStoryView`.

Delete the now-unused `SignalReadingSurface` type from `SignalGlass.swift`. Keep `SignalGlass` and `SignalGlassControlGroup` unchanged because the menu-bar popover still uses them and that surface is outside this redesign.

- [ ] **Step 5: Run the Swift suite to verify GREEN**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: all tests pass; the same summary/source/save/read/regenerate behavior is available through reusable content without the old provenance capsule or serif hero title.

- [ ] **Step 6: Commit**

```bash
git add apps/macos/Sources/SignalAppKit/Views/ExpandedStoryView.swift \
  apps/macos/Sources/SignalAppKit/Views/StoryDetailView.swift \
  apps/macos/Sources/SignalAppKit/Design/SignalGlass.swift \
  apps/macos/Tests/SignalAppKitTests/ReadingFlowTests.swift \
  apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift
git commit -m "refactor: extract inline expanded story content"
```

---

### Task 4: Replace reading lists with one continuous briefing column

**Files:**
- Create: `apps/macos/Sources/SignalAppKit/Views/BriefingHeaderView.swift`
- Create: `apps/macos/Sources/SignalAppKit/Views/SignalDisclosureView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/TodayView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/StoryListView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/StoryRowView.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/ReadingFlowTests.swift`

**Interfaces:**
- Consumes: `TodayPresentation`, `StoryListPresentation`, `StoryRowView`, `ExpandedStoryView`, `AppModel.selectedStoryID`, and existing story context-menu operations.
- Produces: `BriefingHeaderPresentation`, `BriefingHeaderView`, and `SignalDisclosureView.init(presentation:model:)`; Today, Latest, and Saved each render a single `ScrollView`/`LazyVStack` reading column.

- [ ] **Step 1: Write failing briefing-header tests**

Add to `ReadingFlowTests.swift`:

```swift
@Test
func briefingHeadersExposeCompactIdentityAndCounts() {
  let today = BriefingHeaderPresentation(
    destination: .today,
    snapshot: .fixture,
    calendarDate: "Sunday, August 30, 2026"
  )
  let latest = BriefingHeaderPresentation(
    destination: .latest,
    snapshot: .fixture,
    calendarDate: "Sunday, August 30, 2026"
  )

  #expect(today.title == "Today")
  #expect(latest.title == "Latest")
  #expect(today.dateText == "Sunday, August 30, 2026")
  #expect(today.signalCount == 1)
  #expect(today.enabledSourceCount == 1)
  #expect(today.metadataText == "1 signal · 1 source")
}
```

- [ ] **Step 2: Write a failing one-at-a-time disclosure policy test**

Add:

```swift
@Test
func signalDisclosureExpansionIsDerivedOnlyFromSelectedStoryID() {
  #expect(SignalDisclosurePresentation(storyID: "a", selectedStoryID: "a").isExpanded)
  #expect(!SignalDisclosurePresentation(storyID: "b", selectedStoryID: "a").isExpanded)
  #expect(!SignalDisclosurePresentation(storyID: "a", selectedStoryID: nil).isExpanded)
}
```

- [ ] **Step 3: Run the Swift suite to verify RED**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: compilation fails because the header and disclosure presentation types do not exist.

- [ ] **Step 4: Implement the briefing header**

Create `BriefingHeaderView.swift`:

```swift
import SwiftUI

public struct BriefingHeaderPresentation: Sendable, Equatable {
  public let title: String
  public let dateText: String
  public let signalCount: Int
  public let enabledSourceCount: Int
  public let metadataText: String

  public init(destination: Destination, snapshot: AppSnapshot?, calendarDate: String) {
    title = destination.title
    dateText = calendarDate
    switch destination {
    case .today: signalCount = snapshot?.today?.items.count ?? 0
    case .latest: signalCount = snapshot?.latest.count ?? 0
    case .saved: signalCount = snapshot?.saved.count ?? 0
    case .sources, .models, .settings: signalCount = 0
    }
    enabledSourceCount = snapshot?.sources.filter(\.enabled).count ?? 0
    metadataText = "\(signalCount) \(signalCount == 1 ? "signal" : "signals") · "
      + "\(enabledSourceCount) \(enabledSourceCount == 1 ? "source" : "sources")"
  }
}

public struct BriefingHeaderView: View {
  private let presentation: BriefingHeaderPresentation

  public init(presentation: BriefingHeaderPresentation) {
    self.presentation = presentation
  }

  public var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      Text(presentation.dateText)
        .font(.caption)
        .foregroundStyle(.secondary)
      Text(presentation.title)
        .font(.largeTitle.weight(.semibold))
        .accessibilityAddTraits(.isHeader)
      Text(presentation.metadataText)
        .font(.callout)
        .foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(.bottom, 22)
  }
}
```

At call sites, produce `calendarDate` with one shared formatter:

```swift
Date.now.formatted(
  .dateTime.weekday(.wide).month(.wide).day().year()
)
```

- [ ] **Step 5: Implement the inline signal disclosure**

Create `SignalDisclosureView.swift`:

```swift
import AppKit
import SwiftUI

public struct SignalDisclosurePresentation: Sendable, Equatable {
  public let isExpanded: Bool

  public init(storyID: String, selectedStoryID: String?) {
    isExpanded = storyID == selectedStoryID
  }
}

public struct SignalDisclosureView: View {
  private let presentation: StoryRowPresentation
  @Bindable private var model: AppModel

  public init(presentation: StoryRowPresentation, model: AppModel) {
    self.presentation = presentation
    self.model = model
  }

  public var body: some View {
    let disclosure = SignalDisclosurePresentation(
      storyID: presentation.storyID,
      selectedStoryID: model.selectedStoryID
    )
    VStack(alignment: .leading, spacing: 0) {
      if disclosure.isExpanded, let story = model.story(id: presentation.storyID) {
        ExpandedStoryView(story: story, model: model)
          .padding(.vertical, 22)
      } else {
        Button {
          model.selectedStoryID = presentation.storyID
        } label: {
          StoryRowView(presentation: presentation)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .buttonStyle(.plain)
        .accessibilityHint("Expand this signal")
      }
      Divider()
    }
    .contextMenu { storyContextMenu }
  }
}
```

Add the context menu directly to `SignalDisclosureView`:

```swift
@ViewBuilder
private var storyContextMenu: some View {
  let story = model.story(id: presentation.storyID)
  Button("Open Source", systemImage: "safari") {
    if let value = story?.canonicalURL, let url = StorySourceURL.validated(value) {
      NSWorkspace.shared.open(url)
    }
  }
  .disabled(story.map { !StorySourceActionPresentation(story: $0).isEnabled } ?? true)

  Button(story?.isSaved == true ? "Remove from Saved" : "Save Story", systemImage: "bookmark") {
    model.selectedStoryID = presentation.storyID
    Task { await model.toggleSelectedStorySaved() }
  }
  .disabled(
    story.map { model.storyActionState(for: .saving(storyID: $0.id)) != nil } ?? true
  )

  Button(story?.isRead == true ? "Mark Unread" : "Mark Read", systemImage: "checkmark.circle") {
    model.selectedStoryID = presentation.storyID
    Task { await model.toggleSelectedStoryRead() }
  }
  .disabled(
    story.map { model.storyActionState(for: .markingRead(storyID: $0.id)) != nil } ?? true
  )
}
```

- [ ] **Step 6: Convert Today, Latest, and Saved to the shared reading surface**

In `TodayView`, replace `List(selection:)` with:

```swift
GeometryReader { proxy in
  ScrollView {
    LazyVStack(alignment: .leading, spacing: 0) {
      BriefingHeaderView(
        presentation: BriefingHeaderPresentation(
          destination: model.destination,
          snapshot: model.snapshot,
          calendarDate: Date.now.formatted(
            .dateTime.weekday(.wide).month(.wide).day().year()
          )
        )
      )
      ForEach(presentation.sections) { section in
        Text(section.title)
          .font(.caption.weight(.semibold))
          .foregroundStyle(.secondary)
          .padding(.top, 12)
          .padding(.bottom, 6)
          .accessibilityAddTraits(.isHeader)
        ForEach(section.rows) { row in
          SignalDisclosureView(presentation: row, model: model)
        }
      }
    }
    .frame(maxWidth: ReadingColumnMetrics.maximumWidth, alignment: .leading)
    .padding(.horizontal, ReadingColumnMetrics.horizontalPadding(for: proxy.size.width))
    .padding(.vertical, 30)
    .frame(maxWidth: .infinity, alignment: .center)
  }
  .background(Color(nsColor: .textBackgroundColor))
}
```

In `StoryListView`, use the same outer `GeometryReader`/`ScrollView`/column padding and replace the inner loop with:

```swift
LazyVStack(alignment: .leading, spacing: 0) {
  BriefingHeaderView(
    presentation: BriefingHeaderPresentation(
      destination: model.destination,
      snapshot: model.snapshot,
      calendarDate: Date.now.formatted(
        .dateTime.weekday(.wide).month(.wide).day().year()
      )
    )
  )
  ForEach(presentation.rows) { row in
    SignalDisclosureView(presentation: row, model: model)
  }
}
```

Delete the two old private `storyContextMenu` builders because that behavior now has one implementation in `SignalDisclosureView`.

- [ ] **Step 7: Run the Swift suite to verify GREEN**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: all tests pass; reading destinations have one scroll container, one selected story expands, compact rows remain buttons, and all story actions still target `selectedStoryID`.

- [ ] **Step 8: Commit**

```bash
git add apps/macos/Sources/SignalAppKit/Views/BriefingHeaderView.swift \
  apps/macos/Sources/SignalAppKit/Views/SignalDisclosureView.swift \
  apps/macos/Sources/SignalAppKit/Views/TodayView.swift \
  apps/macos/Sources/SignalAppKit/Views/StoryListView.swift \
  apps/macos/Sources/SignalAppKit/Views/StoryRowView.swift \
  apps/macos/Tests/SignalAppKitTests/ReadingFlowTests.swift
git commit -m "feat: render briefings as one reading column"
```

---

### Task 5: Replace the nested split with the adaptive native shell

**Files:**
- Create: `apps/macos/Sources/SignalAppKit/Views/AppNavigationView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/ReadingWindowView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Window/WindowCoordinator.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Design/VisualPolicy.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AppLayoutPolicyTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AppPresentationTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift`

**Interfaces:**
- Consumes: `AppLayoutPolicy`, destination metadata, `ReadingWindowView.destinationContent`, and all existing toolbar command closures.
- Produces: `AppNavigationPresentation`, `AppNavigationView.init(mode:selection:)`, one `NavigationSplitView` with expanded/rail/compact behavior, and a 420-by-520 minimum `NSWindow`.

- [ ] **Step 1: Write failing navigation-presentation tests**

Add to `AppLayoutPolicyTests.swift`:

```swift
@Test(arguments: [AppLayoutMode.expanded, .rail, .compact])
func navigationPresentationMatchesEachLayoutMode(mode: AppLayoutMode) {
  let presentation = AppNavigationPresentation(mode: mode)

  switch mode {
  case .expanded:
    #expect(presentation.persistentNavigationVisible)
    #expect(presentation.showsDestinationTitles)
    #expect(!presentation.usesToolbarMenu)
  case .rail:
    #expect(presentation.persistentNavigationVisible)
    #expect(!presentation.showsDestinationTitles)
    #expect(!presentation.usesToolbarMenu)
  case .compact:
    #expect(!presentation.persistentNavigationVisible)
    #expect(!presentation.showsDestinationTitles)
    #expect(presentation.usesToolbarMenu)
  }
}
```

Add to `AppPresentationTests.swift` after creating/opening the managed window:

```swift
#expect(window.minSize == NSSize(width: 420, height: 520))
```

Add `.compactNavigation` to the expected icon-control coverage in `AccessibilityPolicyTests` and assert its label/help are nonempty.

- [ ] **Step 2: Run the Swift suite to verify RED**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: compilation fails for `AppNavigationPresentation` and `.compactNavigation`; the window minimum assertion reports 860 by 600.

- [ ] **Step 3: Implement adaptive navigation content**

Create `AppNavigationView.swift`:

```swift
import AppKit
import SwiftUI

public struct AppNavigationPresentation: Sendable, Equatable {
  public let persistentNavigationVisible: Bool
  public let showsDestinationTitles: Bool
  public let usesToolbarMenu: Bool

  public init(mode: AppLayoutMode) {
    persistentNavigationVisible = mode != .compact
    showsDestinationTitles = mode == .expanded
    usesToolbarMenu = mode == .compact
  }
}

public struct AppNavigationView: View {
  private let mode: AppLayoutMode
  @Binding private var selection: Destination?

  public init(mode: AppLayoutMode, selection: Binding<Destination?>) {
    self.mode = mode
    _selection = selection
  }

  public var body: some View {
    if mode == .expanded {
      List(Destination.allCases, id: \.self, selection: $selection) { destination in
        Label(destination.title, systemImage: destination.systemImage)
          .tag(destination)
      }
      .navigationTitle("AI Daily Signal")
    } else {
      VStack(spacing: 8) {
        ForEach(Destination.allCases, id: \.self) { destination in
          Button {
            selection = destination
          } label: {
            Image(systemName: destination.systemImage)
              .frame(
                minWidth: VisualPolicy().minimumControlDimension,
                minHeight: VisualPolicy().minimumControlDimension
              )
          }
          .buttonStyle(.plain)
          .foregroundStyle(
            selection == destination
              ? Color.accentColor
              : Color(nsColor: .secondaryLabelColor)
          )
          .accessibilityLabel(destination.title)
          .help(destination.title)
        }
        Spacer()
      }
      .padding(.vertical, 12)
      .frame(maxWidth: .infinity)
    }
  }
}
```

Add `.compactNavigation` to `IconControlDescriptor` with label `Choose section` and help `Open the app navigation menu`.

- [ ] **Step 4: Rebuild `ReadingWindowView` around one split**

Add state:

```swift
@State private var columnVisibility = NavigationSplitViewVisibility.all
```

Replace `readingShell` with a `GeometryReader` that computes the mode and renders exactly one split:

```swift
GeometryReader { proxy in
  let mode = AppLayoutPolicy.mode(for: proxy.size.width)
  let navigationWidth = AppLayoutPolicy.navigationWidth(for: mode) ?? 58
  NavigationSplitView(columnVisibility: $columnVisibility) {
    AppNavigationView(mode: mode, selection: destinationSelection)
      .navigationSplitViewColumnWidth(
        min: navigationWidth,
        ideal: navigationWidth,
        max: navigationWidth
      )
  } detail: {
    destinationContent
      .navigationTitle(model.destination.title)
      .background(Color(nsColor: .textBackgroundColor))
  }
  .onAppear { columnVisibility = mode == .compact ? .detailOnly : .all }
  .onChange(of: mode) { _, value in
    columnVisibility = value == .compact ? .detailOnly : .all
  }
  .toolbar { toolbarContent(mode: mode) }
}
```

Delete the reading-destination `HSplitView` branch entirely. Keep loading, cached error, startup error, refresh notice, and destination routing behavior in `destinationContent`.

Keep Refresh visible. Keep Open Source and Save in the toolbar only for Today, Latest, and Saved; keep their current disabled conditions and keyboard shortcuts. Keep Preferences available via Command-comma and the navigation destinations.

Implement the toolbar builder with the current command closures:

```swift
@ToolbarContentBuilder
private func toolbarContent(mode: AppLayoutMode) -> some ToolbarContent {
  if mode == .compact {
    ToolbarItem(placement: .navigation) {
      Menu {
        ForEach(Destination.allCases, id: \.self) { destination in
          Button(destination.title, systemImage: destination.systemImage) {
            model.destination = destination
          }
        }
      } label: {
        Label(model.destination.title, systemImage: "sidebar.left")
      }
      .accessibilityLabel(IconControlDescriptor.compactNavigation.label)
      .help(IconControlDescriptor.compactNavigation.help)
    }
  }

  ToolbarItemGroup(placement: .primaryAction) {
    refreshToolbarButton(
      ReadingToolbarPresentation(
        phase: model.phase,
        refreshInProgress: model.activeOperationID != nil
      ).refreshControl
    )

    if [.today, .latest, .saved].contains(model.destination) {
      Button("Open Source", systemImage: "safari") {
        openSelectedSource()
      }
      .keyboardShortcut(ReadingCommand.openSource.keyEquivalent, modifiers: .command)
      .disabled(
        model.selectedStory.map {
          !StorySourceActionPresentation(story: $0).isEnabled
        } ?? true
      )
      .help(ReadingCommand.openSource.descriptor.help)

      Button("Save Story", systemImage: "bookmark") {
        Task { await model.saveSelectedStory() }
      }
      .keyboardShortcut(ReadingCommand.save.keyEquivalent, modifiers: .command)
      .disabled(
        model.selectedStory.map {
          $0.isSaved || model.storyActionState(for: .saving(storyID: $0.id)) != nil
        } ?? true
      )
      .help(ReadingCommand.save.descriptor.help)
    }

    Button("Preferences", systemImage: "gearshape") {
      model.destination = .settings
    }
    .keyboardShortcut(ReadingCommand.settings.keyEquivalent, modifiers: .command)
    .help(ReadingCommand.settings.descriptor.help)
  }
}
```

- [ ] **Step 5: Lower both SwiftUI and AppKit minimum sizes**

Change the root frame in `ReadingWindowView` to:

```swift
.frame(
  minWidth: ReadingColumnMetrics.minimumWindowWidth,
  minHeight: ReadingColumnMetrics.minimumWindowHeight
)
```

Change `WindowCoordinator.makeWindow()` to:

```swift
window.minSize = NSSize(
  width: ReadingColumnMetrics.minimumWindowWidth,
  height: ReadingColumnMetrics.minimumWindowHeight
)
```

Keep the 1,120-by-760 opening content rect and frame autosave name.

- [ ] **Step 6: Run the Swift suite and structural search**

Run:

```bash
scripts/test-swift-testing.sh
rg -n "HSplitView|frame\(minWidth: (860|440|320)" \
  apps/macos/Sources/SignalAppKit/Views \
  apps/macos/Sources/SignalAppKit/Window
```

Expected: the test suite passes and `rg` returns no matches.

- [ ] **Step 7: Commit**

```bash
git add apps/macos/Sources/SignalAppKit/Views/AppNavigationView.swift \
  apps/macos/Sources/SignalAppKit/Views/ReadingWindowView.swift \
  apps/macos/Sources/SignalAppKit/Window/WindowCoordinator.swift \
  apps/macos/Sources/SignalAppKit/Design/VisualPolicy.swift \
  apps/macos/Tests/SignalAppKitTests/AppLayoutPolicyTests.swift \
  apps/macos/Tests/SignalAppKitTests/AppPresentationTests.swift \
  apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift
git commit -m "feat: add adaptive native macos shell"
```

---

### Task 6: Move source creation inline without changing source mutations

**Files:**
- Modify: `apps/macos/Sources/SignalAppKit/Views/SourcesView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/SourceEditorView.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/SourceSettingsTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift`

**Interfaces:**
- Consumes: `AppModel.inlineEditorRoute`, `SourceEditorDraft`, `SourceEditorPresentation`, and existing add/toggle/remove bridge methods.
- Produces: `SourceEditorLayoutPolicy` and a Sources destination that inserts `SourceEditorView` in its main content instead of presenting a sheet.

- [ ] **Step 1: Write failing inline source-layout tests**

Add to `SourceSettingsTests.swift`:

```swift
@Test
func sourceCreationUsesAResponsiveInlineForm() {
  #expect(SourceEditorLayoutPolicy.presentation == .inline)
  #expect(SourceEditorLayoutPolicy.maximumWidth == 720)
  #expect(SourceEditorLayoutPolicy.minimumWidth == nil)
  #expect(!SourceEditorLayoutPolicy.usesSheet)
}
```

Define the finite test vocabulary in the implementation:

```swift
public enum EditorPresentationStyle: Sendable, Equatable { case inline }
```

- [ ] **Step 2: Run the Swift suite to verify RED**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: compilation fails because `SourceEditorLayoutPolicy` and `EditorPresentationStyle` do not exist.

- [ ] **Step 3: Make `SourceEditorView` responsive and inline**

Add to `SourceEditorView.swift`:

```swift
public enum EditorPresentationStyle: Sendable, Equatable { case inline }

public enum SourceEditorLayoutPolicy {
  public static let presentation = EditorPresentationStyle.inline
  public static let maximumWidth: CGFloat = ReadingColumnMetrics.maximumWidth
  public static let minimumWidth: CGFloat? = nil
  public static let usesSheet = false
}
```

Remove `.frame(minWidth: 460, idealWidth: 520, minHeight: 390)`, the modal navigation title, and the bottom `.bar` inset. Keep the `Form`, but place compact actions directly after its sections:

```swift
VStack(alignment: .leading, spacing: 16) {
  HStack {
    Text("Add Personal Source")
      .font(.title2.weight(.semibold))
    Spacer()
    Button("Cancel") { model.dismissSourceEditor() }
      .keyboardShortcut(.cancelAction)
    Button("Add Source") { save() }
      .keyboardShortcut(.defaultAction)
      .disabled(!presentation.canSave)
  }
  sourceForm(presentation: presentation)
  .formStyle(.grouped)
}
.frame(maxWidth: SourceEditorLayoutPolicy.maximumWidth, alignment: .leading)
```

Extract the current form body without changing its fields into:

```swift
private func sourceForm(presentation: SourceEditorPresentation) -> some View {
  Form {
    Section("Feed") {
      TextField("Name", text: $draft.name, prompt: Text("Publication or project"))
        .focused($focusedField, equals: .name)
        .textContentType(.name)
      TextField("Feed URL", text: $draft.feedURL, prompt: Text("https://example.com/feed.xml"))
        .focused($focusedField, equals: .feedURL)
        .textContentType(.URL)
      TextField("Category", text: $draft.category, prompt: Text("Research"))
        .focused($focusedField, equals: .category)
    }
    Section {
      TextField("Weight", text: $weightText, prompt: Text("0.8"))
        .focused($focusedField, equals: .weight)
        .accessibilityHint("Enter a number from 0 to 1")
      Toggle("Enabled", isOn: $draft.enabled)
    } header: {
      Text("Briefing")
    } footer: {
      Text("Weight controls how strongly this feed contributes to story ranking.")
    }
    if let message = presentation.validationMessage ?? model.sourceEditorError {
      Section {
        Label(message, systemImage: "exclamationmark.circle")
          .foregroundStyle(.secondary)
          .accessibilityLabel("Source form error: \(message)")
      }
    }
  }
}
```

When saving, retain the existing rule: dismiss only after `await model.addSource(input)` returns `true`; on failure, keep the route and draft visible.

- [ ] **Step 4: Replace the Sources sheet with inline destination content**

In `SourcesView.swift`, delete `sourceEditorPresentation` and `.sheet`. Wrap the destination in:

```swift
VStack(spacing: 0) {
  if model.inlineEditorRoute == .addSource {
    ScrollView {
      SourceEditorView(model: model)
        .padding(.horizontal, 28)
        .padding(.vertical, 24)
        .frame(maxWidth: .infinity)
    }
  } else {
    List {
      Section("Standard Sources") {
        if standard.isEmpty {
          sourceEmptyRow(
            "No standard sources",
            detail: "Build a briefing to initialize the standard source set."
          )
        } else {
          ForEach(standard) { source in sourceRow(source) }
        }
      }
      Section("Personal Sources") {
        if personal.isEmpty {
          sourceEmptyRow(
            "No personal sources",
            detail: "Add an RSS or Atom feed to include it in future briefings."
          )
        } else {
          ForEach(personal) { source in sourceRow(source) }
        }
      }
    }
    .listStyle(.inset)
  }
}
```

Keep the Add Source toolbar button, exact `⇧⌘N` shortcut, per-row busy state, enable toggle, personal-only removal policy, and confirmation dialog. Disable Add Source while `.addSource` is already active.

- [ ] **Step 5: Run tests and verify the sheet/minimum-width removal**

Run:

```bash
scripts/test-swift-testing.sh
rg -n "\.sheet|frame\(minWidth: 460" \
  apps/macos/Sources/SignalAppKit/Views/SourcesView.swift \
  apps/macos/Sources/SignalAppKit/Views/SourceEditorView.swift
```

Expected: all tests pass and `rg` returns no matches.

- [ ] **Step 6: Commit**

```bash
git add apps/macos/Sources/SignalAppKit/Views/SourcesView.swift \
  apps/macos/Sources/SignalAppKit/Views/SourceEditorView.swift \
  apps/macos/Tests/SignalAppKitTests/SourceSettingsTests.swift \
  apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift
git commit -m "feat: make source creation inline"
```

---

### Task 7: Separate Models and Preferences and make model creation inline

**Files:**
- Modify: `apps/macos/Sources/SignalAppKit/Views/ModelsSettingsView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/ModelProfileEditorView.swift`
- Modify: `apps/macos/Sources/SignalAppKit/Views/SettingsView.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/ModelSettingsTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AppPresentationTests.swift`

**Interfaces:**
- Consumes: `AppModel.inlineEditorRoute`, existing model creation/test/default/remove behavior, `ModelProfileEditorDraft`, and `AppSnapshot.hasUsableAIProfile`.
- Produces: `ModelEditorLayoutPolicy`, `PreferencesPresentation`, a first-class Models page, and a separate Preferences page containing truthful current-state rows only.

- [ ] **Step 1: Write failing model layout and Preferences presentation tests**

Add to `ModelSettingsTests.swift`:

```swift
@Test
func modelCreationUsesAResponsiveInlineFormWithoutAddingEditScope() {
  #expect(ModelEditorLayoutPolicy.presentation == .inline)
  #expect(ModelEditorLayoutPolicy.maximumWidth == 720)
  #expect(ModelEditorLayoutPolicy.minimumWidth == nil)
  #expect(!ModelEditorLayoutPolicy.usesSheet)
  #expect(ModelsSettingsRenderPlan.profileActions == [.test, .setDefault, .remove])
}
```

Add to `AppPresentationTests.swift`:

```swift
@Test
func preferencesOnlyDescribeBehaviorTheCurrentAppOwns() {
  let optional = PreferencesPresentation(hasUsableAIProfile: false)
  let enabled = PreferencesPresentation(hasUsableAIProfile: true)

  #expect(optional.storage == "On this Mac")
  #expect(optional.aiSummaries == "Optional")
  #expect(enabled.aiSummaries == "Enabled")
  #expect(optional.launchBehavior == "Menu bar companion")
  #expect(optional.cliCompatibility == "Shares local data and configuration")
  #expect(!optional.hasInoperativeControls)
}
```

- [ ] **Step 2: Run the Swift suite to verify RED**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: compilation fails for `ModelEditorLayoutPolicy` and `PreferencesPresentation`.

- [ ] **Step 3: Make model creation responsive and inline**

Add to `ModelProfileEditorView.swift`:

```swift
public enum ModelEditorLayoutPolicy {
  public static let presentation = EditorPresentationStyle.inline
  public static let maximumWidth: CGFloat = ReadingColumnMetrics.maximumWidth
  public static let minimumWidth: CGFloat? = nil
  public static let usesSheet = false
}
```

Remove `.frame(minWidth: 500, idealWidth: 560, minHeight: 620)`, the modal navigation title, and bottom `.bar` inset. Use the same compact inline heading/actions structure as the source form, while retaining all current sections, secure credential handling, exact budget strings, consent disclosure, error copy, and `onDisappear` secret clearing. The Add Model button still calls `takeInput()` once and dismisses only after confirmed success.

- [ ] **Step 4: Replace the Models sheet with inline page content**

In `ModelsSettingsView.swift`, delete `modelEditorPresentation` and `.sheet`. Use:

```swift
VStack(spacing: 0) {
  if model.inlineEditorRoute == .addModel {
    ScrollView {
      ModelProfileEditorView(model: model)
        .padding(.horizontal, 28)
        .padding(.vertical, 24)
        .frame(maxWidth: .infinity)
    }
  } else {
    modelList
  }
}
```

Extract the current Models section into a computed view without changing its row/action helpers:

```swift
private var modelList: some View {
  List {
    Section("Models") {
      let profiles = model.snapshot?.modelProfiles ?? []
      if profiles.isEmpty {
        Label {
          VStack(alignment: .leading, spacing: 3) {
            Text("No model profiles")
            Text("Raw and Smart summaries remain available without AI.")
              .font(.caption)
              .foregroundStyle(.secondary)
          }
        } icon: {
          Image(systemName: "cpu")
            .foregroundStyle(.secondary)
        }
        .padding(.vertical, 6)
      } else {
        ForEach(profiles) { profile in
          profileRow(profile)
        }
      }
    }
  }
  .listStyle(.inset)
}
```

Move the existing `Briefing` status rows out of Models. Keep Add Model in the toolbar, paid test confirmation, removal confirmation, cleanup warning, default eligibility, and all bridge-confirmed mutation behavior. Do not add edit actions or test-on-save.

- [ ] **Step 5: Build the truthful Preferences destination**

Replace `SettingsView.swift` with:

```swift
import SwiftUI

public struct PreferencesPresentation: Sendable, Equatable {
  public let storage = "On this Mac"
  public let aiSummaries: String
  public let launchBehavior = "Menu bar companion"
  public let cliCompatibility = "Shares local data and configuration"
  public let hasInoperativeControls = false

  public init(hasUsableAIProfile: Bool) {
    aiSummaries = hasUsableAIProfile ? "Enabled" : "Optional"
  }
}

public struct SettingsView: View {
  private let model: AppModel

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    let presentation = PreferencesPresentation(
      hasUsableAIProfile: model.snapshot?.hasUsableAIProfile == true
    )
    Form {
      Section("Briefing") {
        LabeledContent("Storage", value: presentation.storage)
        LabeledContent("AI summaries", value: presentation.aiSummaries)
      }
      Section("Companion") {
        LabeledContent("Launch behavior", value: presentation.launchBehavior)
        LabeledContent("CLI", value: presentation.cliCompatibility)
      }
    }
    .formStyle(.grouped)
    .frame(maxWidth: ReadingColumnMetrics.maximumWidth)
    .padding(.horizontal, 24)
    .padding(.vertical, 20)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
  }
}
```

These are status rows, not toggles: the current app has no preference or bridge operation for scheduling, summary depth, appearance, or CLI installation.

- [ ] **Step 6: Run tests and verify modal/minimum-width removal**

Run:

```bash
scripts/test-swift-testing.sh
rg -n "\.sheet|frame\(minWidth: 500|case edit|ModelsSettingsAction\.edit" \
  apps/macos/Sources/SignalAppKit/Views/ModelsSettingsView.swift \
  apps/macos/Sources/SignalAppKit/Views/ModelProfileEditorView.swift \
  apps/macos/Sources/SignalAppKit/Views/SettingsView.swift
```

Expected: all tests pass and `rg` returns no matches.

- [ ] **Step 7: Commit**

```bash
git add apps/macos/Sources/SignalAppKit/Views/ModelsSettingsView.swift \
  apps/macos/Sources/SignalAppKit/Views/ModelProfileEditorView.swift \
  apps/macos/Sources/SignalAppKit/Views/SettingsView.swift \
  apps/macos/Tests/SignalAppKitTests/ModelSettingsTests.swift \
  apps/macos/Tests/SignalAppKitTests/AppPresentationTests.swift
git commit -m "feat: separate inline models and preferences"
```

---

### Task 8: Restyle onboarding in the shared restrained visual language

**Files:**
- Modify: `apps/macos/Sources/SignalAppKit/Views/WelcomeView.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AppPresentationTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift`

**Interfaces:**
- Consumes: `WelcomePresentation`, `WelcomeContent`, `AppModel.buildFirstBriefing()`, `VisualPolicy.minimumControlDimension`, and `ReadingColumnMetrics`.
- Produces: `WelcomeLayoutPolicy`; the welcome screen keeps its existing one-action behavior and disclosures but uses compact, responsive controls and no decorative card.

- [ ] **Step 1: Write failing welcome-layout tests**

Add to `AppPresentationTests.swift`:

```swift
@Test
func welcomeUsesTheSharedResponsiveContentLanguage() {
  #expect(WelcomeLayoutPolicy.maximumWidth == 520)
  #expect(WelcomeLayoutPolicy.horizontalPadding == 28)
  #expect(WelcomeLayoutPolicy.primaryControlSize == .regular)
  #expect(!WelcomeLayoutPolicy.usesDecorativeCard)
  #expect(!WelcomeLayoutPolicy.usesGradient)
  #expect(WelcomeContent.primaryAction == "Build My First Briefing")
}
```

Define `WelcomeControlSize` as a finite presentation enum rather than testing SwiftUI internals:

```swift
public enum WelcomeControlSize: Sendable, Equatable { case regular }
```

- [ ] **Step 2: Run the Swift suite to verify RED**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: compilation fails because `WelcomeLayoutPolicy` and `WelcomeControlSize` do not exist.

- [ ] **Step 3: Implement the restrained responsive welcome**

Add to `WelcomeView.swift`:

```swift
public enum WelcomeControlSize: Sendable, Equatable { case regular }

public enum WelcomeLayoutPolicy {
  public static let maximumWidth: CGFloat = 520
  public static let horizontalPadding: CGFloat = 28
  public static let primaryControlSize = WelcomeControlSize.regular
  public static let usesDecorativeCard = false
  public static let usesGradient = false
}
```

Add `import AppKit` above the existing SwiftUI import, then refactor the view to:

```swift
ScrollView {
  VStack(alignment: .leading, spacing: 22) {
    Image(systemName: "dot.radiowaves.left.and.right")
      .font(.title2.weight(.medium))
      .foregroundStyle(.tint)
      .accessibilityHidden(true)
    Text("AI Daily Signal")
      .font(.largeTitle.weight(.semibold))
      .accessibilityAddTraits(.isHeader)
    Text("A focused daily briefing for understanding what changed in AI.")
      .font(.title3)
      .foregroundStyle(.secondary)
    Divider()
    Text(WelcomeContent.localFirstExplanation)
      .foregroundStyle(.secondary)
    Button(WelcomeContent.primaryAction) {
      Task { await model.buildFirstBriefing() }
    }
    .buttonStyle(.borderedProminent)
    .controlSize(.regular)
    .disabled(!presentation.primaryActionEnabled)
    if presentation.showsProgress {
      ProgressView("Building your briefing…")
        .controlSize(.small)
    }
    Label(WelcomeContent.refreshDisclosure, systemImage: "network")
      .font(.caption)
      .foregroundStyle(.secondary)
  }
  .frame(maxWidth: WelcomeLayoutPolicy.maximumWidth, alignment: .leading)
  .padding(.horizontal, WelcomeLayoutPolicy.horizontalPadding)
  .padding(.vertical, 44)
  .frame(maxWidth: .infinity)
}
.background(Color(nsColor: .textBackgroundColor))
```

Keep the exact existing action, optional-AI copy, network disclosure, progress behavior, accessibility hint, and sort priorities. Do not add onboarding pages, scheduling, model setup, or source editing.

- [ ] **Step 4: Run the Swift suite to verify GREEN**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: all tests pass and the welcome screen remains behaviorally identical while fitting the 420-point minimum window.

- [ ] **Step 5: Commit**

```bash
git add apps/macos/Sources/SignalAppKit/Views/WelcomeView.swift \
  apps/macos/Tests/SignalAppKitTests/AppPresentationTests.swift \
  apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift
git commit -m "refactor: simplify responsive welcome layout"
```

---

### Task 9: Complete accessibility, preview, packaging, and visual-size verification

**Files:**
- Modify: `apps/macos/Sources/SignalAppKit/Design/PreviewFixtures.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/PreviewFixtureTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AlphaAcceptanceTests.swift`
- Modify: `apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift`
- Modify: `docs/superpowers/specs/2026-08-30-responsive-codex-inspired-redesign-design.md`

**Interfaces:**
- Consumes: the completed adaptive shell, all existing preview states, standalone bundle scripts, and the approved acceptance criteria.
- Produces: explicit expanded/rail/compact preview-size fixtures, final acceptance assertions, recorded visual verification results, and a verified standalone app without CLI coupling.

- [ ] **Step 1: Write failing preview-size and acceptance tests**

Extend `PreviewFixture` with `windowWidth` and `windowHeight`, then add tests in `PreviewFixtureTests.swift`:

```swift
@Test
func responsivePreviewFixturesCoverEveryApprovedWindowClass() {
  let sizes = Set(PreviewFixtures.responsive.map { [$0.windowWidth, $0.windowHeight] })
  #expect(sizes.contains([1_100, 720]))
  #expect(sizes.contains([760, 640]))
  #expect(sizes.contains([480, 620]))
  #expect(
    Set(PreviewFixtures.responsive.map { AppLayoutPolicy.mode(for: $0.windowWidth) })
      == [.expanded, .rail, .compact]
  )
}
```

Add to `AlphaAcceptanceTests.swift`:

```swift
@Test
func redesignedCompanionKeepsTheApprovedStructuralBoundaries() {
  #expect(Destination.allCases == [.today, .latest, .saved, .sources, .models, .settings])
  #expect(ReadingColumnMetrics.minimumWindowWidth == 420)
  #expect(ReadingColumnMetrics.minimumWindowHeight == 520)
  #expect(ReadingColumnMetrics.maximumWidth == 720)
  #expect(!ExpandedStoryVisualPolicy.usesDecorativeContainer)
  #expect(!SourceEditorLayoutPolicy.usesSheet)
  #expect(!ModelEditorLayoutPolicy.usesSheet)
}
```

- [ ] **Step 2: Run the Swift suite to verify RED**

Run:

```bash
scripts/test-swift-testing.sh
```

Expected: compilation fails because preview fixtures do not yet expose window dimensions or `responsive` fixtures.

- [ ] **Step 3: Add deterministic responsive preview fixtures**

Extend `PreviewFixture`:

```swift
public let windowWidth: CGFloat
public let windowHeight: CGFloat

public init(
  id: String,
  phase: AppPhase,
  snapshot: AppSnapshot?,
  refreshNotice: RefreshNotice? = nil,
  selectedStoryID: String? = nil,
  appearance: SignalAppearance = .light,
  reduceTransparency: Bool = false,
  increaseContrast: Bool = false,
  windowWidth: CGFloat = 1_100,
  windowHeight: CGFloat = 720
) {
  self.id = id
  self.phase = phase
  self.snapshot = snapshot
  self.refreshNotice = refreshNotice
  self.selectedStoryID = selectedStoryID
  self.appearance = appearance
  self.reduceTransparency = reduceTransparency
  self.increaseContrast = increaseContrast
  self.windowWidth = windowWidth
  self.windowHeight = windowHeight
}
```

The defaults keep existing state fixtures source-compatible. Add:

```swift
public static let expandedWindow = PreviewFixture(
  id: "expanded-window",
  phase: .ready,
  snapshot: populatedSnapshot,
  selectedStoryID: aiStory.id,
  windowWidth: 1_100,
  windowHeight: 720
)

public static let railWindow = PreviewFixture(
  id: "rail-window",
  phase: .ready,
  snapshot: populatedSnapshot,
  selectedStoryID: aiStory.id,
  windowWidth: 760,
  windowHeight: 640
)

public static let compactWindow = PreviewFixture(
  id: "compact-window",
  phase: .ready,
  snapshot: populatedSnapshot,
  selectedStoryID: aiStory.id,
  windowWidth: 480,
  windowHeight: 620
)

public static let responsive = [expandedWindow, railWindow, compactWindow]
```

Keep fixture timestamps deterministic and the existing security audit redaction coverage intact.

- [ ] **Step 4: Run the complete automated verification matrix**

Run:

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
rg -n "HSplitView|\.sheet|frame\(minWidth: (860|500|460|440|320)|sparkles|LinearGradient" \
  apps/macos/Sources/SignalAppKit/Views/ReadingWindowView.swift \
  apps/macos/Sources/SignalAppKit/Views/TodayView.swift \
  apps/macos/Sources/SignalAppKit/Views/StoryListView.swift \
  apps/macos/Sources/SignalAppKit/Views/StoryDetailView.swift \
  apps/macos/Sources/SignalAppKit/Views/ExpandedStoryView.swift \
  apps/macos/Sources/SignalAppKit/Views/SourcesView.swift \
  apps/macos/Sources/SignalAppKit/Views/SourceEditorView.swift \
  apps/macos/Sources/SignalAppKit/Views/ModelsSettingsView.swift \
  apps/macos/Sources/SignalAppKit/Views/ModelProfileEditorView.swift \
  apps/macos/Sources/SignalAppKit/Views/WelcomeView.swift
```

Expected: every command passes; the final `rg` returns no matches. `scripts/smoke-test-macos-app.sh` confirms the standalone app launches with an isolated home and no installed CLI dependency.

- [ ] **Step 5: Perform visual verification at all approved sizes and accessibility modes**

Launch the built bundle:

```bash
open "target/macos/AI Daily Signal.app"
```

Inspect and record each of the following in the spec's Testing Strategy section under a new `Implementation verification` subsection:

```text
1100×720 — expanded sidebar — light and dark
760×640 — icon rail — light and dark
480×620 — compact navigation — light and dark
480×620 — Reduce Transparency enabled
480×620 — Increase Contrast enabled
420×520 — minimum size smoke check
```

For every row, verify: no horizontal clipping; no overlapping toolbar items; destination changes remain reachable; only one story is expanded; prose stays within 720 points; source/model forms fit without minimum-width pressure; validation copy wraps; icon-only controls have VoiceOver labels and help; content remains opaque; sidebar material becomes opaque under Reduce Transparency.

- [ ] **Step 6: Update the spec status with exact verification evidence**

Append this format to the spec using the actual command results and inspection date:

```markdown
## Implementation verification

- Swift presentation and acceptance suites: passed via `scripts/test-swift-testing.sh`.
- Rust workspace: passed via `cargo test --workspace --all-features` with no Rust changes.
- Standalone bundle verification and smoke launch: passed.
- Visual sizes: 1100×720, 760×640, 480×620, and 420×520 inspected in light/dark and required accessibility modes.
- Structural scan: no nested `HSplitView`, editor sheets, conflicting minimum widths, sparkle symbols, or gradients in the redesigned window views.
```

Do not claim an item passed unless its command or visual check completed successfully.

- [ ] **Step 7: Commit the verification artifacts**

```bash
git add apps/macos/Sources/SignalAppKit/Design/PreviewFixtures.swift \
  apps/macos/Tests/SignalAppKitTests/PreviewFixtureTests.swift \
  apps/macos/Tests/SignalAppKitTests/AlphaAcceptanceTests.swift \
  apps/macos/Tests/SignalAppKitTests/AccessibilityPolicyTests.swift \
  docs/superpowers/specs/2026-08-30-responsive-codex-inspired-redesign-design.md
git commit -m "test: verify responsive macos redesign"
```

---

## Completion Gate

Before declaring the redesign complete, use `superpowers:requesting-code-review` for a fresh requirements and quality review, resolve all valid findings, then use `superpowers:verification-before-completion` and rerun the Task 9 matrix from a clean working tree. The final handoff must name the resulting commits, state that changes were merged locally on `main`, and report any visual checks that could not be performed instead of implying they passed.
