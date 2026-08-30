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

private enum ReloadOrigin: Equatable {
  case requested
  case polling(generation: UInt64)
}

private enum BridgeActivity: Equatable {
  case idle
  case reloading(token: UUID, origin: ReloadOrigin)
  case refreshing(operationID: String)
  case mutating(token: UUID, action: StoryAction)
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
  let continuation: CheckedContinuation<Void, Never>
}

private struct StoryMutationConfirmation {
  let story: Story
  let revision: StateRevision
  let generatedSelectionID: String?
}

@MainActor
@Observable
public final class AppModel {
  public private(set) var snapshot: AppSnapshot?
  public private(set) var phase: AppPhase = .loading
  public private(set) var activeOperationID: String?
  public private(set) var storyActionStates: [StoryAction: StoryActionState] = [:]
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
  @ObservationIgnored private var reloadTask: Task<Void, Never>?
  @ObservationIgnored private var bridgeActivity = BridgeActivity.idle
  @ObservationIgnored private var pendingRefresh: PendingRefresh?
  @ObservationIgnored private var pendingReloadWaiters: [CheckedContinuation<Void, Never>] = []
  @ObservationIgnored private var pendingStoryMutations: [PendingStoryMutation] = []
  @ObservationIgnored private var isActive = false
  @ObservationIgnored private var pollGeneration: UInt64 = 0
  @ObservationIgnored private var revisionEpoch: UInt64 = 0
  private var summarySelections: [String: ReadingSummarySelection] = [:]
  private var storyActionErrors: [StoryAction: String] = [:]

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
    case .reloading, .mutating:
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
      await enqueueStoryMutation(
        action: action,
        payload: .select(storyID: storyID, variantID: variantID)
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
    await enqueueStoryMutation(
      action: action,
      payload: .regenerate(
        storyID: selectedStory.id,
        profileID: resolvedProfile,
        force: force
      )
    )
  }

  private func enqueueStoryMutation(
    action: StoryAction,
    payload: StoryMutationPayload
  ) async {
    storyActionErrors[action] = nil
    storyActionStates[action] = .queued
    await withCheckedContinuation { continuation in
      pendingStoryMutations.append(
        PendingStoryMutation(
          token: UUID(),
          action: action,
          payload: payload,
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
        self?.finishStoryMutation(pending, confirmation: confirmation, error: nil)
      } catch {
        self?.finishStoryMutation(pending, confirmation: nil, error: error)
      }
    }
  }

  private func finishStoryMutation(
    _ pending: PendingStoryMutation,
    confirmation: StoryMutationConfirmation?,
    error: (any Error)?
  ) {
    guard bridgeActivity == .mutating(token: pending.token, action: pending.action) else { return }
    invalidatePendingRevisionReads()
    if let confirmation,
      confirmation.revision.dataGeneration > (snapshot?.revision.dataGeneration ?? 0)
    {
      replaceStory(confirmation.story, revision: confirmation.revision)
      if let variantID = confirmation.generatedSelectionID {
        summarySelections[confirmation.story.id] = .ai(variantID: variantID)
      }
    } else if let error {
      storyActionErrors[pending.action] = userFacingMessage(for: error)
    }
    storyActionStates[pending.action] = nil
    bridgeActivity = .idle
    pending.continuation.resume()
    beginNextBridgeActivityIfPossible()
  }

  private func beginNextBridgeActivityIfPossible() {
    guard bridgeActivity == .idle else { return }
    if let pendingRefresh {
      self.pendingRefresh = nil
      beginRefresh(pendingRefresh)
    } else if !pendingStoryMutations.isEmpty {
      beginStoryMutation(pendingStoryMutations.removeFirst())
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
    case .mutating:
      await withCheckedContinuation { continuation in
        pendingReloadWaiters.append(continuation)
      }
    }
  }

  @discardableResult
  public func addModel(
    _ input: ModelProfileInput,
    clearSecret: @MainActor () -> Void
  ) async -> ModelProfile? {
    defer { clearSecret() }
    do {
      return try await bridge.addModel(input)
    } catch {
      apply(error)
      return nil
    }
  }

  private func startPolling() {
    guard !isActive else { return }
    invalidatePendingRevisionReads()
    isActive = true
    pollGeneration &+= 1
    let generation = pollGeneration
    let interval = pollInterval
    let bridge = bridge
    pollTask = Task { [weak self, bridge] in
      while !Task.isCancelled {
        do {
          try await Task.sleep(for: interval)
        } catch is CancellationError {
          return
        } catch {
          guard !Task.isCancelled else { return }
          self?.receivePollingError(error, generation: generation)
          continue
        }
        guard !Task.isCancelled else { return }
        guard let revisionEpoch = self?.revisionReadEpoch(generation: generation) else { continue }
        do {
          let revision = try await bridge.stateRevision()
          guard !Task.isCancelled else { return }
          self?.receivePolledRevision(
            revision,
            generation: generation,
            revisionEpoch: revisionEpoch
          )
        } catch is CancellationError {
          return
        } catch {
          guard !Task.isCancelled else { return }
          self?.receivePollingError(
            error,
            generation: generation,
            revisionEpoch: revisionEpoch
          )
        }
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
    if case .reloading(_, .polling) = bridgeActivity {
      reloadTask?.cancel()
    }
  }

  private func pollMayReadRevision(generation: UInt64) -> Bool {
    isActive
      && generation == pollGeneration
      && activeOperationID == nil
      && bridgeActivity == .idle
  }

  private func revisionReadEpoch(generation: UInt64) -> UInt64? {
    guard pollMayReadRevision(generation: generation) else { return nil }
    return revisionEpoch
  }

  private func receivePolledRevision(
    _ revision: StateRevision,
    generation: UInt64,
    revisionEpoch: UInt64
  ) {
    guard revisionEpoch == self.revisionEpoch else { return }
    guard pollMayReadRevision(generation: generation) else { return }
    guard revision != snapshot?.revision else { return }
    beginReload(origin: .polling(generation: generation))
  }

  private func receivePollingError(
    _ error: any Error,
    generation: UInt64,
    revisionEpoch: UInt64? = nil
  ) {
    if let revisionEpoch {
      guard revisionEpoch == self.revisionEpoch else { return }
    }
    guard pollMayReadRevision(generation: generation) else { return }
    apply(error)
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

  private func isValid(_ selection: ReadingSummarySelection, for story: Story?) -> Bool {
    guard let story else { return false }
    switch selection {
    case .raw, .smart: return true
    case .ai(let variantID):
      return story.summaryVariants.contains(where: { $0.id == variantID })
    }
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
