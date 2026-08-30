import Foundation
import Observation

public enum AppPhase: Sendable, Equatable {
  case loading
  case welcome
  case buildingFirstBriefing
  case startupFailure(message: String)
  case empty
  case ready
  case refreshing
  case stale
  case offline(message: String)
  case failure(message: String)
}

public enum StoryAction: Sendable, Hashable {
  case saving(storyID: String)
  case markingRead(storyID: String)
  case selectingSummary(storyID: String)
  case regenerating(storyID: String)
}

public enum StoryActionState: Sendable, Equatable {
  case queued
  case inFlight
}

public enum SourceAction: Sendable, Hashable {
  case adding
  case toggling(sourceID: String)
  case removing(sourceID: String)
}

public enum SourceActionState: Sendable, Equatable {
  case queued
  case inFlight
}

public enum ModelAction: Sendable, Hashable {
  case adding
  case settingDefault(profileID: String)
  case testing(profileID: String)
  case removing(profileID: String)
}

public enum ModelActionState: Sendable, Equatable {
  case queued
  case inFlight
}

private enum ReloadOrigin: Equatable {
  case requested
  case polling(generation: UInt64)
}

private enum BridgeActivity: Equatable {
  case idle
  case reloading(token: UUID, origin: ReloadOrigin)
  case refreshing(operationID: String)
  case mutating(token: UUID, action: StoryAction)
  case sourceMutating(token: UUID, action: SourceAction)
  case modelMutating(token: UUID, action: ModelAction)
  case polling(token: UUID, generation: UInt64, revisionEpoch: UInt64)
}

private struct PendingRefresh: Equatable {
  let operationID: String
  let ai: Bool
}

private enum StoryMutationPayload: Sendable {
  case save(storyID: String, saved: Bool)
  case read(storyID: String, read: Bool)
  case select(storyID: String, variantID: String)
  case regenerate(storyID: String, profileID: String, force: Bool)
}

private struct PendingStoryMutation {
  let token: UUID
  let action: StoryAction
  let payload: StoryMutationPayload
  let summaryIntentEpoch: UInt64?
  let continuation: CheckedContinuation<Void, Never>
}

private struct StoryMutationConfirmation {
  let story: Story
  let revision: StateRevision
  let generatedSelectionID: String?
}

private enum SourceMutationPayload: Sendable {
  case add(FeedSourceInput)
  case toggle(id: String, enabled: Bool)
  case remove(id: String)
}

private struct PendingSourceMutation {
  let token: UUID
  let action: SourceAction
  let payload: SourceMutationPayload
  let continuation: CheckedContinuation<Bool, Never>
}

private struct SourceMutationConfirmation {
  let result: SourceMutationResult
  let removesSource: Bool
}

private struct PendingModelTurn {
  let token: UUID
  let action: ModelAction
  let continuation: CheckedContinuation<Void, Never>
}

@MainActor
@Observable
public final class AppModel {
  public private(set) var snapshot: AppSnapshot?
  public private(set) var phase: AppPhase = .loading
  public private(set) var activeOperationID: String?
  public private(set) var storyActionStates: [StoryAction: StoryActionState] = [:]
  public private(set) var sourceActionStates: [SourceAction: SourceActionState] = [:]
  public private(set) var modelActionStates: [ModelAction: ModelActionState] = [:]
  public private(set) var sourceEditorError: String?
  public private(set) var modelEditorError: String?
  public private(set) var credentialCleanupWarning: String?
  public private(set) var isSourceEditorPresented = false
  public private(set) var isModelEditorPresented = false
  public var destination: Destination {
    didSet {
      preferences.selectedDestination = destination
      validateSelectedStoryForDestination()
    }
  }
  public var selectedStoryID: String?

  @ObservationIgnored private let bridge: any BridgeClient
  @ObservationIgnored private let preferences: any AppPreferences
  @ObservationIgnored private let pollInterval: Duration
  @ObservationIgnored private var refreshTask: Task<Void, Never>?
  @ObservationIgnored private var pollTask: Task<Void, Never>?
  @ObservationIgnored private var pollBridgeTask: Task<Void, Never>?
  @ObservationIgnored private var reloadTask: Task<Void, Never>?
  @ObservationIgnored private var bridgeActivity = BridgeActivity.idle
  @ObservationIgnored private var pendingRefresh: PendingRefresh?
  @ObservationIgnored private var pendingReloadWaiters: [CheckedContinuation<Void, Never>] = []
  @ObservationIgnored private var pendingStoryMutations: [PendingStoryMutation] = []
  @ObservationIgnored private var pendingSourceMutations: [PendingSourceMutation] = []
  @ObservationIgnored private var pendingModelTurns: [PendingModelTurn] = []
  @ObservationIgnored private var isActive = false
  @ObservationIgnored private var pollGeneration: UInt64 = 0
  @ObservationIgnored private var revisionEpoch: UInt64 = 0
  private var summarySelections: [String: ReadingSummarySelection] = [:]
  private var summarySelectionIntentEpochs: [String: UInt64] = [:]
  private var storyActionErrors: [StoryAction: String] = [:]
  private var sourceActionErrors: [String: String] = [:]
  private var modelActionErrors: [String: String] = [:]

  public init(
    bridge: any BridgeClient,
    preferences: any AppPreferences,
    pollInterval: Duration = .seconds(2)
  ) {
    self.bridge = bridge
    self.preferences = preferences
    self.pollInterval = pollInterval
    destination = preferences.selectedDestination
  }

  isolated deinit {
    pollTask?.cancel()
    pollBridgeTask?.cancel()
    reloadTask?.cancel()
    refreshTask?.cancel()
    if let activeOperationID {
      _ = bridge.cancelOperation(id: activeOperationID)
    }
  }

