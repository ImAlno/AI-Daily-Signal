import Foundation
import Testing

@testable import SignalAppKit

@Suite(.serialized)
struct AlphaAcceptanceTests {
  @Test @MainActor
  func completePersonalAlphaFlowIsReachableThroughTheAppModel() async {
    // Break caught: shipping a surface whose welcome recovery, reading actions, source setup, or
    // explicitly confirmed model lifecycle cannot be completed through one app-model session.
    let initial = acceptanceSnapshot(
      generation: 0,
      sourceRevision: "source-a",
      status: CollectionStatus(state: .notInitialized, refresh: nil),
      story: nil,
      sources: [.fixture],
      profiles: []
    )
    let bridge = FakeBridgeClient(snapshot: initial)
    let preferences = MemoryAppPreferences()
    let model = AppModel(bridge: bridge, preferences: preferences)

    await model.start()
    #expect(model.phase == .welcome)

    bridge.refreshError = BridgeError.offline
    await model.buildFirstBriefing()
    await acceptanceEventually {
      model.phase == .offline(message: "The network is unavailable. Try again when you're online.")
    }
    #expect(preferences.welcomeCompleted)

    let populated = acceptanceSnapshot(
      generation: 1,
      sourceRevision: "source-a",
      status: CollectionStatus(
        state: .ready,
        refresh: RefreshMetadata(
          lastRefreshAt: Date(timeIntervalSince1970: 1_700_000_000), storyCount: 1)
      ),
      story: .fixture,
      sources: [.fixture],
      profiles: []
    )
    bridge.refreshError = nil
    bridge.enqueueSnapshot(populated)
    await model.refresh(ai: false)
    await acceptanceEventually { bridge.refreshIdentifiers.count == 2 }
    bridge.finishRefresh(with: .fixture)
    await acceptanceEventually { model.phase == .ready && model.snapshot == populated }

    var reached: Set<Destination> = []
    for destination in Destination.allCases {
      model.destination = destination
      reached.insert(model.destination)
    }
    #expect(reached == Set(Destination.allCases))

    model.destination = .today
    model.selectedStoryID = Story.fixture.id
    let savedStory = acceptanceStory(saved: true, read: false)
    bridge.savedResult = StoryMutationResult(
      story: savedStory,
      revision: StateRevision(dataGeneration: 2, sourceConfigRevision: "source-a")
    )
    await model.toggleSelectedStorySaved()
    #expect(model.selectedStory?.isSaved == true)
    #expect(model.snapshot?.saved.map(\.id) == [Story.fixture.id])

    let savedReadStory = acceptanceStory(saved: true, read: true)
    bridge.readResult = StoryMutationResult(
      story: savedReadStory,
      revision: StateRevision(dataGeneration: 3, sourceConfigRevision: "source-a")
    )
    await model.toggleSelectedStoryRead()
    #expect(model.selectedStory?.isRead == true)
    #expect(bridge.savedRequests.count == 1)
    #expect(bridge.readRequests.count == 1)

    let personalSource = Source(
      id: "personal-alpha",
      name: "Personal alpha feed",
      category: "research",
      enabled: true,
      weight: 0.7,
      feedURL: "https://personal.example.test/feed.xml",
      origin: .personal
    )
    bridge.addSourceResult = SourceMutationResult(
      source: personalSource,
      revision: StateRevision(dataGeneration: 3, sourceConfigRevision: "source-b")
    )
    #expect(
      await model.addSource(
        FeedSourceInput(
          name: personalSource.name,
          category: personalSource.category,
          url: personalSource.feedURL,
          weight: personalSource.weight,
          enabled: true
        ))
    )
    #expect(model.snapshot?.sources.contains(personalSource) == true)
    #expect(bridge.addSourceInputs.count == 1)

