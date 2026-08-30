public enum BridgeError: Error, Sendable, Equatable {
  case startupUnavailable
  case notInitialized
  case invalidInput
  case notFound
  case credentialUnavailable
  case consentRequired
  case budgetExhausted
  case providerUnavailable
  case offline
  case refreshAlreadyRunning
  case cancelled
  case storageUnavailable
}

public protocol BridgeClient: Sendable {
  func snapshot() async throws -> AppSnapshot
  func stateRevision() async throws -> StateRevision
  func refresh(operationID: String, ai: Bool) async throws -> RefreshResult
  func cancelOperation(id: String) -> Bool
  func setSaved(storyID: String, saved: Bool) async throws -> StoryMutationResult
  func setRead(storyID: String, read: Bool) async throws -> Story
  func selectSummary(storyID: String, variantID: String) async throws -> SummaryVariant
  func regenerate(storyID: String, profile: String?, force: Bool) async throws -> GenerationResult
  func addSource(_ input: FeedSourceInput) async throws -> Source
  func setSourceEnabled(id: String, enabled: Bool) async throws -> Source
  func removeSource(id: String) async throws -> Source
  func addModel(_ input: ModelProfileInput) async throws -> ModelProfile
  func setDefaultModel(_ selector: String) async throws -> ModelProfile
  func testModel(_ selector: String) async throws -> ModelTestResult
  func removeModel(_ selector: String) async throws -> ModelRemovalResult
}