  public var errorMessage: String? {
    switch phase {
    case .offline(let message), .startupFailure(let message), .failure(let message): message
    default: nil
    }
  }

  public var selectedStory: Story? {
    guard let selectedStoryID else { return nil }
    switch destination {
    case .today:
      return snapshot?.today?.items.first(where: { $0.story.id == selectedStoryID })?.story
    case .latest:
      return snapshot?.latest.first(where: { $0.id == selectedStoryID })
    case .saved:
      return snapshot?.saved.first(where: { $0.id == selectedStoryID })
    case .sources, .settings:
      return nil
    }
  }

  public var activeStoryAction: StoryAction? {
    storyActionStates.first(where: { $0.value == .inFlight })?.key
  }

  public var storyActionError: String? {
    guard let selectedStoryID else { return nil }
    return storyActionErrors.first(where: { actionStoryID($0.key) == selectedStoryID })?.value
  }

  public func storyActionState(for action: StoryAction) -> StoryActionState? {
    storyActionStates[action]
  }

  public func storyActionError(for action: StoryAction) -> String? {
    storyActionErrors[action]
  }

  public func sourceActionState(for action: SourceAction) -> SourceActionState? {
    sourceActionStates[action]
  }

  public func sourceActionError(for sourceID: String) -> String? {
    sourceActionErrors[sourceID]
  }

  public func modelActionState(for action: ModelAction) -> ModelActionState? {
    modelActionStates[action]
  }

  public func modelActionError(for profileID: String) -> String? {
    modelActionErrors[profileID]
  }

  public func presentSourceEditor() {
    sourceEditorError = nil
    isSourceEditorPresented = true
  }

  public func dismissSourceEditor() {
    sourceEditorError = nil
    isSourceEditorPresented = false
  }

  public func presentModelEditor() {
    modelEditorError = nil
    isModelEditorPresented = true
  }

  public func dismissModelEditor() {
    modelEditorError = nil
    isModelEditorPresented = false
  }

  public func dismissCredentialCleanupWarning() {
    credentialCleanupWarning = nil
  }

  public var selectedSummarySelection: ReadingSummarySelection? {
    guard let selectedStoryID else { return nil }
    return summarySelection(for: selectedStoryID)
  }

  public func story(id: String) -> Story? {
    if let story = snapshot?.today?.items.first(where: { $0.story.id == id })?.story {
      return story
    }
    return snapshot?.latest.first(where: { $0.id == id })
      ?? snapshot?.saved.first(where: { $0.id == id })
  }

  public func isStoryStale(id: String) -> Bool {
    guard let today = snapshot?.today else { return false }
    return today.isStale || today.items.first(where: { $0.story.id == id })?.isStale == true
  }

  public func summarySelection(for storyID: String) -> ReadingSummarySelection {
    if let selection = summarySelections[storyID], isValid(selection, for: story(id: storyID)) {
      return selection
    }
    if let variantID = story(id: storyID)?.selectedSummary?.id {
      return .ai(variantID: variantID)
    }
    return .smart
  }

  public func showSummary(
    _ selection: ReadingSummarySelection,
    for storyID: String? = nil
  ) {
    guard let storyID = storyID ?? selectedStoryID, story(id: storyID) != nil else { return }
    switch selection {
    case .raw, .smart:
      _ = advanceSummarySelectionIntent(for: storyID)
      summarySelections[storyID] = selection
      storyActionErrors = storyActionErrors.filter { actionStoryID($0.key) != storyID }
    case .ai:
      break
    }
  }

  public func start() async {
    phase = .loading
    await reloadSnapshot()
  }

  public func buildFirstBriefing() async {
    preferences.welcomeCompleted = true
    await requestRefresh(ai: true, startingPhase: .buildingFirstBriefing)
  }

  public func refresh(ai: Bool = true) async {
    await requestRefresh(ai: ai, startingPhase: .refreshing)
  }

  private func requestRefresh(ai: Bool, startingPhase: AppPhase) async {
    guard activeOperationID == nil else { return }
    invalidatePendingRevisionReads()
    let operationID = UUID().uuidString
    activeOperationID = operationID
    phase = startingPhase
    let request = PendingRefresh(operationID: operationID, ai: ai)
    switch bridgeActivity {
    case .idle:
      beginRefresh(request)
    case .reloading, .mutating, .sourceMutating, .modelMutating, .polling:
      pendingRefresh = request
    case .refreshing:
      assertionFailure("An active refresh must own activeOperationID")
    }
  }

  public func cancelRefresh() {
    guard let activeOperationID else { return }
    refreshTask?.cancel()
    _ = bridge.cancelOperation(id: activeOperationID)
    if pendingRefresh?.operationID == activeOperationID {
      pendingRefresh = nil
      self.activeOperationID = nil
      invalidatePendingRevisionReads()
      phase = snapshot.map(presentationPhase(for:)) ?? .empty
      return
    }
  }

  public func toggleSelectedStorySaved() async {
    guard let selectedStory else { return }
    let action = StoryAction.saving(storyID: selectedStory.id)
    guard storyActionStates[action] == nil else { return }
    await enqueueStoryMutation(
      action: action,
      payload: .save(storyID: selectedStory.id, saved: !selectedStory.isSaved)
    )
  }

  public func saveSelectedStory() async {
    guard let selectedStory, !selectedStory.isSaved else { return }
    let action = StoryAction.saving(storyID: selectedStory.id)
    guard storyActionStates[action] == nil else { return }
    await enqueueStoryMutation(
      action: action,
      payload: .save(storyID: selectedStory.id, saved: true)
    )
  }

