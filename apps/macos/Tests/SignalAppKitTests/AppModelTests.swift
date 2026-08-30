import Foundation
import Testing

@testable import SignalAppKit

@Suite(.serialized)
struct AppModelTests {
  @Test @MainActor
  func startShowsWelcomeForAnUninitializedFirstLaunch() async {
    // Break caught: treating a first launch as an ordinary empty briefing.
    let snapshot = AppSnapshot.fixture.with(
      status: CollectionStatus(state: .notInitialized, refresh: nil))
    let model = AppModel(
      bridge: FakeBridgeClient(snapshot: snapshot), preferences: MemoryAppPreferences())

    await model.start()

    #expect(model.snapshot == snapshot)
    #expect(model.phase == .welcome)
  }

  @Test @MainActor
  func welcomeCompletesWhenFirstRefreshIsOffline() async {
    // Break caught: reopening welcome forever after standard sources initialized but the network failed.
    let preferences = MemoryAppPreferences()
    let bridge = FakeBridgeClient(snapshot: .fixture)
    bridge.refreshError = BridgeError.offline
    let model = AppModel(bridge: bridge, preferences: preferences)

    await model.buildFirstBriefing()
    await eventually {
      model.phase == .offline(message: "The network is unavailable. Try again when you're online.")
    }

    #expect(preferences.welcomeCompleted)
  }

  @Test @MainActor
  func refreshIsSingleFlightAndCancellationUsesTheActiveIdentifier() async {
    // Break caught: starting overlapping bridge refreshes or cancelling with a newly generated identifier.
    let bridge = FakeBridgeClient(snapshot: .fixture)
    let model = AppModel(bridge: bridge, preferences: MemoryAppPreferences())

    await model.refresh()
    await model.refresh()
    await eventually { bridge.refreshIdentifiers.count == 1 }
    model.cancelRefresh()

    #expect(bridge.cancelledIdentifiers == bridge.refreshIdentifiers)
  }

  @Test @MainActor
  func completedRefreshReplacesTheWholeSnapshot() async {
    // Break caught: updating only the returned briefing and leaving source/model/story lists stale.
    let bridge = FakeBridgeClient(snapshot: .fixture)
    let replacement = AppSnapshot.fixture.with(
      revision: StateRevision(dataGeneration: 2, sourceConfigRevision: "source-b"),
      latest: [Story.fixture.with(title: "Replacement")]
    )
    bridge.enqueueSnapshot(replacement)
    let model = AppModel(bridge: bridge, preferences: MemoryAppPreferences(welcomeCompleted: true))
    await model.start()

    await model.refresh(ai: false)
    await eventually { bridge.refreshIdentifiers.count == 1 }
    bridge.finishRefresh(with: .fixture)
    await eventually { model.snapshot == replacement }

    #expect(model.phase == .ready)
  }

