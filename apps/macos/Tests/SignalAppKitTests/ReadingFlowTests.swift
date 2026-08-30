import Foundation
import SwiftUI
import Testing

@testable import SignalAppKit

@Suite(.serialized)
struct ReadingFlowTests {
  @Test
  func briefingHeadersExposeCompactIdentityAndCounts() {
    // Break caught: header identity or counts drifting from the loaded destination snapshot.
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

  @Test
  func signalDisclosureExpansionIsDerivedOnlyFromSelectedStoryID() {
    // Break caught: expanding a row independently of the model's single selected story.
    #expect(SignalDisclosurePresentation(storyID: "a", selectedStoryID: "a").isExpanded)
    #expect(!SignalDisclosurePresentation(storyID: "b", selectedStoryID: "a").isExpanded)
    #expect(!SignalDisclosurePresentation(storyID: "a", selectedStoryID: nil).isExpanded)
  }

  @Test
  func todayRenderPlanPreservesContiguousSectionRunsAndExactFlatOrder() {
    // Break caught: globally grouping a repeated section and moving its later stories earlier.
    let first = story(id: "first", title: "First")
    let second = story(id: "second", title: "Second")
    let third = story(id: "third", title: "Third")
    let briefing = Briefing(
      date: "2026-08-30",
      generatedAt: referenceDate,
      isStale: false,
      items: [
        item(first, position: 8, section: "Research"),
        item(second, position: 2, section: "Top Signals"),
        item(third, position: 4, section: "Research"),
      ]
    )

    let presentation = TodayPresentation(
      briefing: briefing,
      sources: sourceFixtures,
      selectionForStory: { _ in .smart },
      relativeTo: referenceDate
    )

    #expect(presentation.sections.map(\.title) == ["Research", "Top Signals", "Research"])
    #expect(
      presentation.sections.flatMap { $0.rows.map(\.storyID) } == ["first", "second", "third"])
    #expect(presentation.sections.flatMap { $0.rows.map(\.rank) } == [8, 2, 4])
  }

  @Test
  func todayEmptyStateDirectsARefreshInsteadOfInventingContent() {
    // Break caught: rendering an endless/loading feed when the loaded Today briefing is empty.
    let presentation = TodayPresentation(
      briefing: Briefing(date: "2026-08-30", generatedAt: nil, isStale: false, items: []),
      sources: [],
      selectionForStory: { _ in .smart }
    )

    #expect(presentation.sections.isEmpty)
    #expect(presentation.emptyState?.title == "No briefing yet")
    #expect(presentation.emptyState?.action == .refresh)
  }

  @Test
  func latestAndSavedRenderPlansPreserveLoadedSnapshotOrder() {
    // Break caught: silently resorting or paginating the finite bridge-owned datasets.
    let stories = [
      story(id: "older", title: "Older", publishedAt: referenceDate.addingTimeInterval(-600)),
      story(id: "newer", title: "Newer", publishedAt: referenceDate),
    ]

    let latest = StoryListPresentation(
      kind: .latest,
      stories: stories,
      sources: sourceFixtures,
      staleStoryIDs: [],
      selectionForStory: { _ in .smart },
      relativeTo: referenceDate
    )
    let saved = StoryListPresentation(
      kind: .saved,
      stories: stories,
      sources: sourceFixtures,
      staleStoryIDs: [],
      selectionForStory: { _ in .smart },
      relativeTo: referenceDate
    )

    #expect(latest.rows.map(\.storyID) == ["older", "newer"])
    #expect(saved.rows.map(\.storyID) == ["older", "newer"])
    #expect(latest.isFinite && saved.isFinite)
  }

  @Test
  func rowPresentationCarriesSourceFreshnessAndPlainSummaryProvenance() {
    // Break caught: hiding source/staleness or marketing Smart as an AI-generated summary.
    let row = StoryRowPresentation(
      story: story(id: "story", title: "A signal", saved: true),
      primarySource: "Signal Research",
      relativeTime: "now",
      isStale: true,
      rank: 3,
      summarySelection: .smart
    )

    #expect(row.primarySource == "Signal Research")
    #expect(row.isStale)
    #expect(row.isSaved)
    #expect(row.rank == 3)
    #expect(row.provenance == .smart)
    #expect(row.accessibilitySummary.contains("stale"))
    #expect(row.accessibilitySummary.contains("Rank 3"))
    #expect(row.provenance.shortLabel == "Smart · local algorithmic summary")
  }

  @Test
  func storyHeaderKeepsIdentityAndDisclosesItsState() {
    // Break caught: collapsed and expanded stories rendering different identity or disclosure state.
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
    #expect(!collapsed.showsSelectionSurface)
    #expect(expanded.showsSelectionSurface)
    #expect(collapsed.titleLineLimit == 3)
  }

  @Test
  func accessibilityTextLeavesStoryTitlesUnrestricted() {
    // Break caught: clipping long story identity when Dynamic Type enters an accessibility size.
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
  }