  public func toggleSelectedStoryRead() async {
    guard let selectedStory else { return }
    let action = StoryAction.markingRead(storyID: selectedStory.id)
    guard storyActionStates[action] == nil else { return }
    await enqueueStoryMutation(
      action: action,
      payload: .read(storyID: selectedStory.id, read: !selectedStory.isRead)
    )
  }

  public func selectSummary(
    _ selection: ReadingSummarySelection,
    for storyID: String? = nil
  ) async {
    guard let storyID = storyID ?? selectedStoryID else { return }
    switch selection {
    case .raw, .smart:
      showSummary(selection, for: storyID)
    case .ai(let variantID):
      guard let current = story(id: storyID),
        current.summaryVariants.contains(where: { $0.id == variantID })
      else { return }
      let action = StoryAction.selectingSummary(storyID: storyID)
      guard storyActionStates[action] == nil else { return }
      let intentEpoch = advanceSummarySelectionIntent(for: storyID)
      await enqueueStoryMutation(
        action: action,
        payload: .select(storyID: storyID, variantID: variantID),
        summaryIntentEpoch: intentEpoch
      )
    }
  }

  public func regenerateSelectedStory(profileID: String?, force: Bool) async {
    guard let selectedStory else { return }
    let resolvedProfile = profileID ?? snapshot?.defaultModelProfileID
    let enabledProfileIDs = Set(snapshot?.modelProfiles.filter(\.enabled).map(\.id) ?? [])
    let action = StoryAction.regenerating(storyID: selectedStory.id)
    guard let resolvedProfile, enabledProfileIDs.contains(resolvedProfile) else {
      storyActionErrors[action] = "Choose an enabled model profile before regenerating."
      return
    }
    guard storyActionStates[action] == nil else { return }
    let intentEpoch = advanceSummarySelectionIntent(for: selectedStory.id)
    await enqueueStoryMutation(
      action: action,
      payload: .regenerate(
        storyID: selectedStory.id,
        profileID: resolvedProfile,
        force: force
      ),
      summaryIntentEpoch: intentEpoch
    )
  }

  private func enqueueStoryMutation(
    action: StoryAction,
    payload: StoryMutationPayload,
    summaryIntentEpoch: UInt64? = nil
  ) async {
    invalidatePendingRevisionReads()
    storyActionErrors[action] = nil
    storyActionStates[action] = .queued
    await withCheckedContinuation { continuation in
      pendingStoryMutations.append(
        PendingStoryMutation(
          token: UUID(),
          action: action,
          payload: payload,
          summaryIntentEpoch: summaryIntentEpoch,
          continuation: continuation
        )
      )
      beginNextBridgeActivityIfPossible()
    }
  }

  private func beginStoryMutation(_ pending: PendingStoryMutation) {
    guard bridgeActivity == .idle else { return }
    invalidatePendingRevisionReads()
    storyActionStates[pending.action] = .inFlight
    bridgeActivity = .mutating(token: pending.token, action: pending.action)
    let bridge = bridge
    Task { [weak self, bridge] in
      do {
        let confirmation: StoryMutationConfirmation
        switch pending.payload {
        case .save(let storyID, let saved):
          let result = try await bridge.setSaved(storyID: storyID, saved: saved)
          confirmation = StoryMutationConfirmation(
            story: result.story,
            revision: result.revision,
            generatedSelectionID: nil
          )
        case .read(let storyID, let read):
          let result = try await bridge.setRead(storyID: storyID, read: read)
          confirmation = StoryMutationConfirmation(
            story: result.story,
            revision: result.revision,
            generatedSelectionID: nil
          )
        case .select(let storyID, let variantID):
          let result = try await bridge.selectSummary(storyID: storyID, variantID: variantID)
          confirmation = StoryMutationConfirmation(
            story: result.story,
            revision: result.revision,
            generatedSelectionID: result.story.selectedSummary?.id == variantID ? variantID : nil
          )
        case .regenerate(let storyID, let profileID, let force):
          let result = try await bridge.regenerate(
            storyID: storyID,
            profile: profileID,
            force: force
          )
          confirmation = StoryMutationConfirmation(
            story: result.story,
            revision: result.revision,
            generatedSelectionID: result.selectedSummary?.id
          )
        }
        let requiresReconciliation =
          self?.storyMutationRequiresReconciliation(confirmation) == true
        if requiresReconciliation {
          let replacement = try await bridge.snapshot()
          self?.finishStoryMutation(
            pending,
            confirmation: confirmation,
            reconciliationSnapshot: replacement,
            requiresReconciliation: true,
            error: nil
          )
        } else {
          self?.finishStoryMutation(
            pending,
            confirmation: confirmation,
            reconciliationSnapshot: nil,
            requiresReconciliation: false,
            error: nil
          )
        }
      } catch {
        self?.finishStoryMutation(
          pending,
          confirmation: nil,
          reconciliationSnapshot: nil,
          requiresReconciliation: false,
          error: error
        )
      }
    }
  }

  private func finishStoryMutation(
    _ pending: PendingStoryMutation,
    confirmation: StoryMutationConfirmation?,
    reconciliationSnapshot: AppSnapshot?,
    requiresReconciliation: Bool,
    error: (any Error)?
  ) {
    guard bridgeActivity == .mutating(token: pending.token, action: pending.action) else { return }
    invalidatePendingRevisionReads()
    var appliedConfirmation: StoryMutationConfirmation?
    if requiresReconciliation, let reconciliationSnapshot {
      replaceSnapshot(reconciliationSnapshot)
      if snapshot?.revision == reconciliationSnapshot.revision {
        appliedConfirmation = confirmation
      }
    } else if let confirmation,
      confirmation.revision.dataGeneration >= (snapshot?.revision.dataGeneration ?? 0)
    {
      replaceStory(confirmation.story, revision: confirmation.revision)
      appliedConfirmation = confirmation
    } else if let error {
      storyActionErrors[pending.action] = userFacingMessage(for: error)
    }
    if let confirmation = appliedConfirmation,
      let variantID = confirmation.generatedSelectionID,
      pending.summaryIntentEpoch == summarySelectionIntentEpochs[confirmation.story.id],
      story(id: confirmation.story.id)?.summaryVariants.contains(where: { $0.id == variantID })
        == true,
      !requiresReconciliation || story(id: confirmation.story.id)?.selectedSummary?.id == variantID
    {
      summarySelections[confirmation.story.id] = .ai(variantID: variantID)
    }
    storyActionStates[pending.action] = nil
    bridgeActivity = .idle
    pending.continuation.resume()
    beginNextBridgeActivityIfPossible()
  }

