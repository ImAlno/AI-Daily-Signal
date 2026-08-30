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
  func reserveRefresh(operationID: String) throws
  func releaseRefreshReservation(operationID: String) -> Bool
  func refresh(operationID: String, ai: Bool) async throws -> RefreshResult
  func cancelOperation(id: String) -> Bool
  func setSaved(storyID: String, saved: Bool) async throws -> StoryMutationResult
  func setRead(storyID: String, read: Bool) async throws -> StoryMutationResult
  func selectSummary(storyID: String, variantID: String) async throws -> StoryMutationResult
  func regenerate(storyID: String, profile: String?, force: Bool) async throws -> GenerationResult
  func addSource(_ input: FeedSourceInput) async throws -> SourceMutationResult
  func setSourceEnabled(id: String, enabled: Bool) async throws -> SourceMutationResult
  func removeSource(id: String) async throws -> SourceMutationResult
  func addModel(_ input: ModelProfileInput) async throws -> ModelMutationResult
  func setDefaultModel(_ selector: String) async throws -> ModelMutationResult
  func testModel(_ selector: String) async throws -> ModelTestResult
  func removeModel(_ selector: String) async throws -> ModelRemovalResult
}
