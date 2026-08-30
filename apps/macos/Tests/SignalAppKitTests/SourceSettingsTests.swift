import Foundation
import Testing

@testable import SignalAppKit

@Suite(.serialized)
struct SourceSettingsTests {
  @Test
  func sourceDraftTrimsLabelsButPreservesURLForCoreValidation() {
    // Break caught: Swift rewrites the feed address before Rust validates the authoritative input.
    let draft = SourceEditorDraft(
      name: "  Example feed  ",
      feedURL: " https://example.test/feed.xml?token=keep#fragment ",
      category: "  Research  ",
      weight: 0.75,
      enabled: true
    )

    #expect(
      draft.input
        == FeedSourceInput(
          name: "Example feed",
          category: "Research",
          url: " https://example.test/feed.xml?token=keep#fragment ",
          weight: 0.75,
          enabled: true
        )
    )
  }

  @Test(arguments: [Double.nan, -.leastNonzeroMagnitude, 1.000_000_1, .infinity])
  func sourceDraftRejectsNonfiniteOrOutOfRangeWeight(weight: Double) {
    // Break caught: invalid floating-point weights cross the Swift bridge unnecessarily.
    let draft = SourceEditorDraft(
      name: "Example",
      feedURL: "https://example.test/feed.xml",
      category: "Research",
      weight: weight,
      enabled: true
    )

    #expect(draft.input == nil)
    #expect(draft.validationMessage == "Enter a weight from 0 to 1.")
  }

  @Test(arguments: [0.0, 1.0])
  func sourceDraftAcceptsInclusiveWeightBounds(weight: Double) {
    // Break caught: excluding a boundary Rust explicitly accepts.
    let draft = SourceEditorDraft(
      name: "Example",
      feedURL: "https://example.test/feed.xml",
      category: "Research",
      weight: weight,
      enabled: true
    )

    #expect(draft.input?.weight == weight)
  }

  @Test
  func sourceEditorDisablesSavingDuringAnAdd() {
    // Break caught: a second Save can enqueue duplicate personal feeds while the first call is suspended.
    let draft = SourceEditorDraft(
      name: "Example",
      feedURL: "https://example.test/feed.xml",
      category: "Research",
      weight: 0.8,
      enabled: true
    )

    #expect(SourceEditorPresentation(draft: draft, isSaving: false).canSave)
    #expect(!SourceEditorPresentation(draft: draft, isSaving: true).canSave)
  }

  @Test
  func sourceRowsExposeOnlySafeHostAndPersonalRemoval() {
    // Break caught: showing secret-like URL material or offering Delete for bundled definitions.
    let standard = SourceRowPresentation(
      source: Source.fixture.with(
        feedURL: "https://reader:secret@example.test/feed.xml?token=hidden#private",
        origin: .standard
      )
    )
    let personal = SourceRowPresentation(
      source: Source.fixture.with(origin: .personal)
    )

    #expect(standard.displayHost == "example.test")
    #expect(standard.originLabel == "Standard source")
    #expect(!standard.canRemove)
    #expect(!standard.requiresRemovalConfirmation)
    #expect(personal.originLabel == "Personal source")
    #expect(personal.canRemove)
    #expect(personal.requiresRemovalConfirmation)
  }

  @Test @MainActor
  func suspendedAddUsesNormalizedInputAndRevisionAwareResult() async {
    // Break caught: dismissing/re-enabling the form before the bridge confirms the new source.
    let initial = sourceSnapshot(sources: [standardSource])
    let added = personalSource.with(name: "New feed")
    let revision = StateRevision(dataGeneration: 1, sourceConfigRevision: "source-b")
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.addSourceResult = SourceMutationResult(source: added, revision: revision)
    bridge.suspendNextAddSource()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    let input = FeedSourceInput(
      name: "New feed",
      category: "Research",
      url: " https://example.test/new.xml ",
      weight: 0.6,
      enabled: true
    )

    let task = Task { await model.addSource(input) }
    await eventually { bridge.addSourceInputs.count == 1 }

    #expect(model.sourceActionState(for: .adding) == .inFlight)
    #expect(model.snapshot == initial)
    #expect(bridge.addSourceInputs == [input])

    bridge.releaseAddSources()
    #expect(await task.value)
    #expect(model.sourceActionState(for: .adding) == nil)
    #expect(model.snapshot?.revision == revision)
    #expect(model.snapshot?.sources == [standardSource, added])
  }

  @Test @MainActor
  func suspendedToggleDisablesOnlyItsRowAndRollsBackOnFailure() async {
    // Break caught: optimistic state surviving a failed Rust write or freezing unrelated source rows.
    let initial = sourceSnapshot(sources: [standardSource, personalSource])
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.sourceError = BridgeError.storageUnavailable
    bridge.suspendNextSourceToggle()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()

    let task = Task { await model.setSourceEnabled(id: standardSource.id, enabled: false) }
    await eventually { bridge.sourceToggleRequests.count == 1 }

    #expect(model.sourceActionState(for: .toggling(sourceID: standardSource.id)) == .inFlight)
    #expect(model.sourceActionState(for: .toggling(sourceID: personalSource.id)) == nil)
    #expect(model.snapshot == initial)

    bridge.releaseSourceToggles()
    await task.value
    #expect(model.snapshot == initial)
    #expect(model.sourceActionError(for: standardSource.id) == "The source could not be updated.")
    #expect(model.phase == .ready)
  }

  @Test @MainActor
  func suspendedToggleRejectsASecondMutationForTheSameSource() async {
    // Break caught: queueing removal behind an in-flight toggle despite the affected row being disabled.
    let initial = sourceSnapshot(sources: [personalSource])
    let toggled = personalSource.with(enabled: false)
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.sourceToggleResult = SourceMutationResult(
      source: toggled,
      revision: StateRevision(dataGeneration: 1, sourceConfigRevision: "source-b")
    )
    bridge.suspendNextSourceToggle()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()

    let toggle = Task { await model.setSourceEnabled(id: personalSource.id, enabled: false) }
    await eventually { bridge.sourceToggleRequests.count == 1 }
    await model.removePersonalSource(id: personalSource.id)

    #expect(model.sourceActionState(for: .removing(sourceID: personalSource.id)) == nil)
    bridge.releaseSourceToggles()
    await toggle.value
    #expect(bridge.sourceRemovalRequests.isEmpty)
  }

  @Test @MainActor
  func suspendedRemovalFailureKeepsConfirmedPersonalSource() async {
    // Break caught: deleting the row optimistically and failing to restore it after Rust rejects removal.
    let initial = sourceSnapshot(sources: [standardSource, personalSource])
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.sourceError = BridgeError.storageUnavailable
    bridge.suspendNextSourceRemoval()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()

    let removal = Task { await model.removePersonalSource(id: personalSource.id) }
    await eventually { bridge.sourceRemovalRequests.count == 1 }
    #expect(model.snapshot == initial)
    #expect(model.sourceActionState(for: .removing(sourceID: personalSource.id)) == .inFlight)

    bridge.releaseSourceRemovals()
    await removal.value
    #expect(model.snapshot == initial)
    #expect(model.sourceActionError(for: personalSource.id) == "The source could not be updated.")
  }

  @Test @MainActor
  func failedAddKeepsTheEditorOpenWithSafeCopy() async {
    // Break caught: losing form input or exposing a bridge diagnostic when Rust rejects a feed.
    let initial = sourceSnapshot(sources: [standardSource])
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.sourceError = DetailedFakeError(description: "secret token in backend detail")
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.isSourceEditorPresented = true

    let succeeded = await model.addSource(
      FeedSourceInput(
        name: "Feed",
        category: "Research",
        url: "not validated by Swift",
        weight: 0.5,
        enabled: true
      )
    )

    #expect(!succeeded)
    #expect(model.isSourceEditorPresented)
    #expect(model.sourceEditorError == "The source could not be updated.")
    #expect(!model.sourceEditorError.orEmpty.contains("secret"))
    #expect(model.snapshot == initial)
  }

  @Test @MainActor
  func personalRemovalUsesConfirmationPolicyAndStandardRemovalNeverCallsBridge() async {
    // Break caught: bypassing personal-only deletion enforcement at the AppModel boundary.
    let initial = sourceSnapshot(sources: [standardSource, personalSource])
    let revision = StateRevision(dataGeneration: 1, sourceConfigRevision: "source-c")
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.removeSourceResult = SourceMutationResult(source: personalSource, revision: revision)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()

    await model.removePersonalSource(id: standardSource.id)
    #expect(bridge.sourceRemovalRequests.isEmpty)
    #expect(model.snapshot == initial)

    await model.removePersonalSource(id: personalSource.id)
    #expect(bridge.sourceRemovalRequests == [personalSource.id])
    #expect(model.snapshot?.sources == [standardSource])
    #expect(model.snapshot?.revision == revision)
  }

  @Test @MainActor
  func olderSourceMutationReconcilesThroughTheCoordinator() async {
    // Break caught: applying a source result whose database generation regresses the loaded snapshot.
    let initialRevision = StateRevision(dataGeneration: 5, sourceConfigRevision: "source-a")
    let initial = sourceSnapshot(revision: initialRevision, sources: [standardSource])
    let toggled = standardSource.with(enabled: false)
    let mutationRevision = StateRevision(dataGeneration: 4, sourceConfigRevision: "source-b")
    let reconciled = sourceSnapshot(
      revision: StateRevision(dataGeneration: 5, sourceConfigRevision: "source-b"),
      sources: [toggled]
    )
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.sourceToggleResult = SourceMutationResult(
      source: toggled,
      revision: mutationRevision
    )
    bridge.enqueueSnapshot(reconciled)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()

    await model.setSourceEnabled(id: standardSource.id, enabled: false)

    #expect(bridge.snapshotCalls == 2)
    #expect(model.snapshot == reconciled)
  }

  @Test @MainActor
  func sourceMutationWaitsForSuspendedPollingRead() async {
    // Break caught: overlapping revision polling with a local source configuration write.
    let initial = sourceSnapshot(sources: [standardSource])
    let toggled = standardSource.with(enabled: false)
    let bridge = FakeBridgeClient(
      snapshot: initial,
      revisions: [initial.revision]
    )
    bridge.sourceToggleResult = SourceMutationResult(
      source: toggled,
      revision: StateRevision(dataGeneration: 1, sourceConfigRevision: "source-b")
    )
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true),
      pollInterval: .milliseconds(1)
    )
    await model.start()
    bridge.suspendNextStateRevision()
    model.setActive(true)
    await eventually { bridge.stateRevisionCalls == 1 }

    let task = Task { await model.setSourceEnabled(id: standardSource.id, enabled: false) }
    await Task.yield()
    #expect(bridge.sourceToggleRequests.isEmpty)
    #expect(model.sourceActionState(for: .toggling(sourceID: standardSource.id)) == .queued)
    #expect(bridge.maximumConcurrentBridgeCalls == 1)

    bridge.releaseStateRevisions()
    await task.value
    model.stopPolling()
    #expect(bridge.sourceToggleRequests.count == 1)
    #expect(bridge.maximumConcurrentBridgeCalls == 1)
  }
}