  private func storyMutationRequiresReconciliation(
    _ confirmation: StoryMutationConfirmation
  ) -> Bool {
    guard let revision = snapshot?.revision else { return false }
    return confirmation.revision.dataGeneration < revision.dataGeneration
      || confirmation.revision.sourceConfigRevision != revision.sourceConfigRevision
  }

  private func beginNextBridgeActivityIfPossible() {
    guard bridgeActivity == .idle else { return }
    if let pendingRefresh {
      self.pendingRefresh = nil
      beginRefresh(pendingRefresh)
    } else if !pendingStoryMutations.isEmpty {
      beginStoryMutation(pendingStoryMutations.removeFirst())
    } else if !pendingSourceMutations.isEmpty {
      beginSourceMutation(pendingSourceMutations.removeFirst())
    } else if !pendingModelTurns.isEmpty {
      beginModelTurn(pendingModelTurns.removeFirst())
    } else if !pendingReloadWaiters.isEmpty {
      beginReload(origin: .requested)
    }
  }

  public func setActive(_ active: Bool) {
    if active {
      startPolling()
    } else {
      stopPolling()
    }
  }

  public func stopPolling() {
    stopPollingOwnedTasks()
  }

  public func pollRevisionWhileActive() async {
    startPolling()
    await Task.yield()
  }

  public func reloadSnapshot() async {
    switch bridgeActivity {
    case .idle:
      beginReload(origin: .requested)
      await reloadTask?.value
    case .reloading:
      await reloadTask?.value
    case .refreshing:
      await refreshTask?.value
    case .mutating, .sourceMutating, .modelMutating, .polling:
      await withCheckedContinuation { continuation in
        pendingReloadWaiters.append(continuation)
      }
    }
  }

  @discardableResult
  public func addSource(_ input: FeedSourceInput) async -> Bool {
    guard sourceActionStates[.adding] == nil else { return false }
    return await enqueueSourceMutation(action: .adding, payload: .add(input))
  }

  public func setSourceEnabled(id: String, enabled: Bool) async {
    guard snapshot?.sources.contains(where: { $0.id == id }) == true else { return }
    guard !isSourceMutationPending(for: id) else { return }
    let action = SourceAction.toggling(sourceID: id)
    _ = await enqueueSourceMutation(
      action: action,
      payload: .toggle(id: id, enabled: enabled)
    )
  }

  public func removePersonalSource(id: String) async {
    guard snapshot?.sources.first(where: { $0.id == id })?.origin == .personal else { return }
    guard !isSourceMutationPending(for: id) else { return }
    let action = SourceAction.removing(sourceID: id)
    _ = await enqueueSourceMutation(action: action, payload: .remove(id: id))
  }

  private func isSourceMutationPending(for sourceID: String) -> Bool {
    sourceActionStates[.toggling(sourceID: sourceID)] != nil
      || sourceActionStates[.removing(sourceID: sourceID)] != nil
  }

  private func enqueueSourceMutation(
    action: SourceAction,
    payload: SourceMutationPayload
  ) async -> Bool {
    invalidatePendingRevisionReads()
    if let sourceID = sourceID(for: action) {
      sourceActionErrors[sourceID] = nil
    } else {
      sourceEditorError = nil
    }
    sourceActionStates[action] = .queued
    return await withCheckedContinuation { continuation in
      pendingSourceMutations.append(
        PendingSourceMutation(
          token: UUID(),
          action: action,
          payload: payload,
          continuation: continuation
        )
      )
      beginNextBridgeActivityIfPossible()
    }
  }

  private func beginSourceMutation(_ pending: PendingSourceMutation) {
    guard bridgeActivity == .idle else { return }
    invalidatePendingRevisionReads()
    sourceActionStates[pending.action] = .inFlight
    bridgeActivity = .sourceMutating(token: pending.token, action: pending.action)
    let bridge = bridge
    Task { [weak self, bridge] in
      do {
        let confirmation: SourceMutationConfirmation
        switch pending.payload {
        case .add(let input):
          confirmation = SourceMutationConfirmation(
            result: try await bridge.addSource(input),
            removesSource: false
          )
        case .toggle(let id, let enabled):
          confirmation = SourceMutationConfirmation(
            result: try await bridge.setSourceEnabled(id: id, enabled: enabled),
            removesSource: false
          )
        case .remove(let id):
          confirmation = SourceMutationConfirmation(
            result: try await bridge.removeSource(id: id),
            removesSource: true
          )
        }
        let requiresReconciliation =
          self?.snapshot.map {
            confirmation.result.revision.dataGeneration
              != $0.revision.dataGeneration
          } ?? false
        if requiresReconciliation {
          let replacement = try await bridge.snapshot()
          self?.finishSourceMutation(
            pending,
            confirmation: confirmation,
            reconciliationSnapshot: replacement,
            error: nil
          )
        } else {
          self?.finishSourceMutation(
            pending,
            confirmation: confirmation,
            reconciliationSnapshot: nil,
            error: nil
          )
        }
      } catch {
        self?.finishSourceMutation(
          pending,
          confirmation: nil,
          reconciliationSnapshot: nil,
          error: error
        )
      }
    }
  }

