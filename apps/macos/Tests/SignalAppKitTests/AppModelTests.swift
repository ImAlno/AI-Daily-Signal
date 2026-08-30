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
  func firstAppLaunchShowsWelcomeEvenWhenTheCLIPrepopulatedToday() async {
    // Break caught: treating shared CLI data as proof that this app's welcome was completed.
    let model = AppModel(
      bridge: FakeBridgeClient(snapshot: .fixture),
      preferences: MemoryAppPreferences(welcomeCompleted: false)
    )

    await model.start()

    #expect(model.snapshot?.today != nil)
    #expect(model.phase == .welcome)
  }

  @Test @MainActor
  func firstBuildStaysOnWelcomeWhileItsRefreshIsRunning() async {
    // Break caught: replacing the first-build composition with the generic reading shell mid-action.
    let bridge = FakeBridgeClient(
      snapshot: AppSnapshot.fixture.with(
        status: CollectionStatus(state: .notInitialized, refresh: nil),
        today: .some(nil),
        latest: []
      )
    )
    bridge.suspendNextRefreshBeforeRegistration()
    let model = AppModel(bridge: bridge, preferences: MemoryAppPreferences())
    await model.start()

    await model.buildFirstBriefing()
    await eventually { model.phase == .buildingFirstBriefing }
    let presentation = WelcomePresentation(phase: model.phase)

    #expect(presentation.isPresented)
    #expect(presentation.showsProgress)
    #expect(!presentation.primaryActionEnabled)

    model.cancelRefresh()
    bridge.releaseRefreshStarts()
    await eventually { model.activeOperationID == nil }
  }

  @Test @MainActor
  func offlineFirstRefreshAfterStartupDoesNotClaimAPriorBriefing() async {
    // Break caught: treating a noninitialized snapshot as a successfully built prior briefing.
    let preferences = MemoryAppPreferences()
    let bridge = FakeBridgeClient(
      snapshot: AppSnapshot.fixture.with(
        status: CollectionStatus(state: .notInitialized, refresh: nil),
        today: .some(nil),
        latest: []
      )
    )
    bridge.refreshError = BridgeError.offline
    let model = AppModel(bridge: bridge, preferences: preferences)

    await model.start()
    await model.buildFirstBriefing()
    await eventually {
      model.phase == .offline(message: "The network is unavailable. Try again when you're online.")
    }

    #expect(preferences.welcomeCompleted)
  }

  @Test @MainActor
  func offlineRefreshWithAnExistingTodayPreservesTruthfulCopy() async {
    // Break caught: losing the preservation message when a real prior Today briefing exists.
    let bridge = FakeBridgeClient(snapshot: .fixture)
    bridge.refreshError = BridgeError.offline
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()

    await model.refresh()
    await eventually {
      model.phase
        == .offline(message: "The network is unavailable. Your last briefing is still here.")
    }
  }

  @Test @MainActor
  func refreshIsSingleFlightAndCancellationUsesTheActiveIdentifier() async {
    // Break caught: starting overlapping bridge refreshes or cancelling the wrong running identifier.
    let bridge = FakeBridgeClient(snapshot: .fixture)
    let model = AppModel(
      bridge: bridge, preferences: MemoryAppPreferences(welcomeCompleted: true))
    await model.start()

    await model.refresh()
    await eventually { bridge.refreshIdentifiers.count == 1 }
    await model.refresh()
    model.cancelRefresh()
    await eventually {
      bridge.refreshIdentifiers.count == 1
        && bridge.cancelledIdentifiers.count == 1
        && model.activeOperationID == nil
    }

    #expect(bridge.cancelledIdentifiers == bridge.refreshIdentifiers)
    #expect(model.phase == .ready)
  }

  @Test @MainActor
  func immediateCancellationBeforeRefreshTaskStartsNeverEntersBridge() async throws {
    // Break caught: relying on Rust cancellation registration instead of cancelling the local task.
    let bridge = FakeBridgeClient(snapshot: .fixture)
    bridge.discardCancellationBeforeRefreshRegistration()
    bridge.suspendNextRefreshBeforeRegistration()
    let model = AppModel(
      bridge: bridge, preferences: MemoryAppPreferences(welcomeCompleted: true))
    await model.start()

    await model.refresh()
    let operationID = try #require(model.activeOperationID)
    model.cancelRefresh()
    try? await Task.sleep(for: .milliseconds(20))

    #expect(bridge.refreshEntryCalls == 0)
    #expect(bridge.refreshIdentifiers.isEmpty)
    #expect(bridge.cancelledIdentifiers == [operationID])
    #expect(model.activeOperationID == nil)
    #expect(model.phase == .ready)

    if bridge.refreshEntryCalls > 0 {
      bridge.releaseRefreshStarts()
      await eventually { bridge.refreshIdentifiers.count == 1 }
      bridge.finishRefresh(with: .fixture)
      await eventually { model.activeOperationID == nil }
    }
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

    await model.pollRevisionWhileActive()
    await eventually { model.snapshot == changed }
    try? await Task.sleep(for: .milliseconds(25))
    model.setActive(false)

    #expect(bridge.snapshotCalls == 2)
  }

  @Test @MainActor
  func lateRevisionAfterStoppingPollingDoesNotReload() async {
    // Break caught: using a revision result after its polling task was cancelled.
    let initial = AppSnapshot.fixture
    let changedRevision = StateRevision(dataGeneration: 1, sourceConfigRevision: "source-b")
    let changed = initial.with(
      revision: changedRevision, sources: [Source.fixture.with(name: "Changed")])
    let bridge = FakeBridgeClient(snapshot: initial, revisions: [changedRevision])
    bridge.enqueueSnapshot(changed)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true),
      pollInterval: .milliseconds(1)
    )
    await model.start()
    bridge.suspendNextStateRevision()

    model.setActive(true)
    await eventually { bridge.stateRevisionCalls == 1 }
    model.setActive(false)
    bridge.releaseStateRevisions()
    try? await Task.sleep(for: .milliseconds(20))

    #expect(model.snapshot == initial)
    #expect(bridge.snapshotCalls == 1)
  }

  @Test @MainActor
  func refreshWaitsForAnInFlightPollingReload() async {
    // Break caught: overlapping refresh with a polling snapshot and then skipping its final snapshot.
    let initial = AppSnapshot.fixture
    let changedRevision = StateRevision(dataGeneration: 2, sourceConfigRevision: "source-a")
    let polled = initial.with(revision: changedRevision)
    let refreshed = initial.with(
      revision: StateRevision(dataGeneration: 3, sourceConfigRevision: "source-a"),
      latest: [Story.fixture.with(title: "Fresh result")]
    )
    let bridge = FakeBridgeClient(snapshot: initial, revisions: [changedRevision])
    bridge.enqueueSnapshot(polled)
    bridge.enqueueSnapshot(refreshed)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true),
      pollInterval: .milliseconds(1)
    )
    await model.start()
    bridge.suspendNextSnapshot()
    model.setActive(true)
    await eventually { bridge.snapshotCalls == 2 }

    await model.refresh()
    try? await Task.sleep(for: .milliseconds(20))
    #expect(bridge.refreshIdentifiers.isEmpty)
    #expect(bridge.maximumConcurrentBridgeCalls == 1)

    bridge.releaseSnapshots()
    await eventually { bridge.refreshIdentifiers.count == 1 }
    bridge.finishRefresh(with: .fixture)
    await eventually {
      model.snapshot == refreshed && model.activeOperationID == nil
    }
    model.setActive(false)

    #expect(bridge.maximumConcurrentBridgeCalls == 1)
  }

  @Test @MainActor
  func latePolledRevisionCannotReloadAfterRefreshStarts() async {
    // Break caught: starting a stale polling reload after a refresh has claimed the bridge.
    let initial = AppSnapshot.fixture
    let changedRevision = StateRevision(dataGeneration: 2, sourceConfigRevision: "source-a")
    let refreshed = initial.with(
      revision: StateRevision(dataGeneration: 3, sourceConfigRevision: "source-a"),
      latest: [Story.fixture.with(title: "Fresh result")]
    )
    let bridge = FakeBridgeClient(snapshot: initial, revisions: [changedRevision])
    bridge.enqueueSnapshot(refreshed)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true),
      pollInterval: .milliseconds(1)
    )
    await model.start()
    bridge.suspendNextStateRevision()
    model.setActive(true)
    await eventually { bridge.stateRevisionCalls == 1 }

    await model.refresh()
    await eventually { bridge.refreshIdentifiers.count == 1 }
    bridge.releaseStateRevisions()
    try? await Task.sleep(for: .milliseconds(20))
    #expect(bridge.snapshotCalls == 1)

    bridge.finishRefresh(with: .fixture)
    await eventually {
      model.snapshot == refreshed && model.activeOperationID == nil
    }
    model.setActive(false)
    #expect(bridge.snapshotCalls == 2)
  }

  @Test @MainActor
  func revisionStartedBeforeRefreshCannotReloadAfterRefreshFinishes() async {
    // Break caught: accepting a pre-refresh revision after bridge activity returns to idle.
    let initial = AppSnapshot.fixture
    let oldRevision = StateRevision(dataGeneration: 2, sourceConfigRevision: "source-a")
    let refreshed = initial.with(
      revision: StateRevision(dataGeneration: 3, sourceConfigRevision: "source-a"),
      latest: [Story.fixture.with(title: "Fresh result")]
    )
    let stale = initial.with(
      revision: oldRevision,
      latest: [Story.fixture.with(title: "Stale reload")]
    )
    let bridge = FakeBridgeClient(
      snapshot: initial, revisions: [oldRevision, refreshed.revision])
    bridge.enqueueSnapshot(refreshed)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true),
      pollInterval: .milliseconds(1)
    )
    await model.start()
    bridge.suspendNextStateRevision()
    model.setActive(true)
    await eventually { bridge.stateRevisionCalls == 1 }

    await model.refresh()
    await eventually { bridge.refreshIdentifiers.count == 1 }
    bridge.finishRefresh(with: .fixture)
    await eventually {
      model.snapshot == refreshed && model.activeOperationID == nil
    }
    bridge.enqueueSnapshot(stale)

    bridge.releaseStateRevisions()
    try? await Task.sleep(for: .milliseconds(20))
    model.stopPolling()

    #expect(model.snapshot == refreshed)
    #expect(bridge.snapshotCalls == 2)
  }

  @Test @MainActor
  func stoppingPollingReleasesModelAndIgnoresLateRevision() async {
    // Break caught: an owned polling task retaining its model across a suspended bridge call.
    let bridge = FakeBridgeClient(snapshot: .fixture)
    var model: AppModel? = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true),
      pollInterval: .milliseconds(1)
    )
    await model?.start()
    bridge.suspendNextStateRevision()
    model?.setActive(true)
    await eventually { bridge.stateRevisionCalls == 1 }

    model?.stopPolling()
    weak let weakModel = model
    model = nil
    await eventually { weakModel == nil }
    bridge.releaseStateRevisions()

    #expect(bridge.snapshotCalls == 1)
  }

  @Test @MainActor
  func droppingModelCancelsPendingRefreshAndReleasesIt() async {
    // Break caught: an owned refresh task retaining its model for an unbounded bridge call.
    let bridge = FakeBridgeClient(snapshot: .fixture)
    var model: AppModel? = AppModel(
      bridge: bridge, preferences: MemoryAppPreferences(welcomeCompleted: true))
    await model?.start()
    await model?.refresh()
    await eventually { bridge.refreshIdentifiers.count == 1 }

    weak let weakModel = model
    model = nil
    await eventually { weakModel == nil }
    if weakModel != nil, let operationID = bridge.refreshIdentifiers.first {
      _ = bridge.cancelOperation(id: operationID)
    }
    await eventually { weakModel == nil }

    #expect(bridge.cancelledIdentifiers == bridge.refreshIdentifiers)
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
