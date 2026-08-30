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
  @ObservationIgnored private var reloadInProgress = false

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
    guard refreshTask == nil else { return }
    let operationID = UUID().uuidString
    activeOperationID = operationID
    phase = .refreshing
    refreshTask = Task { [weak self] in
      await self?.performRefresh(operationID: operationID, ai: ai)
    }
    await Task.yield()
  }

  public func cancelRefresh() {
    guard let activeOperationID else { return }
    _ = bridge.cancelOperation(id: activeOperationID)
  }

  public func setActive(_ active: Bool) {
    if active {
      guard pollTask == nil else { return }
      pollTask = Task { [weak self] in
        await self?.pollRevisionWhileActive()
      }
    } else {
      pollTask?.cancel()
      pollTask = nil
    }
  }

  public func pollRevisionWhileActive() async {
    while !Task.isCancelled {
      do {
        try await Task.sleep(for: pollInterval)
        guard !Task.isCancelled else { break }
        guard refreshTask == nil, !reloadInProgress else { continue }
        let revision = try await bridge.stateRevision()
        if revision != snapshot?.revision {
          await reloadSnapshot()
        }
      } catch is CancellationError {
        break
      } catch {
        apply(error)
      }
    }
  }

  public func reloadSnapshot() async {
    guard !reloadInProgress else { return }
    reloadInProgress = true
    defer { reloadInProgress = false }
    do {
      replaceSnapshot(try await bridge.snapshot())
    } catch {
      apply(error)
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

  private func performRefresh(operationID: String, ai: Bool) async {
    defer {
      if activeOperationID == operationID {
        activeOperationID = nil
        refreshTask = nil
      }
    }
    do {
      _ = try await bridge.refresh(operationID: operationID, ai: ai)
      await reloadSnapshot()
    } catch {
      apply(error)
    }
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