  private func finishSourceMutation(
    _ pending: PendingSourceMutation,
    confirmation: SourceMutationConfirmation?,
    reconciliationSnapshot: AppSnapshot?,
    error: (any Error)?
  ) {
    guard bridgeActivity == .sourceMutating(token: pending.token, action: pending.action) else {
      return
    }
    invalidatePendingRevisionReads()
    var succeeded = false
    if let reconciliationSnapshot {
      replaceSnapshot(reconciliationSnapshot)
      succeeded = snapshot?.revision == reconciliationSnapshot.revision
    } else if let confirmation {
      replaceSource(
        confirmation.result.source,
        revision: confirmation.result.revision,
        removing: confirmation.removesSource
      )
      succeeded = snapshot?.revision == confirmation.result.revision
    } else if error != nil {
      if let sourceID = sourceID(for: pending.action) {
        sourceActionErrors[sourceID] = "The source could not be updated."
      } else {
        sourceEditorError = "The source could not be updated."
      }
    }
    sourceActionStates[pending.action] = nil
    bridgeActivity = .idle
    pending.continuation.resume(returning: succeeded)
    beginNextBridgeActivityIfPossible()
  }

  private func sourceID(for action: SourceAction) -> String? {
    switch action {
    case .adding: nil
    case .toggling(let sourceID), .removing(let sourceID): sourceID
    }
  }

  @discardableResult
  public func addModel(
    _ input: ModelProfileInput,
    clearSecret: @MainActor () -> Void
  ) async -> Bool {
    defer { clearSecret() }
    let action = ModelAction.adding
    guard let token = await acquireModelTurn(action) else { return false }
    do {
      let result = try await bridge.addModel(input)
      let replacement = try await bridge.snapshot()
      return finishModelTurn(
        token: token,
        action: action,
        mutationRevision: result.revision,
        replacement: replacement,
        credentialDeletion: nil,
        error: nil
      )
    } catch {
      return finishModelTurn(
        token: token,
        action: action,
        mutationRevision: nil,
        replacement: nil,
        credentialDeletion: nil,
        error: error
      )
    }
  }

  @discardableResult
  public func setDefaultModel(id: String) async -> Bool {
    guard let profile = snapshot?.modelProfiles.first(where: { $0.id == id }),
      profile.enabled,
      profile.consentedAt != nil,
      !isModelMutationPending(for: id)
    else { return false }
    let action = ModelAction.settingDefault(profileID: id)
    guard let token = await acquireModelTurn(action) else { return false }
    do {
      let result = try await bridge.setDefaultModel(id)
      let replacement = try await bridge.snapshot()
      return finishModelTurn(
        token: token,
        action: action,
        mutationRevision: result.revision,
        replacement: replacement,
        credentialDeletion: nil,
        error: nil
      )
    } catch {
      return finishModelTurn(
        token: token,
        action: action,
        mutationRevision: nil,
        replacement: nil,
        credentialDeletion: nil,
        error: error
      )
    }
  }

  @discardableResult
  public func testModel(id: String, confirmedCost: Bool) async -> Bool {
    guard confirmedCost,
      let profile = snapshot?.modelProfiles.first(where: { $0.id == id }),
      profile.enabled,
      profile.consentedAt != nil,
      !isModelMutationPending(for: id)
    else { return false }
    let action = ModelAction.testing(profileID: id)
    guard let token = await acquireModelTurn(action) else { return false }
    do {
      let result = try await bridge.testModel(id)
      let replacement = try await bridge.snapshot()
      return finishModelTurn(
        token: token,
        action: action,
        mutationRevision: result.revision,
        replacement: replacement,
        credentialDeletion: nil,
        error: nil
      )
    } catch {
      return finishModelTurn(
        token: token,
        action: action,
        mutationRevision: nil,
        replacement: nil,
        credentialDeletion: nil,
        error: error
      )
    }
  }

  @discardableResult
  public func removeModel(id: String, confirmed: Bool) async -> Bool {
    guard confirmed,
      snapshot?.modelProfiles.contains(where: { $0.id == id }) == true,
      !isModelMutationPending(for: id)
    else { return false }
    let action = ModelAction.removing(profileID: id)
    guard let token = await acquireModelTurn(action) else { return false }
    do {
      let result = try await bridge.removeModel(id)
      let replacement = try await bridge.snapshot()
      return finishModelTurn(
        token: token,
        action: action,
        mutationRevision: result.revision,
        replacement: replacement,
        credentialDeletion: result.credentialDeletion,
        error: nil
      )
    } catch {
      return finishModelTurn(
        token: token,
        action: action,
        mutationRevision: nil,
        replacement: nil,
        credentialDeletion: nil,
        error: error
      )
    }
  }

  private func acquireModelTurn(_ action: ModelAction) async -> UUID? {
    guard modelActionStates[action] == nil else { return nil }
    invalidatePendingRevisionReads()
    credentialCleanupWarning = nil
    if let profileID = modelProfileID(for: action) {
      modelActionErrors[profileID] = nil
    } else {
      modelEditorError = nil
    }
    let token = UUID()
    modelActionStates[action] = .queued
    await withCheckedContinuation { continuation in
      pendingModelTurns.append(
        PendingModelTurn(token: token, action: action, continuation: continuation)
      )
      beginNextBridgeActivityIfPossible()
    }
    return token
  }

