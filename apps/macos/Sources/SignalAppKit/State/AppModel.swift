import Foundation
import Observation

public enum AppPhase: Sendable, Equatable {
  case loading
  case welcome
  case empty
  case ready
  case refreshing
  case stale
  case offline(message: String)
  case failure(message: String)
}

private enum ReloadOrigin: Equatable {
  case requested
  case polling(generation: UInt64)
}

private enum BridgeActivity: Equatable {
  case idle
  case reloading(token: UUID, origin: ReloadOrigin)
  case refreshing(operationID: String)
}

private struct PendingRefresh: Equatable {
  let operationID: String
  let ai: Bool
}

@MainActor
@Observable
public final class AppModel {
  public private(set) var snapshot: AppSnapshot?
  public private(set) var phase: AppPhase = .loading
  public private(set) var activeOperationID: String?
  public var destination: Destination {
    didSet { preferences.selectedDestination = destination }
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
  @ObservationIgnored private var isActive = false
  @ObservationIgnored private var pollGeneration: UInt64 = 0
  @ObservationIgnored private var revisionEpoch: UInt64 = 0

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
    case .offline(let message), .failure(let message): message
    default: nil
    }
  }

  public func start() async {
    phase = .loading
    await reloadSnapshot()
  }

  public func buildFirstBriefing() async {
    preferences.welcomeCompleted = true
    await refresh()
  }

  public func refresh(ai: Bool = true) async {
    guard activeOperationID == nil else { return }
    invalidatePendingRevisionReads()
    let operationID = UUID().uuidString
    activeOperationID = operationID
    phase = .refreshing
    let request = PendingRefresh(operationID: operationID, ai: ai)
    switch bridgeActivity {
    case .idle:
      beginRefresh(request)
    case .reloading:
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

    if let pendingRefresh {
      self.pendingRefresh = nil
      beginRefresh(pendingRefresh)
    }
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
  }

  private func invalidatePendingRevisionReads() {
    revisionEpoch &+= 1
  }

  private func replaceSnapshot(_ replacement: AppSnapshot) {
    snapshot = replacement
    if let selectedStoryID, !replacement.containsStory(id: selectedStoryID) {
      self.selectedStoryID = nil
    }
    phase = presentationPhase(for: replacement)
  }

  private func presentationPhase(for snapshot: AppSnapshot) -> AppPhase {
    if snapshot.status.state == .notInitialized, !preferences.welcomeCompleted {
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
      phase = .failure(message: "Something went wrong. Please try again.")
      return
    }
    switch error {
    case .cancelled:
      phase = snapshot.map(presentationPhase(for:)) ?? .empty
    case .offline:
      phase = .offline(
        message: snapshot == nil
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
}