extension Optional where Wrapped == String {
  fileprivate var orEmpty: String { self ?? "" }
}

private let standardSource = Source.fixture
private let personalSource = Source(
  id: "personal-1",
  name: "Personal feed",
  category: "Research",
  enabled: true,
  weight: 0.7,
  feedURL: "https://personal.example.test/feed.xml",
  origin: .personal
)

private func sourceSnapshot(
  revision: StateRevision = .fixture,
  sources: [Source]
) -> AppSnapshot {
  let fixture = AppSnapshot.fixture
  return AppSnapshot(
    revision: revision,
    status: fixture.status,
    today: fixture.today,
    latest: fixture.latest,
    saved: fixture.saved,
    sources: sources,
    modelProfiles: fixture.modelProfiles,
    defaultModelProfileID: fixture.defaultModelProfileID,
    hasUsableAIProfile: fixture.hasUsableAIProfile
  )
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
  }
}

extension Source {
  fileprivate func with(
    name: String? = nil,
    enabled: Bool? = nil,
    feedURL: String? = nil,
    origin: SourceOrigin? = nil
  ) -> Source {
    Source(
      id: id,
      name: name ?? self.name,
      category: category,
      enabled: enabled ?? self.enabled,
      weight: weight,
      feedURL: feedURL ?? self.feedURL,
      origin: origin ?? self.origin
    )
  }
}