  private func beginModelTurn(_ pending: PendingModelTurn) {
    guard bridgeActivity == .idle else { return }
    invalidatePendingRevisionReads()
    modelActionStates[pending.action] = .inFlight
    bridgeActivity = .modelMutating(token: pending.token, action: pending.action)
    pending.continuation.resume()
  }

  private func finishModelTurn(
    token: UUID,
    action: ModelAction,
    mutationRevision: StateRevision?,
    replacement: AppSnapshot?,
    credentialDeletion: CredentialDeletionStatus?,
    error: (any Error)?
  ) -> Bool {
    guard bridgeActivity == .modelMutating(token: token, action: action) else { return false }
    invalidatePendingRevisionReads()
    var succeeded = false
    if let mutationRevision, let replacement,
      replacement.revision.dataGeneration >= mutationRevision.dataGeneration
    {
      replaceSnapshot(replacement)
      succeeded = snapshot?.revision == replacement.revision
      if succeeded, credentialDeletion == .deleteFailed {
        credentialCleanupWarning = ModelSettingsCopy.credentialCleanupWarning
      }
    } else if mutationRevision != nil || replacement != nil {
      recordModelActionError(
        "The model state changed before it could be confirmed. Reload and try again.",
        for: action
      )
    } else if let error {
      recordModelActionError(modelActionMessage(for: action, error: error), for: action)
    }
    modelActionStates[action] = nil
    bridgeActivity = .idle
    beginNextBridgeActivityIfPossible()
    return succeeded
  }

  private func isModelMutationPending(for profileID: String) -> Bool {
    modelActionStates.keys.contains { modelProfileID(for: $0) == profileID }
  }

  private func recordModelActionError(_ message: String, for action: ModelAction) {
    if let profileID = modelProfileID(for: action) {
      modelActionErrors[profileID] = message
    } else {
      modelEditorError = message
    }
  }

  private func modelProfileID(for action: ModelAction) -> String? {
    switch action {
    case .adding: nil
    case .settingDefault(let profileID), .testing(let profileID), .removing(let profileID):
      profileID
    }
  }

  private func modelActionMessage(for action: ModelAction, error: any Error) -> String {
    if case .testing = action, let bridgeError = error as? BridgeError {
      switch bridgeError {
      case .credentialUnavailable:
        return
          "The model credential is unavailable. Check its Keychain entry or environment variable."
      case .consentRequired:
        return "Provider data-sharing consent is required before testing this model."
      case .budgetExhausted:
        return "The model test was stopped by the configured budget."
      case .providerUnavailable, .offline:
        return "The provider could not complete the model test."
      case .cancelled:
        return "The model test was cancelled."
      default:
        break
      }
    }
    switch action {
    case .adding: return "The model profile could not be added."
    case .settingDefault: return "The default model could not be changed."
    case .testing: return "The model test could not be completed."
    case .removing: return "The model profile could not be removed."
    }
  }

  private func startPolling() {
    guard !isActive else { return }
    invalidatePendingRevisionReads()
    isActive = true
    pollGeneration &+= 1
    let generation = pollGeneration
    let interval = pollInterval
    pollTask = Task { [weak self] in
      while !Task.isCancelled {
        do {
          try await Task.sleep(for: interval)
        } catch is CancellationError {
          return
        } catch {
          guard !Task.isCancelled else { return }
          self?.apply(error)
          continue
        }
        guard !Task.isCancelled else { return }
        self?.beginPollingCycle(generation: generation)
      }
    }
  }

  private func stopPollingOwnedTasks() {
    guard isActive || pollTask != nil else { return }
    invalidatePendingRevisionReads()
    isActive = false
    pollGeneration &+= 1
    pollTask?.cancel()
    pollTask = nil
    if case .polling = bridgeActivity {
      pollBridgeTask?.cancel()
    }
  }

  private func beginPollingCycle(generation: UInt64) {
    guard isActive,
      generation == pollGeneration,
      activeOperationID == nil,
      bridgeActivity == .idle
    else { return }
    let token = UUID()
    let epoch = revisionEpoch
    bridgeActivity = .polling(token: token, generation: generation, revisionEpoch: epoch)
    let bridge = bridge
    pollBridgeTask = Task { [weak self, bridge] in
      do {
        let revision = try await bridge.stateRevision()
        guard
          self?.pollingCycleMayContinue(
            token: token,
            generation: generation,
            revisionEpoch: epoch
          ) == true
        else {
          self?.finishPollingCycle(
            token: token,
            generation: generation,
            revisionEpoch: epoch,
            replacement: nil,
            error: nil,
            taskWasCancelled: Task.isCancelled
          )
          return
        }
        if self?.polledRevisionNeedsSnapshot(revision) != true {
          self?.finishPollingCycle(
            token: token,
            generation: generation,
            revisionEpoch: epoch,
            replacement: nil,
            error: nil,
            taskWasCancelled: Task.isCancelled
          )
        } else {
          let replacement = try await bridge.snapshot()
          self?.finishPollingCycle(
            token: token,
            generation: generation,
            revisionEpoch: epoch,
            replacement: replacement,
            error: nil,
            taskWasCancelled: Task.isCancelled
          )
        }
      } catch {
        self?.finishPollingCycle(
          token: token,
          generation: generation,
          revisionEpoch: epoch,
          replacement: nil,
          error: error,
          taskWasCancelled: Task.isCancelled
        )
      }
    }
  }

  private func pollingCycleMayContinue(
    token: UUID,
    generation: UInt64,
    revisionEpoch: UInt64
  ) -> Bool {
    !Task.isCancelled
      && isActive
      && generation == pollGeneration
      && activeOperationID == nil
      && revisionEpoch == self.revisionEpoch
      && bridgeActivity
        == .polling(token: token, generation: generation, revisionEpoch: revisionEpoch)
  }