    let profile = ModelProfile.fixture
    let withProfile = acceptanceSnapshot(
      generation: 4,
      sourceRevision: "source-b",
      status: populated.status,
      story: savedReadStory,
      sources: [.fixture, personalSource],
      profiles: [profile]
    )
    bridge.addModelResult = ModelMutationResult(
      profile: profile,
      revision: withProfile.revision
    )
    bridge.enqueueSnapshot(withProfile)
    var transientSecret = "alpha-acceptance-secret"
    let input = acceptanceModelInput(secret: transientSecret)
    #expect(await model.addModel(input) { transientSecret = "" })
    #expect(transientSecret.isEmpty)
    #expect(bridge.addModelRequests.count == 1)

    let afterTest = acceptanceSnapshot(
      generation: 5,
      sourceRevision: "source-b",
      status: populated.status,
      story: savedReadStory,
      sources: [.fixture, personalSource],
      profiles: [profile]
    )
    bridge.modelTestResult = ModelTestResult(
      profile: profile,
      costMayApply: true,
      revision: afterTest.revision
    )
    bridge.enqueueSnapshot(afterTest)
    #expect(await model.testModel(id: profile.id, confirmedCost: true))
    #expect(bridge.testModelRequests == [profile.id])

    let afterRemoval = acceptanceSnapshot(
      generation: 6,
      sourceRevision: "source-b",
      status: populated.status,
      story: savedReadStory,
      sources: [.fixture, personalSource],
      profiles: []
    )
    bridge.modelRemovalResult = ModelRemovalResult(
      profile: profile,
      credentialDeletion: .deleted,
      revision: afterRemoval.revision
    )
    bridge.enqueueSnapshot(afterRemoval)
    #expect(await model.removeModel(id: profile.id, confirmed: true))
    #expect(bridge.removeModelRequests == [profile.id])
    #expect(model.snapshot?.modelProfiles.isEmpty == true)
  }
}

private func acceptanceStory(saved: Bool, read: Bool) -> Story {
  let story = Story.fixture
  return Story(
    id: story.id,
    title: story.title,
    canonicalURL: story.canonicalURL,
    excerpt: story.excerpt,
    category: story.category,
    publishedAt: story.publishedAt,
    sourceIDs: story.sourceIDs,
    score: story.score,
    smartSummary: story.smartSummary,
    isRead: read,
    isSaved: saved,
    selectedSummary: story.selectedSummary,
    summaryVariants: story.summaryVariants
  )
}

private func acceptanceSnapshot(
  generation: UInt64,
  sourceRevision: String,
  status: CollectionStatus,
  story: Story?,
  sources: [Source],
  profiles: [ModelProfile]
) -> AppSnapshot {
  let today = story.map {
    Briefing(
      date: "2026-08-30",
      generatedAt: Date(timeIntervalSince1970: 1_700_000_000),
      isStale: false,
      items: [
        BriefingItem(
          position: 1,
          section: "Top Signals",
          isStale: false,
          story: $0,
          selectedSummary: $0.selectedSummary,
          summaryVariants: $0.summaryVariants
        )
      ]
    )
  }
  return AppSnapshot(
    revision: StateRevision(
      dataGeneration: generation,
      sourceConfigRevision: sourceRevision
    ),
    status: status,
    today: today,
    latest: story.map { [$0] } ?? [],
    saved: story.map { $0.isSaved ? [$0] : [] } ?? [],
    sources: sources,
    modelProfiles: profiles,
    defaultModelProfileID: nil,
    hasUsableAIProfile: false
  )
}

private func acceptanceModelInput(secret: String) -> ModelProfileInput {
  ModelProfileInput(
    name: "Alpha acceptance",
    provider: .openAI,
    model: "opaque-alpha-model",
    endpoint: nil,
    dialect: .responses,
    credential: .systemStore(secret: secret),
    consentProviderDataSharing: true,
    limits: ProfileLimitsInput(
      maxSummariesPerRefresh: 5,
      maxDailyCostUSD: "0.25",
      inputCostUSDPerMillion: "1.00",
      outputCostUSDPerMillion: "4.00",
      maxOutputTokens: 384,
      timeoutSeconds: 30,
      maxRetries: 2
    )
  )
}

@MainActor
private func acceptanceEventually(
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