  @Test
  func detailRenderPlanUsesApprovedHierarchyAndDoesNotInventStructuredSmartFields() {
    // Break caught: rearranging reading hierarchy or presenting Smart text as validated AI fields.
    let value = story(id: "story", title: "A signal", saved: true)
    let detail = StoryDetailPresentation(
      story: value,
      sourceNames: ["Signal Research", "AI Lab Notes"],
      isStale: true,
      selection: .smart,
      relativeTo: referenceDate
    )

    #expect(
      detail.elements == [
        .metadata, .title, .provenance, .whatHappened, .scoreAndSources, .actions,
      ])
    #expect(detail.provenance == .smart)
    #expect(detail.whatHappened == value.smartSummary)
    #expect(detail.whyItMatters == nil)
    #expect(detail.caveat == nil)
    #expect(detail.stateLabels == ["Unread", "Saved"])
    #expect(detail.accessibilityMetadata.contains("Unread"))
    #expect(detail.accessibilityMetadata.contains("Saved"))
  }

  @Test
  func rawAndAIReadingModesRenderOnlyTheirOwnedContent() throws {
    // Break caught: leaking AI prose into Raw or replacing validated AI fields with the excerpt.
    let value = story(id: "story", title: "A signal")
    let raw = StoryDetailPresentation(
      story: value,
      sourceNames: ["Signal Research"],
      isStale: false,
      selection: .raw
    )
    let ai = StoryDetailPresentation(
      story: value,
      sourceNames: ["Signal Research"],
      isStale: false,
      selection: .ai(variantID: "variant-old")
    )

    #expect(raw.originalExcerpt == value.excerpt)
    #expect(raw.whatHappened == nil)
    #expect(raw.elements.contains(.originalExcerpt))
    #expect(ai.originalExcerpt == nil)
    #expect(ai.whatHappened == "AI happened")
    #expect(ai.whyItMatters == "AI matters")
    #expect(ai.caveat == "AI caveat")
    #expect(ai.provenance == .ai(provider: "OpenAI", model: "gpt-signal"))
  }

  @Test
  func expandedBodyExcludesIdentityAndKeepsSemanticOrder() {
    // Break caught: duplicating header identity in the expanded body or rearranging article sections.
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

  @Test
  func pickerOrdersRawSmartThenImmutableAINewestFirst() {
    // Break caught: conflating Smart with AI or presenting cached variants in unstable order.
    let value = story(id: "story", title: "A signal")
    let picker = SummaryVariantPickerPresentation(story: value, selection: .smart)

    #expect(
      picker.options.map(\.selection) == [
        .raw, .smart, .ai(variantID: "variant-new"), .ai(variantID: "variant-old"),
      ])
    #expect(picker.options[1].provenance == .smart)
    #expect(picker.options[1].detail == "Local algorithmic summary")
    #expect(picker.options[1].displayLabel == "Smart · local algorithmic summary")
    #expect(picker.options[2].provenance == .ai(provider: "Anthropic", model: "claude-signal"))
    #expect(picker.options[3].provenance == .ai(provider: "OpenAI", model: "gpt-signal"))
    #expect(picker.accessibilityLabel == "Summary version")
    #expect(picker.selectedValue == "Smart · local algorithmic summary")
  }

  @Test
  func sourceURLPolicyAllowsOnlyAbsoluteHTTPAndHTTPS() {
    // Break caught: passing file, script, malformed, or relative URLs to NSWorkspace.
    #expect(StorySourceURL.validated("https://example.test/story")?.host == "example.test")
    #expect(StorySourceURL.validated("http://example.test/story") != nil)
    #expect(StorySourceURL.validated("file:///tmp/story") == nil)
    #expect(StorySourceURL.validated("javascript:alert(1)") == nil)
    #expect(StorySourceURL.validated("https:relative") == nil)
    #expect(StorySourceURL.validated("/relative") == nil)
    #expect(
      !StorySourceActionPresentation(
        story: story(id: "unsafe", title: "Unsafe", url: "file:///tmp/story")
      ).isEnabled)
    #expect(StorySourceActionPresentation(story: story(id: "safe", title: "Safe")).isEnabled)
  }

  @Test
  func wholeBriefingStalenessMarksEveryLatestAndSavedRow() {
    // Break caught: propagating only item-level staleness when the entire briefing is stale.
    let first = story(id: "first", title: "First")
    let second = story(id: "second", title: "Second")
    let value = AppSnapshot(
      revision: .fixture,
      status: AppSnapshot.fixture.status,
      today: Briefing(
        date: "2026-08-30",
        generatedAt: referenceDate,
        isStale: true,
        items: [
          item(first, position: 1, section: "Signals"),
          item(second, position: 2, section: "Signals"),
        ]
      ),
      latest: [first, second],
      saved: [second],
      sources: sourceFixtures,
      modelProfiles: [.fixture],
      defaultModelProfileID: "profile-1",
      hasUsableAIProfile: true
    )

    #expect(ReaderPresentationSupport.staleStoryIDs(in: value) == ["first", "second"])
  }

  @Test
  func generationRequiresAnEnabledResolvedDefaultOrSelection() {
    // Break caught: treating a missing/disabled profile identifier as authorization to generate.
    let enabled = modelProfile(id: "enabled", enabled: true)
    let disabled = modelProfile(id: "disabled", enabled: false)

    let invalidDefault = GenerationPopoverPresentation(
      profiles: [enabled, disabled],
      defaultProfileID: "disabled",
      selectedProfileID: nil
    )
    let invalidSelection = GenerationPopoverPresentation(
      profiles: [enabled, disabled],
      defaultProfileID: "enabled",
      selectedProfileID: "disabled"
    )
    let validSelection = GenerationPopoverPresentation(
      profiles: [enabled, disabled],
      defaultProfileID: nil,
      selectedProfileID: "enabled"
    )

    #expect(invalidDefault.requiresExplicitSelection)
    #expect(!invalidDefault.canGenerate)
    #expect(!invalidSelection.canGenerate)
    #expect(validSelection.canGenerate)
  }

  @Test @MainActor
  func savedRemovalWaitsForConfirmationThenUpdatesTodayLatestAndSaved() async {
    // Break caught: optimistically removing a save or leaving duplicate story collections stale.
    let savedStory = story(id: "story", title: "Saved", saved: true)
    let other = story(id: "other", title: "Other", saved: true)
    let initial = snapshot(
      todayStories: [savedStory], latest: [savedStory], saved: [savedStory, other])
    let confirmed = story(id: "story", title: "Saved", saved: false)
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.savedResult = StoryMutationResult(story: confirmed, revision: mutationRevision)
    bridge.suspendNextSavedMutation()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true, selectedDestination: .saved)
    )
    await model.start()
    model.selectedStoryID = "story"

    let task = Task { await model.toggleSelectedStorySaved() }
    await eventually { bridge.savedRequests.count == 1 }
    #expect(model.snapshot?.saved.map(\.id) == ["story", "other"])
    #expect(model.activeStoryAction == .saving(storyID: "story"))

    bridge.releaseSavedMutations()
    await task.value
    #expect(model.snapshot?.today?.items.first?.story.isSaved == false)
    #expect(model.snapshot?.latest.first?.isSaved == false)
    #expect(model.snapshot?.saved.map(\.id) == ["other"])
    #expect(model.snapshot?.revision == mutationRevision)
    #expect(model.activeStoryAction == nil)
  }

  @Test @MainActor
  func snapshotReplacementKeepsStoryAndLocalModeSelectionByIdentifier() async {
    // Break caught: binding detail selection to a replaced value instance instead of stable IDs.
    let original = story(id: "story", title: "Original")
    let replacementStory = story(id: "story", title: "Replacement")
    let initial = snapshot(todayStories: [original], latest: [original], saved: [])
    let replacement = snapshot(
      revision: mutationRevision,
      todayStories: [replacementStory],
      latest: [replacementStory],
      saved: []
    )
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.enqueueSnapshot(replacement)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "story"
    model.showSummary(.raw)

    await model.reloadSnapshot()

    #expect(model.selectedStoryID == "story")
    #expect(model.selectedStory?.title == "Replacement")
    #expect(model.selectedSummarySelection == .raw)
  }

  @Test @MainActor
  func unsavingAStoryThatExistsOnlyInSavedClearsTheDanglingDetailSelection() async {
    // Break caught: retaining a selected identifier after confirmed removal from its final list.
    let savedStory = story(id: "story", title: "Saved only", saved: true)
    let initial = snapshot(todayStories: [], latest: [], saved: [savedStory])
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.savedResult = StoryMutationResult(
      story: copy(savedStory, saved: false),
      revision: mutationRevision
    )
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true, selectedDestination: .saved)
    )
    await model.start()
    model.selectedStoryID = "story"

    await model.toggleSelectedStorySaved()

    #expect(model.snapshot?.saved.isEmpty == true)
    #expect(model.selectedStoryID == nil)
  }

  @Test @MainActor
  func unsavingInSavedClearsSelectionEvenWhenTheStoryRemainsInLatest() async {
    // Break caught: validating detail selection against the global snapshot instead of Saved.
    let savedStory = story(id: "story", title: "Saved", saved: true)
    let initial = snapshot(todayStories: [], latest: [savedStory], saved: [savedStory])
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.savedResult = StoryMutationResult(
      story: copy(savedStory, saved: false),
      revision: mutationRevision
    )
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true, selectedDestination: .saved)
    )
    await model.start()
    model.selectedStoryID = "story"

    await model.toggleSelectedStorySaved()

    #expect(model.snapshot?.latest.first?.id == "story")
    #expect(model.snapshot?.saved.isEmpty == true)
    #expect(model.selectedStoryID == nil)
  }

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

  @Test @MainActor
  func destinationChangeAndSnapshotReplacementPruneSelectionByVisibleMembership() async {
    // Break caught: retaining a Latest selection after switching to Saved or after Saved removal.
    let latestOnly = story(id: "latest", title: "Latest only")
    let savedStory = story(id: "saved", title: "Saved", saved: true)
    let initial = snapshot(
      todayStories: [], latest: [latestOnly, savedStory], saved: [savedStory])
    let replacement = snapshot(
      revision: mutationRevision,
      todayStories: [],
      latest: [latestOnly, savedStory],
      saved: []
    )
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.enqueueSnapshot(replacement)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true, selectedDestination: .latest)
    )
    await model.start()
    model.selectedStoryID = "latest"

    model.destination = .saved
    #expect(model.selectedStoryID == "saved")

    await model.reloadSnapshot()
    #expect(model.selectedStoryID == nil)
  }

  @Test @MainActor
  func enabledCrossActionQueuesBehindTheInFlightMutationAndBothApplyInRevisionOrder() async {
    // Break caught: showing Mark Read enabled during Save but silently dropping its action.
    let initialStory = story(id: "story", title: "Signal")
    let savedStory = copy(initialStory, saved: true)
    let readSavedStory = copy(savedStory, read: true)
    let initial = snapshot(todayStories: [initialStory], latest: [initialStory], saved: [])
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.savedResult = StoryMutationResult(story: savedStory, revision: mutationRevision)
    bridge.readResult = StoryMutationResult(
      story: readSavedStory,
      revision: StateRevision(dataGeneration: 3, sourceConfigRevision: "source-a")
    )
    bridge.suspendNextSavedMutation()
    bridge.suspendNextReadMutation()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "story"

    let saveTask = Task { await model.toggleSelectedStorySaved() }
    await eventually { bridge.savedRequests.count == 1 }
    let readTask = Task { await model.toggleSelectedStoryRead() }
    await eventually {
      model.storyActionState(for: .markingRead(storyID: "story")) == .queued
    }

    #expect(model.storyActionState(for: .saving(storyID: "story")) == .inFlight)
    #expect(bridge.readRequests.isEmpty)
    bridge.releaseSavedMutations()
    await eventually { bridge.readRequests.count == 1 }
    #expect(model.selectedStory?.isSaved == true)
    #expect(model.storyActionState(for: .markingRead(storyID: "story")) == .inFlight)

    bridge.releaseReadMutations()
    await saveTask.value
    await readTask.value
    #expect(model.selectedStory?.isSaved == true)
    #expect(model.selectedStory?.isRead == true)
    #expect(model.snapshot?.revision.dataGeneration == 3)
    #expect(bridge.maximumConcurrentBridgeCalls == 1)
  }

  @Test @MainActor
  func saveInvalidatesARevisionReadStartedBeforeTheMutation() async {
    // Break caught: a late polling revision launching a stale reload after confirmed Save.
    let initialStory = story(id: "story", title: "Signal")
    let savedStory = copy(initialStory, saved: true)
    let futureRevision = StateRevision(dataGeneration: 9, sourceConfigRevision: "external")
    let bridge = FakeBridgeClient(
      snapshot: snapshot(todayStories: [initialStory], latest: [initialStory], saved: []),
      revisions: [futureRevision]
    )
    bridge.savedResult = StoryMutationResult(story: savedStory, revision: mutationRevision)
    bridge.suspendNextStateRevision()
    bridge.suspendNextSavedMutation()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true),
      pollInterval: .milliseconds(1)
    )
    await model.start()
    model.selectedStoryID = "story"
    model.setActive(true)
    await eventually { bridge.stateRevisionCalls == 1 }

    let saveTask = Task { await model.toggleSelectedStorySaved() }
    await eventually {
      model.storyActionState(for: .saving(storyID: "story")) == .queued
    }
    #expect(bridge.savedRequests.isEmpty)
    bridge.releaseStateRevisions()
    await eventually { bridge.savedRequests.count == 1 }
    bridge.releaseSavedMutations()
    await saveTask.value
    model.stopPolling()
    try? await Task.sleep(for: .milliseconds(20))

    #expect(model.selectedStory?.isSaved == true)
    #expect(model.snapshot?.revision == mutationRevision)
    #expect(bridge.snapshotCalls == 2)
  }

  @Test @MainActor
  func readBlocksReloadAndRejectsItsOlderSnapshotAfterConfirmation() async {
    // Break caught: overlapping Read with reload, then overwriting the confirmed record.
    let initialStory = story(id: "story", title: "Signal")
    let readStory = copy(initialStory, read: true)
    let initial = snapshot(todayStories: [initialStory], latest: [initialStory], saved: [])
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.readResult = StoryMutationResult(story: readStory, revision: mutationRevision)
    bridge.enqueueSnapshot(initial)
    bridge.suspendNextReadMutation()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    bridge.suspendNextSnapshot()
    model.selectedStoryID = "story"

    let readTask = Task { await model.toggleSelectedStoryRead() }
    await eventually { bridge.readRequests.count == 1 }
    let reloadTask = Task { await model.reloadSnapshot() }
    try? await Task.sleep(for: .milliseconds(10))
    #expect(bridge.snapshotCalls == 1)
    #expect(model.selectedStory?.isRead == false)

    bridge.releaseReadMutations()
    await eventually { bridge.snapshotCalls == 2 }
    bridge.releaseSnapshots()
    await readTask.value
    await reloadTask.value
    #expect(model.selectedStory?.isRead == false)
    #expect(model.snapshot?.revision == initial.revision)
    #expect(
      model.storyActionError(for: .markingRead(storyID: "story"))
        == "The story state changed before it could be confirmed. Reload and try again."
    )
    #expect(bridge.maximumConcurrentBridgeCalls == 1)
  }

  @Test @MainActor
  func summarySelectionBlocksReloadAndRejectsItsOlderSnapshotAfterConfirmation() async {
    // Break caught: a stale reload replacing a confirmed AI variant selection.
    let initialStory = story(id: "story", title: "Signal")
    let selectedStory = copy(initialStory, selectedVariantID: "variant-new")
    let initial = snapshot(todayStories: [initialStory], latest: [initialStory], saved: [])
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.summaryResult = StoryMutationResult(story: selectedStory, revision: mutationRevision)
    bridge.enqueueSnapshot(initial)
    bridge.suspendNextSummaryMutation()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    bridge.suspendNextSnapshot()
    model.selectedStoryID = "story"

    let selectTask = Task { await model.selectSummary(.ai(variantID: "variant-new")) }
    await eventually { bridge.summaryRequests.count == 1 }
    let reloadTask = Task { await model.reloadSnapshot() }
    try? await Task.sleep(for: .milliseconds(10))
    #expect(bridge.snapshotCalls == 1)
    #expect(model.selectedSummarySelection == .ai(variantID: "variant-old"))

    bridge.releaseSummaryMutations()
    await eventually { bridge.snapshotCalls == 2 }
    bridge.releaseSnapshots()
    await selectTask.value
    await reloadTask.value
    #expect(model.selectedSummarySelection == .ai(variantID: "variant-old"))
    #expect(model.snapshot?.revision == initial.revision)
    #expect(
      model.storyActionError(for: .selectingSummary(storyID: "story"))
        == "The story state changed before it could be confirmed. Reload and try again."
    )
    #expect(bridge.maximumConcurrentBridgeCalls == 1)
  }

  @Test @MainActor
  func regenerationBlocksReloadAndRejectsItsOlderSnapshotAfterConfirmation() async {
    // Break caught: a stale reload replacing a newly generated confirmed variant.
    let initialStory = story(id: "story", title: "Signal")
    let generatedStory = copy(initialStory, selectedVariantID: "variant-new")
    let initial = snapshot(todayStories: [initialStory], latest: [initialStory], saved: [])
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.generationResult = GenerationResult(
      story: generatedStory,
      selectedSummary: variantNew,
      revision: mutationRevision
    )
    bridge.enqueueSnapshot(initial)
    bridge.suspendNextGeneration()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    bridge.suspendNextSnapshot()
    model.selectedStoryID = "story"

    let generationTask = Task {
      await model.regenerateSelectedStory(profileID: "profile-1", force: false)
    }
    await eventually { bridge.generationRequests.count == 1 }
    let reloadTask = Task { await model.reloadSnapshot() }
    try? await Task.sleep(for: .milliseconds(10))
    #expect(bridge.snapshotCalls == 1)
    #expect(model.selectedSummarySelection == .ai(variantID: "variant-old"))

    bridge.releaseGenerations()
    await eventually { bridge.snapshotCalls == 2 }
    bridge.releaseSnapshots()
    await generationTask.value
    await reloadTask.value
    #expect(model.selectedSummarySelection == .ai(variantID: "variant-old"))
    #expect(model.snapshot?.revision == initial.revision)
    #expect(
      model.storyActionError(for: .regenerating(storyID: "story"))
        == "The story state changed before it could be confirmed. Reload and try again."
    )
    #expect(bridge.maximumConcurrentBridgeCalls == 1)
  }

  @Test @MainActor
  func readAndAIVariantSelectionApplyOnlyConfirmedRecordsAndRevisions() async {
    // Break caught: toggling read locally or displaying an AI variant before Rust confirms it.
    let initialStory = story(id: "story", title: "A signal")
    let initial = snapshot(todayStories: [initialStory], latest: [initialStory], saved: [])
    let readStory = copy(initialStory, read: true)
    let selectedStory = copy(initialStory, selectedVariantID: "variant-new")
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.readResult = StoryMutationResult(story: readStory, revision: mutationRevision)
    bridge.summaryResult = StoryMutationResult(
      story: selectedStory,
      revision: StateRevision(dataGeneration: 3, sourceConfigRevision: "source-a")
    )
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "story"

    await model.toggleSelectedStoryRead()
    #expect(model.selectedStory?.isRead == true)
    #expect(model.snapshot?.revision == mutationRevision)

    await model.selectSummary(.ai(variantID: "variant-new"))
    #expect(model.selectedSummarySelection == .ai(variantID: "variant-new"))
    #expect(model.snapshot?.revision.dataGeneration == 3)
    #expect(bridge.summaryRequests == [SummaryRequest(storyID: "story", variantID: "variant-new")])
  }

  @Test @MainActor
  func failedRegenerationRetainsPriorSmartAndAISelections() async {
    // Break caught: clearing readable prior content when provider generation fails.
    let initialStory = story(id: "story", title: "A signal")
    let initial = snapshot(todayStories: [initialStory], latest: [initialStory], saved: [])
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.generationError = BridgeError.providerUnavailable
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "story"

    model.showSummary(.smart)
    await model.regenerateSelectedStory(profileID: "profile-1", force: false)
    #expect(model.selectedSummarySelection == .smart)
    #expect(model.selectedStory?.smartSummary == initialStory.smartSummary)

    bridge.summaryResult = StoryMutationResult(story: initialStory, revision: mutationRevision)
    await model.selectSummary(.ai(variantID: "variant-old"))
    await model.regenerateSelectedStory(profileID: "profile-1", force: true)
    #expect(model.selectedSummarySelection == .ai(variantID: "variant-old"))
    #expect(model.selectedStory?.summaryVariants == initialStory.summaryVariants)
    #expect(model.storyActionError == "The AI provider is unavailable. Smart summaries were kept.")
  }

  @Test @MainActor
  func regenerationRequiresAProfileWhenNoDefaultExists() async {
    // Break caught: issuing a paid provider request without an explicit or configured default profile.
    let initialStory = story(id: "story", title: "A signal")
    let initial = snapshot(
      todayStories: [initialStory],
      latest: [initialStory],
      saved: [],
      defaultProfileID: nil
    )
    let bridge = FakeBridgeClient(snapshot: initial)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "story"

    await model.regenerateSelectedStory(profileID: nil, force: false)

    #expect(bridge.generationRequests.isEmpty)
    #expect(model.storyActionError == "Choose an enabled model profile before regenerating.")
    #expect(model.selectedSummarySelection == .ai(variantID: "variant-old"))
  }

  @Test @MainActor
  func regenerationRejectsMissingDisabledAndDanglingProfileIdentifiers() async {
    // Break caught: sending a provider request through a profile that is absent or disabled.
    let initialStory = story(id: "story", title: "Signal")
    let enabled = modelProfile(id: "enabled", enabled: true)
    let disabled = modelProfile(id: "disabled", enabled: false)
    let initial = snapshot(
      todayStories: [initialStory],
      latest: [initialStory],
      saved: [],
      defaultProfileID: "missing",
      modelProfiles: [enabled, disabled]
    )
    let bridge = FakeBridgeClient(snapshot: initial)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "story"

    await model.regenerateSelectedStory(profileID: nil, force: false)
    await model.regenerateSelectedStory(profileID: "disabled", force: false)
    await model.regenerateSelectedStory(profileID: "missing", force: false)

    #expect(bridge.generationRequests.isEmpty)
    #expect(model.storyActionError == "Choose an enabled model profile before regenerating.")
  }

  @Test @MainActor
  func storyActionErrorsStayWithTheirStoryAndAction() async {
    // Break caught: showing story A's provider failure while reading story B.
    let first = story(id: "first", title: "First")
    let second = story(id: "second", title: "Second")
    let initial = snapshot(todayStories: [first, second], latest: [first, second], saved: [])
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.generationError = BridgeError.providerUnavailable
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "first"

    await model.regenerateSelectedStory(profileID: "profile-1", force: false)
    #expect(
      model.storyActionError(for: .regenerating(storyID: "first"))
        == "The AI provider is unavailable. Smart summaries were kept."
    )

    model.selectedStoryID = "second"
    #expect(model.storyActionError == nil)
    #expect(model.storyActionError(for: .regenerating(storyID: "first")) != nil)
  }

  @Test @MainActor
  func equalGenerationSummaryConfirmationStillUpdatesTheConfirmedVariantAndVisibleMode() async {
    // Break caught: treating a cache-hit confirmation as stale solely because generation is equal.
    let initialStory = story(id: "story", title: "Signal")
    let confirmedStory = copy(initialStory, selectedVariantID: "variant-new")
    let initial = snapshot(todayStories: [initialStory], latest: [initialStory], saved: [])
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.summaryResult = StoryMutationResult(
      story: confirmedStory,
      revision: initial.revision
    )
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "story"

    await model.selectSummary(.ai(variantID: "variant-new"))

    #expect(model.selectedStory?.selectedSummary?.id == "variant-new")
    #expect(model.selectedSummarySelection == .ai(variantID: "variant-new"))
    #expect(model.snapshot?.revision == initial.revision)
  }

  @Test @MainActor
  func forwardStoryGenerationPublishesTheAuthoritativeFullSnapshot() async {
    // Break caught: publishing one story with a later global revision while hiding an unrelated
    // CLI story change that is already part of that revision.
    let localStory = story(id: "story", title: "Local story")
    let cliStoryBefore = story(id: "cli-story", title: "CLI-updated story", read: false)
    let cliStory = story(id: "cli-story", title: "CLI-updated story", read: true)
    let initialRevision = StateRevision(dataGeneration: 5, sourceConfigRevision: "source-a")
    let initial = snapshot(
      revision: initialRevision,
      todayStories: [localStory],
      latest: [localStory, cliStoryBefore],
      saved: []
    )
    let confirmedStory = copy(localStory, saved: true)
    let forwardRevision = StateRevision(dataGeneration: 7, sourceConfigRevision: "source-a")
    let authoritative = snapshot(
      revision: forwardRevision,
      todayStories: [confirmedStory],
      latest: [confirmedStory, cliStory],
      saved: [confirmedStory]
    )
    let bridge = FakeBridgeClient(snapshot: initial, revisions: [forwardRevision])
    bridge.savedResult = StoryMutationResult(story: confirmedStory, revision: forwardRevision)
    bridge.enqueueSnapshot(authoritative)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true),
      pollInterval: .milliseconds(1)
    )
    await model.start()
    model.selectedStoryID = localStory.id

    await model.toggleSelectedStorySaved()
    model.setActive(true)
    await eventually { bridge.stateRevisionCalls >= 1 }
    model.stopPolling()

    #expect(model.snapshot == authoritative)
    #expect(model.snapshot?.latest.first(where: { $0.id == cliStory.id })?.isRead == true)
    #expect(bridge.snapshotCalls == 2)
  }

  @Test @MainActor
  func lowerGenerationConfirmationIsReconciledInsteadOfReplacingNewerSnapshotState() async {
    // Break caught: allowing an older partial mutation record to overwrite a newer full snapshot.
    let initialStory = story(id: "story", title: "Signal")
    let initialRevision = StateRevision(dataGeneration: 5, sourceConfigRevision: "source-a")
    let initial = snapshot(
      revision: initialRevision,
      todayStories: [initialStory],
      latest: [initialStory],
      saved: []
    )
    let reconciledStory = copy(initialStory, saved: true)
    let reconciledRevision = StateRevision(dataGeneration: 6, sourceConfigRevision: "source-a")
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.savedResult = StoryMutationResult(
      story: reconciledStory,
      revision: StateRevision(dataGeneration: 4, sourceConfigRevision: "source-a")
    )
    bridge.enqueueSnapshot(
      snapshot(
        revision: reconciledRevision,
        todayStories: [reconciledStory],
        latest: [reconciledStory],
        saved: [reconciledStory]
      )
    )
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "story"

    await model.toggleSelectedStorySaved()

    #expect(bridge.snapshotCalls == 2)
    #expect(model.snapshot?.revision == reconciledRevision)
    #expect(model.selectedStory?.isSaved == true)
  }

  @Test @MainActor
  func lowerGenerationAIConfirmationCannotOverrideTheAuthoritativeReconciledSelection() async {
    // Break caught: displaying a stale AI choice after the full reconciliation rejected it.
    let initialStory = story(id: "story", title: "Signal")
    let initialRevision = StateRevision(dataGeneration: 5, sourceConfigRevision: "source-a")
    let initial = snapshot(
      revision: initialRevision,
      todayStories: [initialStory],
      latest: [initialStory],
      saved: []
    )
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.summaryResult = StoryMutationResult(
      story: copy(initialStory, selectedVariantID: "variant-new"),
      revision: StateRevision(dataGeneration: 4, sourceConfigRevision: "source-a")
    )
    bridge.enqueueSnapshot(initial)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "story"

    await model.selectSummary(.ai(variantID: "variant-new"))

    #expect(bridge.snapshotCalls == 2)
    #expect(model.selectedStory?.selectedSummary?.id == "variant-old")
    #expect(model.selectedSummarySelection == .ai(variantID: "variant-old"))
  }

  @Test @MainActor
  func changedSourceRevisionReconcilesTheWholeSnapshotBeforePublishingTheNewRevision() async {
    // Break caught: pairing a new source revision with the old sources/models projection.
    let initialStory = story(id: "story", title: "Signal")
    let initial = snapshot(todayStories: [initialStory], latest: [initialStory], saved: [])
    let confirmedStory = copy(initialStory, saved: true)
    let compositeRevision = StateRevision(dataGeneration: 2, sourceConfigRevision: "source-b")
    let updatedSources = [
      Source(
        id: "source-1",
        name: "Updated Research",
        category: "research",
        enabled: true,
        weight: 0.7,
        feedURL: "https://updated.example.test/feed",
        origin: .personal
      )
    ]
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.savedResult = StoryMutationResult(story: confirmedStory, revision: compositeRevision)
    bridge.enqueueSnapshot(
      snapshot(
        revision: compositeRevision,
        todayStories: [confirmedStory],
        latest: [confirmedStory],
        saved: [confirmedStory],
        sources: updatedSources
      )
    )
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    bridge.suspendNextSnapshot()
    model.selectedStoryID = "story"

    let saveTask = Task { await model.toggleSelectedStorySaved() }
    await eventually { bridge.snapshotCalls == 2 }
    #expect(model.snapshot?.revision == initial.revision)
    #expect(model.snapshot?.sources.map(\.name) == sourceFixtures.map(\.name))
    #expect(model.selectedStory?.isSaved == false)

    bridge.releaseSnapshots()
    await saveTask.value
    #expect(model.snapshot?.revision == compositeRevision)
    #expect(model.snapshot?.sources.map(\.name) == ["Updated Research"])
    #expect(model.selectedStory?.isSaved == true)
  }

  @Test @MainActor
  func mutationQueuesBehindSuspendedPollingRevisionWithoutOverlappingBridgeCalls() async {
    // Break caught: stateRevision bypassing the coordinator and overlapping a visible mutation.
    let initialStory = story(id: "story", title: "Signal")
    let initial = snapshot(todayStories: [initialStory], latest: [initialStory], saved: [])
    let confirmedStory = copy(initialStory, saved: true)
    let bridge = FakeBridgeClient(snapshot: initial, revisions: [initial.revision])
    bridge.savedResult = StoryMutationResult(story: confirmedStory, revision: mutationRevision)
    bridge.suspendNextStateRevision()
    bridge.suspendNextSavedMutation()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true),
      pollInterval: .milliseconds(1)
    )
    await model.start()
    model.selectedStoryID = "story"
    model.setActive(true)
    await eventually { bridge.stateRevisionCalls == 1 }

    let saveTask = Task { await model.toggleSelectedStorySaved() }
    await eventually {
      model.storyActionState(for: .saving(storyID: "story")) == .queued
    }
    #expect(bridge.savedRequests.isEmpty)
    #expect(bridge.maximumConcurrentBridgeCalls == 1)

    bridge.releaseStateRevisions()
    await eventually { bridge.savedRequests.count == 1 }
    model.stopPolling()
    bridge.releaseSavedMutations()
    await saveTask.value
    #expect(model.selectedStory?.isSaved == true)
    #expect(bridge.maximumConcurrentBridgeCalls == 1)
  }

  @Test @MainActor
  func laterSmartIntentSurvivesSuspendedAISelectionConfirmation() async {
    // Break caught: a late AI confirmation overriding the reader's newer local mode choice.
    let initialStory = story(id: "story", title: "Signal")
    let confirmedStory = copy(initialStory, selectedVariantID: "variant-new")
    let initial = snapshot(todayStories: [initialStory], latest: [initialStory], saved: [])
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.summaryResult = StoryMutationResult(story: confirmedStory, revision: mutationRevision)
    bridge.suspendNextSummaryMutation()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "story"

    let selectTask = Task { await model.selectSummary(.ai(variantID: "variant-new")) }
    await eventually { bridge.summaryRequests.count == 1 }
    model.showSummary(.smart)
    bridge.releaseSummaryMutations()
    await selectTask.value

    #expect(model.selectedStory?.selectedSummary?.id == "variant-new")
    #expect(model.selectedSummarySelection == .smart)
  }

  @Test @MainActor
  func laterRawIntentSurvivesSuspendedRegenerationConfirmation() async {
    // Break caught: a late generation completion overriding a newer Raw reading choice.
    let initialStory = story(id: "story", title: "Signal")
    let confirmedStory = copy(initialStory, selectedVariantID: "variant-new")
    let initial = snapshot(todayStories: [initialStory], latest: [initialStory], saved: [])
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.generationResult = GenerationResult(
      story: confirmedStory,
      selectedSummary: variantNew,
      revision: mutationRevision
    )
    bridge.suspendNextGeneration()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "story"

    let generationTask = Task {
      await model.regenerateSelectedStory(profileID: "profile-1", force: false)
    }
    await eventually { bridge.generationRequests.count == 1 }
    model.showSummary(.raw)
    bridge.releaseGenerations()
    await generationTask.value

    #expect(model.selectedStory?.selectedSummary?.id == "variant-new")
    #expect(model.selectedSummarySelection == .raw)
  }
}

