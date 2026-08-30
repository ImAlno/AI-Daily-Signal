import SignalAppKit

@MainActor
final class AppEnvironment {
  let preferences: UserDefaultsAppPreferences
  let model: AppModel
  let windowCoordinator: WindowCoordinator

  init() {
    let preferences = UserDefaultsAppPreferences()
    let bridge: any BridgeClient
    do {
      bridge = try UniFFIBridgeClient()
    } catch {
      bridge = StartupFailureBridgeClient()
    }
    let model = AppModel(bridge: bridge, preferences: preferences)
    self.preferences = preferences
    self.model = model
    windowCoordinator = WindowCoordinator(model: model)
  }
}

private final class StartupFailureBridgeClient: BridgeClient, Sendable {
  func snapshot() async throws -> AppSnapshot { throw BridgeError.startupUnavailable }
  func stateRevision() async throws -> StateRevision { throw BridgeError.startupUnavailable }
  func refresh(operationID: String, ai: Bool) async throws -> RefreshResult {
    throw BridgeError.startupUnavailable
  }
  func cancelOperation(id: String) -> Bool { false }
  func setSaved(storyID: String, saved: Bool) async throws -> StoryMutationResult {
    throw BridgeError.startupUnavailable
  }
  func setRead(storyID: String, read: Bool) async throws -> StoryMutationResult {
    throw BridgeError.startupUnavailable
  }
  func selectSummary(storyID: String, variantID: String) async throws -> StoryMutationResult {
    throw BridgeError.startupUnavailable
  }
  func regenerate(storyID: String, profile: String?, force: Bool) async throws
    -> GenerationResult
  {
    throw BridgeError.startupUnavailable
  }
  func addSource(_ input: FeedSourceInput) async throws -> Source {
    throw BridgeError.startupUnavailable
  }
  func setSourceEnabled(id: String, enabled: Bool) async throws -> Source {
    throw BridgeError.startupUnavailable
  }
  func removeSource(id: String) async throws -> Source {
    throw BridgeError.startupUnavailable
  }
  func addModel(_ input: ModelProfileInput) async throws -> ModelProfile {
    throw BridgeError.startupUnavailable
  }
  func setDefaultModel(_ selector: String) async throws -> ModelProfile {
    throw BridgeError.startupUnavailable
  }
  func testModel(_ selector: String) async throws -> ModelTestResult {
    throw BridgeError.startupUnavailable
  }
  func removeModel(_ selector: String) async throws -> ModelRemovalResult {
    throw BridgeError.startupUnavailable
  }
}