  @Test @MainActor
  func pollingUsesBothRevisionComponentsAndCoalescesReloads() async {
    // Break caught: comparing only SQLite generation or starting overlapping snapshot reloads.
    let initial = AppSnapshot.fixture
    let changedRevision = StateRevision(dataGeneration: 1, sourceConfigRevision: "source-b")
    let changed = initial.with(
      revision: changedRevision, sources: [Source.fixture.with(name: "Changed")])
    let bridge = FakeBridgeClient(
      snapshot: initial, revisions: [initial.revision, changedRevision, changedRevision])
    bridge.enqueueSnapshot(changed)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true),
      pollInterval: .milliseconds(5)
    )
    await model.start()

    model.setActive(true)
    await eventually { model.snapshot == changed }
    try? await Task.sleep(for: .milliseconds(25))
    model.setActive(false)

    #expect(bridge.snapshotCalls == 2)
  }

  @Test @MainActor
  func staleSnapshotProducesStalePhase() async {
    // Break caught: presenting carried or partially stale briefing content as current.
    let stale = AppSnapshot.fixture.with(today: AppSnapshot.fixture.today?.with(isStale: true))
    let model = AppModel(
      bridge: FakeBridgeClient(snapshot: stale),
      preferences: MemoryAppPreferences(welcomeCompleted: true))

    await model.start()

    #expect(model.phase == .stale)
  }

  @Test @MainActor
  func storageFailureIsBlockingRatherThanEmpty() async {
    // Break caught: converting an inaccessible database into a misleading empty briefing.
    let bridge = FakeBridgeClient(snapshot: .fixture)
    bridge.snapshotError = BridgeError.storageUnavailable
    let model = AppModel(bridge: bridge, preferences: MemoryAppPreferences(welcomeCompleted: true))

    await model.start()

    #expect(model.snapshot == nil)
    #expect(model.phase == .failure(message: "AI Daily Signal cannot access local storage."))
  }

  @Test @MainActor
  func snapshotReplacementRetainsOnlyAStillValidSelection() async {
    // Break caught: dropping a valid detail selection or retaining an identifier no longer present.
    let bridge = FakeBridgeClient(snapshot: .fixture)
    let retained = AppSnapshot.fixture.with(
      revision: StateRevision(dataGeneration: 2, sourceConfigRevision: "source-a"))
    let missing = retained.with(
      revision: StateRevision(dataGeneration: 3, sourceConfigRevision: "source-a"),
      today: .some(nil),
      latest: [],
      saved: []
    )
    bridge.enqueueSnapshot(retained)
    bridge.enqueueSnapshot(missing)
    let model = AppModel(bridge: bridge, preferences: MemoryAppPreferences(welcomeCompleted: true))
    await model.start()
    model.selectedStoryID = "story-1"

    await model.reloadSnapshot()
    #expect(model.selectedStoryID == "story-1")
    await model.reloadSnapshot()
    #expect(model.selectedStoryID == nil)
  }

  @Test @MainActor
  func modelSubmissionClearsViewOwnedSecretOnSuccessAndFailure() async {
    // Break caught: retaining a Keychain credential in view state after an awaited bridge call.
    let sentinel = "secret-sentinel-value"
    let bridge = FakeBridgeClient(snapshot: .fixture)
    let model = AppModel(bridge: bridge, preferences: MemoryAppPreferences(welcomeCompleted: true))
    var secret = sentinel
    let input = ModelProfileInput.fixture(secret: secret)

    _ = await model.addModel(input) { secret = "" }
    #expect(secret.isEmpty)
    #expect(bridge.modelInputs.first?.credential == .systemStore(secret: sentinel))

    secret = sentinel
    bridge.modelError = DetailedFakeError(description: "backend detail: \(sentinel)")
    _ = await model.addModel(input) { secret = "" }
    #expect(secret.isEmpty)
    #expect(!model.errorMessage.orEmpty.contains(sentinel))
  }

  @Test @MainActor
  func unknownBackendErrorsUseGenericRedactedCopy() async {
    // Break caught: interpolating arbitrary backend error descriptions into user-visible state.
    let sentinel = "private-backend-sentinel"
    let bridge = FakeBridgeClient(snapshot: .fixture)
    bridge.snapshotError = DetailedFakeError(description: sentinel)
    let model = AppModel(bridge: bridge, preferences: MemoryAppPreferences(welcomeCompleted: true))

    await model.start()

    #expect(model.errorMessage == "Something went wrong. Please try again.")
    #expect(!model.errorMessage.orEmpty.contains(sentinel))
  }

  @Test
  func invalidBridgeTimestampMapsToAnUnknownDate() {
    // Break caught: force-parsing malformed bridge timestamps and crashing the app.
    #expect(SignalFormatters.bridgeDate("not-a-date") == nil)
    #expect(SignalFormatters.bridgeDate("2026-08-30T12:00:00Z") != nil)
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

extension Optional where Wrapped == String {
  fileprivate var orEmpty: String { self ?? "" }
}

extension Story {
  fileprivate func with(title: String) -> Story {
    Story(
      id: id,
      title: title,
      canonicalURL: canonicalURL,
      excerpt: excerpt,
      category: category,
      publishedAt: publishedAt,
      sourceIDs: sourceIDs,
      score: score,
      smartSummary: smartSummary,
      isRead: isRead,
      isSaved: isSaved,
      selectedSummary: selectedSummary,
      summaryVariants: summaryVariants
    )
  }
}

extension Source {
  fileprivate func with(name: String) -> Source {
    Source(
      id: id, name: name, category: category, enabled: enabled, weight: weight, feedURL: feedURL,
      origin: origin)
  }
}

extension Briefing {
  fileprivate func with(isStale: Bool) -> Briefing {
    Briefing(date: date, generatedAt: generatedAt, isStale: isStale, items: items)
  }
}

extension AppSnapshot {
  fileprivate func with(
    revision: StateRevision? = nil,
    status: CollectionStatus? = nil,
    today: Briefing?? = nil,
    latest: [Story]? = nil,
    saved: [Story]? = nil,
    sources: [Source]? = nil
  ) -> AppSnapshot {
    AppSnapshot(
      revision: revision ?? self.revision,
      status: status ?? self.status,
      today: today ?? self.today,
      latest: latest ?? self.latest,
      saved: saved ?? self.saved,
      sources: sources ?? self.sources,
      modelProfiles: modelProfiles,
      defaultModelProfileID: defaultModelProfileID,
      hasUsableAIProfile: hasUsableAIProfile
    )
  }
}

extension ModelProfileInput {
  fileprivate static func fixture(secret: String) -> ModelProfileInput {
    ModelProfileInput(
      name: "Example",
      provider: .openAI,
      model: "model-1",
      endpoint: nil,
      dialect: .responses,
      credential: .systemStore(secret: secret),
      consentProviderDataSharing: true,
      limits: ProfileLimitsInput(
        maxSummariesPerRefresh: 5,
        maxDailyCostUSD: nil,
        inputCostUSDPerMillion: nil,
        outputCostUSDPerMillion: nil,
        maxOutputTokens: 384,
        timeoutSeconds: 30,
        maxRetries: 2
      )
    )
  }
}