@MainActor
private func eventually(
  timeout: Duration = .seconds(1),
  condition: @MainActor () -> Bool
) async {
  let clock = ContinuousClock()
  let deadline = clock.now.advanced(by: timeout)
  while !condition(), clock.now < deadline {
    await Task.yield()
    try? await Task.sleep(for: .milliseconds(1))
  }
  #expect(condition())
}

private let referenceDate = Date(timeIntervalSince1970: 1_800_000_000)
private let mutationRevision = StateRevision(dataGeneration: 2, sourceConfigRevision: "source-a")

private let sourceFixtures = [
  Source(
    id: "source-1",
    name: "Signal Research",
    category: "research",
    enabled: true,
    weight: 0.9,
    feedURL: "https://example.test/feed",
    origin: .standard
  ),
  Source(
    id: "source-2",
    name: "AI Lab Notes",
    category: "labs",
    enabled: true,
    weight: 0.8,
    feedURL: "https://lab.test/feed",
    origin: .personal
  ),
]

private let variantOld = SummaryVariant(
  id: "variant-old",
  storyID: "story",
  profileID: "profile-1",
  provider: .openAI,
  model: "gpt-signal",
  dialect: .responses,
  fields: SummaryFields(
    whatHappened: "AI happened",
    whyItMatters: "AI matters",
    caveat: "AI caveat"
  ),
  generatedAt: referenceDate.addingTimeInterval(-100)
)

