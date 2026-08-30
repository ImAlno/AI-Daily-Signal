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
  func snapshot() async throws -> AppSnapshot { throw BridgeError.storageUnavailable }
  func stateRevision() async throws -> StateRevision { throw BridgeError.storageUnavailable }
  func refresh(operationID: String, ai: Bool) async throws -> RefreshResult {
    throw BridgeError.storageUnavailable
  }
  func cancelOperation(id: String) -> Bool { false }
  func setSaved(storyID: String, saved: Bool) async throws -> Story {
    throw BridgeError.storageUnavailable
  }
  func setRead(storyID: String, read: Bool) async throws -> Story {
    throw BridgeError.storageUnavailable
  }
  func selectSummary(storyID: String, variantID: String) async throws -> SummaryVariant {
    throw BridgeError.storageUnavailable
  }
  func regenerate(storyID: String, profile: String?, force: Bool) async throws
    -> GenerationResult
  {
    throw BridgeError.storageUnavailable
  }
  func addSource(_ input: FeedSourceInput) async throws -> Source {
    throw BridgeError.storageUnavailable
  }
  func setSourceEnabled(id: String, enabled: Bool) async throws -> Source {
    throw BridgeError.storageUnavailable
  }
  func removeSource(id: String) async throws -> Source {
    throw BridgeError.storageUnavailable
  }
  func addModel(_ input: ModelProfileInput) async throws -> ModelProfile {
    throw BridgeError.storageUnavailable
  }
  func setDefaultModel(_ selector: String) async throws -> ModelProfile {
    throw BridgeError.storageUnavailable
  }
  func testModel(_ selector: String) async throws -> ModelTestResult {
    throw BridgeError.storageUnavailable
  }
  func removeModel(_ selector: String) async throws -> ModelRemovalResult {
    throw BridgeError.storageUnavailable
  }
}