  private func polledRevisionNeedsSnapshot(_ revision: StateRevision) -> Bool {
    guard let current = snapshot?.revision else { return true }
    guard revision.dataGeneration >= current.dataGeneration else { return false }
    return revision != current
  }

  private func finishPollingCycle(
    token: UUID,
    generation: UInt64,
    revisionEpoch: UInt64,
    replacement: AppSnapshot?,
    error: (any Error)?,
    taskWasCancelled: Bool
  ) {
    guard
      bridgeActivity
        == .polling(token: token, generation: generation, revisionEpoch: revisionEpoch)
    else { return }
    let mayApply =
      !taskWasCancelled
      && isActive
      && generation == pollGeneration
      && activeOperationID == nil
      && revisionEpoch == self.revisionEpoch
    pollBridgeTask = nil
    bridgeActivity = .idle
    if mayApply, let replacement {
      replaceSnapshot(replacement)
    } else if mayApply, let error {
      apply(error)
    }
    beginNextBridgeActivityIfPossible()
  }

  private func beginReload(origin: ReloadOrigin) {
    guard bridgeActivity == .idle else { return }
    invalidatePendingRevisionReads()
    let token = UUID()
    bridgeActivity = .reloading(token: token, origin: origin)
    let bridge = bridge
    reloadTask = Task { [weak self, bridge] in
      do {
        let replacement = try await bridge.snapshot()
        self?.finishReload(
          token: token,
          origin: origin,
          replacement: replacement,
          error: nil,
          taskWasCancelled: Task.isCancelled
        )
      } catch {
        self?.finishReload(
          token: token,
          origin: origin,
          replacement: nil,
          error: error,
          taskWasCancelled: Task.isCancelled
        )
      }
    }
  }

  private func finishReload(
    token: UUID,
    origin: ReloadOrigin,
    replacement: AppSnapshot?,
    error: (any Error)?,
    taskWasCancelled: Bool
  ) {
    guard bridgeActivity == .reloading(token: token, origin: origin) else { return }
    invalidatePendingRevisionReads()
    reloadTask = nil
    bridgeActivity = .idle

    let mayApply: Bool
    switch origin {
    case .requested:
      mayApply = !taskWasCancelled
    case .polling(let generation):
      mayApply =
        !taskWasCancelled
        && isActive
        && generation == pollGeneration
        && activeOperationID == nil
    }
    if mayApply, let replacement {
      replaceSnapshot(replacement)
    } else if mayApply, let error {
      apply(error)
    }

    let waiters = pendingReloadWaiters
    pendingReloadWaiters.removeAll()
    for waiter in waiters {
      waiter.resume()
    }
    beginNextBridgeActivityIfPossible()
  }

  private func beginRefresh(_ request: PendingRefresh) {
    guard bridgeActivity == .idle,
      activeOperationID == request.operationID
    else { return }
    bridgeActivity = .refreshing(operationID: request.operationID)
    let bridge = bridge
    refreshTask = Task { [weak self, bridge] in
      guard !Task.isCancelled else {
        self?.finishRefresh(
          operationID: request.operationID,
          replacement: nil,
          error: BridgeError.cancelled
        )
        return
      }
      do {
        _ = try await bridge.refresh(operationID: request.operationID, ai: request.ai)
        guard !Task.isCancelled else {
          self?.finishRefresh(
            operationID: request.operationID,
            replacement: nil,
            error: BridgeError.cancelled
          )
          return
        }
        guard self?.refreshMayLoadSnapshot(operationID: request.operationID) == true else { return }
        let replacement = try await bridge.snapshot()
        guard !Task.isCancelled else {
          self?.finishRefresh(
            operationID: request.operationID,
            replacement: nil,
            error: BridgeError.cancelled
          )
          return
        }
        self?.finishRefresh(
          operationID: request.operationID,
          replacement: replacement,
          error: nil
        )
      } catch {
        self?.finishRefresh(
          operationID: request.operationID,
          replacement: nil,
          error: Task.isCancelled ? BridgeError.cancelled : error
        )
      }
    }
  }

  private func refreshMayLoadSnapshot(operationID: String) -> Bool {
    activeOperationID == operationID
      && bridgeActivity == .refreshing(operationID: operationID)
  }

  private func finishRefresh(
    operationID: String,
    replacement: AppSnapshot?,
    error: (any Error)?
  ) {
    guard refreshMayLoadSnapshot(operationID: operationID) else { return }
    invalidatePendingRevisionReads()
    bridgeActivity = .idle
    refreshTask = nil
    activeOperationID = nil
    if let replacement {
      replaceSnapshot(replacement)
    } else if let error {
      apply(error)
    }
    beginNextBridgeActivityIfPossible()
  }

  private func invalidatePendingRevisionReads() {
    revisionEpoch &+= 1
  }

  private func replaceSnapshot(_ replacement: AppSnapshot) {
    if let current = snapshot,
      replacement.revision.dataGeneration < current.revision.dataGeneration
    {
      return
    }
    snapshot = replacement
    summarySelections = summarySelections.filter { storyID, selection in
      replacement.containsStory(id: storyID)
        && isValid(selection, for: story(id: storyID))
    }
    validateSelectedStoryForDestination()
    phase = presentationPhase(for: replacement)
  }