private let variantNew = SummaryVariant(
  id: "variant-new",
  storyID: "story",
  profileID: "profile-2",
  provider: .anthropic,
  model: "claude-signal",
  dialect: nil,
  fields: SummaryFields(
    whatHappened: "New happened",
    whyItMatters: "New matters",
    caveat: nil
  ),
  generatedAt: referenceDate
)

private func story(
  id: String,
  title: String,
  url: String? = nil,
  publishedAt: Date? = referenceDate,
  read: Bool = false,
  saved: Bool = false
) -> Story {
  Story(
    id: id,
    title: title,
    canonicalURL: url ?? "https://example.test/\(id)",
    excerpt: "Original excerpt",
    category: "research",
    publishedAt: publishedAt,
    sourceIDs: ["source-1", "source-2"],
    score: Score(recency: 0.9, sourceWeight: 0.8, corroboration: 0.7, total: 2.4),
    smartSummary: "Smart summary",
    isRead: read,
    isSaved: saved,
    selectedSummary: variantOld,
    summaryVariants: [variantOld, variantNew]
  )
}

private func copy(
  _ story: Story,
  read: Bool? = nil,
  saved: Bool? = nil,
  selectedVariantID: String? = nil
) -> Story {
  Story(
    id: story.id,
    title: story.title,
    canonicalURL: story.canonicalURL,
    excerpt: story.excerpt,
    category: story.category,
    publishedAt: story.publishedAt,
    sourceIDs: story.sourceIDs,
    score: story.score,
    smartSummary: story.smartSummary,
    isRead: read ?? story.isRead,
    isSaved: saved ?? story.isSaved,
    selectedSummary: selectedVariantID.flatMap { id in story.summaryVariants.first { $0.id == id } }
      ?? story.selectedSummary,
    summaryVariants: story.summaryVariants
  )
}

