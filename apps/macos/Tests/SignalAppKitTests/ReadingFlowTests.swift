import Foundation
import Testing

@testable import SignalAppKit

@Suite(.serialized)
struct ReadingFlowTests {
  @Test
  func todayRenderPlanPreservesFirstSeenSectionAndStoryOrder() {
    // Break caught: alphabetizing editorial sections or sorting items by a derived field.
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

    #expect(presentation.sections.map(\.title) == ["Research", "Top Signals"])
    #expect(
      presentation.sections.map { $0.rows.map(\.storyID) } == [["first", "third"], ["second"]])
    #expect(presentation.sections[0].rows.map(\.rank) == [8, 4])
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
  }

  @Test
  func detailRenderPlanUsesApprovedHierarchyAndDoesNotInventStructuredSmartFields() {
    // Break caught: rearranging reading hierarchy or presenting Smart text as validated AI fields.
    let value = story(id: "story", title: "A signal")
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
  func pickerOrdersRawSmartThenImmutableAINewestFirst() {
    // Break caught: conflating Smart with AI or presenting cached variants in unstable order.
    let value = story(id: "story", title: "A signal")
    let picker = SummaryVariantPickerPresentation(story: value, selection: .smart)

    #expect(
      picker.options.map(\.selection) == [
        .raw, .smart, .ai(variantID: "variant-new"), .ai(variantID: "variant-old"),
      ])
    #expect(picker.options[1].provenance == .smart)
    #expect(picker.options[2].provenance == .ai(provider: "Anthropic", model: "claude-signal"))
    #expect(picker.options[3].provenance == .ai(provider: "OpenAI", model: "gpt-signal"))
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
      preferences: MemoryAppPreferences(welcomeCompleted: true)
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
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "story"

    await model.toggleSelectedStorySaved()

    #expect(model.snapshot?.saved.isEmpty == true)
    #expect(model.selectedStoryID == nil)
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
    #expect(model.storyActionError == "Choose a model profile before regenerating.")
    #expect(model.selectedSummarySelection == .ai(variantID: "variant-old"))
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
  publishedAt: Date? = referenceDate,
  read: Bool = false,
  saved: Bool = false
) -> Story {
  Story(
    id: id,
    title: title,
    canonicalURL: "https://example.test/\(id)",
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
  defaultProfileID: String? = "profile-1"
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
    sources: sourceFixtures,
    modelProfiles: [.fixture],
    defaultModelProfileID: defaultProfileID,
    hasUsableAIProfile: true
  )
}