  private func replaceStory(_ replacement: Story, revision: StateRevision) {
    guard let snapshot else { return }
    let today = snapshot.today.map { briefing in
      Briefing(
        date: briefing.date,
        generatedAt: briefing.generatedAt,
        isStale: briefing.isStale,
        items: briefing.items.map { item in
          guard item.story.id == replacement.id else { return item }
          return BriefingItem(
            position: item.position,
            section: item.section,
            isStale: item.isStale,
            story: replacement,
            selectedSummary: replacement.selectedSummary,
            summaryVariants: replacement.summaryVariants
          )
        }
      )
    }
    let latest = snapshot.latest.map { $0.id == replacement.id ? replacement : $0 }
    var saved = snapshot.saved
    if let index = saved.firstIndex(where: { $0.id == replacement.id }) {
      if replacement.isSaved {
        saved[index] = replacement
      } else {
        saved.remove(at: index)
      }
    } else if replacement.isSaved {
      saved.append(replacement)
    }
    self.snapshot = AppSnapshot(
      revision: revision,
      status: snapshot.status,
      today: today,
      latest: latest,
      saved: saved,
      sources: snapshot.sources,
      modelProfiles: snapshot.modelProfiles,
      defaultModelProfileID: snapshot.defaultModelProfileID,
      hasUsableAIProfile: snapshot.hasUsableAIProfile
    )
    if let selectedStoryID, self.snapshot?.containsStory(id: selectedStoryID) != true {
      summarySelections.removeValue(forKey: selectedStoryID)
    }
    validateSelectedStoryForDestination()
  }

  private func replaceSource(_ replacement: Source, revision: StateRevision, removing: Bool) {
    guard let snapshot else { return }
    var sources = snapshot.sources
    if removing {
      sources.removeAll(where: { $0.id == replacement.id })
    } else if let index = sources.firstIndex(where: { $0.id == replacement.id }) {
      sources[index] = replacement
    } else {
      sources.append(replacement)
    }
    self.snapshot = AppSnapshot(
      revision: revision,
      status: snapshot.status,
      today: snapshot.today,
      latest: snapshot.latest,
      saved: snapshot.saved,
      sources: sources,
      modelProfiles: snapshot.modelProfiles,
      defaultModelProfileID: snapshot.defaultModelProfileID,
      hasUsableAIProfile: snapshot.hasUsableAIProfile
    )
  }

  private func isValid(_ selection: ReadingSummarySelection, for story: Story?) -> Bool {
    guard let story else { return false }
    switch selection {
    case .raw, .smart: return true
    case .ai(let variantID):
      return story.summaryVariants.contains(where: { $0.id == variantID })
    }
  }

  private func advanceSummarySelectionIntent(for storyID: String) -> UInt64 {
    let next = (summarySelectionIntentEpochs[storyID] ?? 0) &+ 1
    summarySelectionIntentEpochs[storyID] = next
    return next
  }

  private func validateSelectedStoryForDestination() {
    guard selectedStoryID != nil else { return }
    if selectedStory == nil {
      selectedStoryID = nil
    }
  }

  private func actionStoryID(_ action: StoryAction) -> String {
    switch action {
    case .saving(let storyID), .markingRead(let storyID), .selectingSummary(let storyID),
      .regenerating(let storyID):
      return storyID
    }
  }

  private func presentationPhase(for snapshot: AppSnapshot) -> AppPhase {
    if !preferences.welcomeCompleted {
      return .welcome
    }
    if snapshot.today?.isStale == true
      || snapshot.today?.items.contains(where: \.isStale) == true
    {
      return .stale
    }
    if snapshot.today == nil, snapshot.latest.isEmpty {
      return .empty
    }
    return .ready
  }

  private func apply(_ error: any Error) {
    guard let error = error as? BridgeError else {
      phase = .failure(message: userFacingMessage(for: error))
      return
    }
    switch error {
    case .startupUnavailable:
      phase = .startupFailure(
        message:
          "AI Daily Signal could not open its local data. Quit and reopen the app. If the problem continues, make sure this Mac has available storage."
      )
    case .cancelled:
      phase = snapshot.map(presentationPhase(for:)) ?? .empty
    case .offline:
      phase = .offline(
        message: snapshot?.today == nil
          ? "The network is unavailable. Try again when you're online."
          : "The network is unavailable. Your last briefing is still here."
      )
    case .storageUnavailable:
      phase = .failure(message: "AI Daily Signal cannot access local storage.")
    case .notInitialized:
      phase = .failure(message: "Setup is incomplete.")
    case .invalidInput:
      phase = .failure(message: "Check the information and try again.")
    case .notFound:
      phase = .failure(message: "That item is no longer available.")
    case .credentialUnavailable:
      phase = .failure(message: "The model credential is unavailable.")
    case .consentRequired:
      phase = .failure(message: "Provider data-sharing consent is required.")
    case .budgetExhausted:
      phase = .failure(message: "The daily AI budget has been reached.")
    case .providerUnavailable:
      phase = .failure(message: "The AI provider is unavailable. Smart summaries were kept.")
    case .refreshAlreadyRunning:
      phase = .failure(message: "A refresh is already running.")
    }
  }

  private func userFacingMessage(for error: any Error) -> String {
    guard let error = error as? BridgeError else {
      return "Something went wrong. Please try again."
    }
    switch error {
    case .startupUnavailable:
      return "Local data is unavailable. Quit and reopen the app."
    case .cancelled:
      return "The action was cancelled. Your previous content was kept."
    case .offline:
      return "The network is unavailable. Your previous content was kept."
    case .storageUnavailable:
      return "AI Daily Signal cannot access local storage."
    case .notInitialized:
      return "Setup is incomplete."
    case .invalidInput:
      return "Check the information and try again."
    case .notFound:
      return "That item is no longer available."
    case .credentialUnavailable:
      return "The model credential is unavailable."
    case .consentRequired:
      return "Provider data-sharing consent is required."
    case .budgetExhausted:
      return "The daily AI budget has been reached."
    case .providerUnavailable:
      return "The AI provider is unavailable. Smart summaries were kept."
    case .refreshAlreadyRunning:
      return "A refresh is already running."
    }
  }
}