private func item(
  _ story: Story,
  position: UInt32,
  section: String,
  stale: Bool = false
) -> BriefingItem {
  BriefingItem(
    position: position,
    section: section,
    isStale: stale,
    story: story,
    selectedSummary: story.selectedSummary,
    summaryVariants: story.summaryVariants
  )
}

private func snapshot(
  revision: StateRevision = .fixture,
  todayStories: [Story],
  latest: [Story],
  saved: [Story],
  defaultProfileID: String? = "profile-1",
  modelProfiles: [ModelProfile] = [.fixture],
  sources: [Source] = sourceFixtures
) -> AppSnapshot {
  AppSnapshot(
    revision: revision,
    status: AppSnapshot.fixture.status,
    today: Briefing(
      date: "2026-08-30",
      generatedAt: referenceDate,
      isStale: false,
      items: todayStories.enumerated().map { index, story in
        item(story, position: UInt32(index + 1), section: "Top Signals")
      }
    ),
    latest: latest,
    saved: saved,
    sources: sources,
    modelProfiles: modelProfiles,
    defaultModelProfileID: defaultProfileID,
    hasUsableAIProfile: true
  )
}

private func modelProfile(id: String, enabled: Bool) -> ModelProfile {
  let fixture = ModelProfile.fixture
  return ModelProfile(
    id: id,
    name: id.capitalized,
    provider: fixture.provider,
    model: fixture.model,
    endpoint: fixture.endpoint,
    dialect: fixture.dialect,
    credentialSource: fixture.credentialSource,
    consentedAt: fixture.consentedAt,
    enabled: enabled,
    limits: fixture.limits,
    createdAt: fixture.createdAt,
    updatedAt: fixture.updatedAt
  )
}
