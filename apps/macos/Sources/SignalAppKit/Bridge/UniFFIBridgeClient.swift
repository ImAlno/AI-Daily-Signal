import SignalFFIBindings

public final class UniFFIBridgeClient: BridgeClient, Sendable {
  private let client: any CompanionClientProtocol

  init(client: any CompanionClientProtocol) {
    self.client = client
  }

  public convenience init() throws {
    try self.init(client: CompanionClient())
  }

  public func snapshot() async throws -> AppSnapshot {
    try await call { try await client.snapshot().localValue }
  }

  public func stateRevision() async throws -> StateRevision {
    try await call { try await client.stateRevision().localValue }
  }

  public func refresh(operationID: String, ai: Bool) async throws -> RefreshResult {
    try await call {
      try await client.refresh(operationId: operationID, ai: ai).localValue
    }
  }

  public func cancelOperation(id: String) -> Bool {
    client.cancelOperation(operationId: id)
  }

  public func setSaved(storyID: String, saved: Bool) async throws -> Story {
    try await call {
      try await client.setStorySaved(id: storyID, saved: saved).story.localValue
    }
  }

  public func setRead(storyID: String, read: Bool) async throws -> Story {
    try await call {
      try await client.setStoryRead(id: storyID, read: read).story.localValue
    }
  }

  public func selectSummary(storyID: String, variantID: String) async throws -> SummaryVariant {
    try await call {
      let mutation = try await client.selectSummaryVariant(
        storyId: storyID,
        variantId: variantID
      )
      guard let selected = mutation.story.selectedSummary else {
        throw BridgeError.storageUnavailable
      }
      return selected.localValue
    }
  }

  public func regenerate(storyID: String, profile: String?, force: Bool) async throws
    -> GenerationResult
  {
    try await call {
      let mutation = try await client.regenerateStory(
        storyId: storyID,
        profile: profile,
        force: force
      )
      let story = mutation.story.localValue
      return GenerationResult(
        story: story,
        selectedSummary: story.selectedSummary,
        revision: mutation.revision.localValue
      )
    }
  }

  public func addSource(_ input: FeedSourceInput) async throws -> Source {
    try await call {
      try await client.addFeedSource(request: input.ffiValue).source.localValue
    }
  }

  public func setSourceEnabled(id: String, enabled: Bool) async throws -> Source {
    try await call {
      try await client.setSourceEnabled(id: id, enabled: enabled).source.localValue
    }
  }

  public func removeSource(id: String) async throws -> Source {
    try await call {
      try await client.removePersonalSource(id: id).source.localValue
    }
  }

  public func addModel(_ input: ModelProfileInput) async throws -> ModelProfile {
    try await call {
      try await client.addModelProfile(request: input.ffiValue).profile.localValue
    }
  }

  public func setDefaultModel(_ selector: String) async throws -> ModelProfile {
    try await call {
      try await client.setDefaultModelProfile(profile: selector).profile.localValue
    }
  }

  public func testModel(_ selector: String) async throws -> ModelTestResult {
    try await call {
      let result = try await client.testModelProfile(profile: selector)
      return ModelTestResult(
        profile: result.profile.localValue,
        costMayApply: result.costMayApply
      )
    }
  }

  public func removeModel(_ selector: String) async throws -> ModelRemovalResult {
    try await call {
      let result = try await client.removeModelProfile(profile: selector)
      return ModelRemovalResult(
        profile: result.profile.localValue,
        credentialDeletion: result.credentialDeletion.localValue
      )
    }
  }

  private func call<Value: Sendable>(
    _ operation: () async throws -> Value
  ) async throws -> Value {
    do {
      return try await operation()
    } catch let error as SignalFFIBindings.CompanionError {
      throw BridgeError(error)
    }
  }
}

extension BridgeError {
  fileprivate init(_ error: SignalFFIBindings.CompanionError) {
    switch error {
    case .NotInitialized: self = .notInitialized
    case .InvalidInput: self = .invalidInput
    case .NotFound: self = .notFound
    case .CredentialUnavailable: self = .credentialUnavailable
    case .ConsentRequired: self = .consentRequired
    case .BudgetExhausted: self = .budgetExhausted
    case .ProviderUnavailable: self = .providerUnavailable
    case .Offline: self = .offline
    case .RefreshAlreadyRunning: self = .refreshAlreadyRunning
    case .Cancelled: self = .cancelled
    case .StorageUnavailable: self = .storageUnavailable
    }
  }
}

extension SignalFFIBindings.FfiStateRevision {
  fileprivate var localValue: StateRevision {
    StateRevision(
      dataGeneration: dataGeneration,
      sourceConfigRevision: sourceConfigRevision
    )
  }
}

extension SignalFFIBindings.FfiCollectionState {
  fileprivate var localValue: CollectionState {
    switch self {
    case .notInitialized: .notInitialized
    case .ready: .ready
    }
  }
}

extension SignalFFIBindings.FfiRefreshMetadata {
  fileprivate var localValue: RefreshMetadata {
    RefreshMetadata(
      lastRefreshAt: SignalFormatters.bridgeDate(lastRefreshAt),
      storyCount: storyCount
    )
  }
}

extension SignalFFIBindings.FfiCollectionStatus {
  fileprivate var localValue: CollectionStatus {
    CollectionStatus(state: state.localValue, refresh: refresh?.localValue)
  }
}

extension SignalFFIBindings.FfiScore {
  fileprivate var localValue: Score {
    Score(
      recency: recency,
      sourceWeight: sourceWeight,
      corroboration: corroboration,
      total: total
    )
  }
}

extension SignalFFIBindings.FfiSummaryFields {
  fileprivate var localValue: SummaryFields {
    SummaryFields(
      whatHappened: whatHappened,
      whyItMatters: whyItMatters,
      caveat: caveat
    )
  }
}

extension SignalFFIBindings.FfiProviderKind {
  fileprivate var localValue: ProviderKind {
    switch self {
    case .openAi: .openAI
    case .anthropic: .anthropic
    case .gemini: .gemini
    case .openAiCompatible: .openAICompatible
    }
  }
}

extension ProviderKind {
  fileprivate var ffiValue: SignalFFIBindings.FfiProviderKind {
    switch self {
    case .openAI: .openAi
    case .anthropic: .anthropic
    case .gemini: .gemini
    case .openAICompatible: .openAiCompatible
    }
  }
}

extension SignalFFIBindings.FfiApiDialect {
  fileprivate var localValue: APIDialect {
    switch self {
    case .responses: .responses
    case .chatCompletions: .chatCompletions
    }
  }
}

extension APIDialect {
  fileprivate var ffiValue: SignalFFIBindings.FfiApiDialect {
    switch self {
    case .responses: .responses
    case .chatCompletions: .chatCompletions
    }
  }
}

extension SignalFFIBindings.FfiSummaryVariant {
  fileprivate var localValue: SummaryVariant {
    SummaryVariant(
      id: id,
      storyID: storyId,
      profileID: profileId,
      provider: provider.localValue,
      model: model,
      dialect: dialect?.localValue,
      fields: fields.localValue,
      generatedAt: SignalFormatters.bridgeDate(generatedAt)
    )
  }
}

extension SignalFFIBindings.FfiStory {
  fileprivate var localValue: Story {
    Story(
      id: id,
      title: title,
      canonicalURL: canonicalUrl,
      excerpt: excerpt,
      category: category,
      publishedAt: SignalFormatters.bridgeDate(publishedAt),
      sourceIDs: sourceIds,
      score: score.localValue,
      smartSummary: smartSummary,
      isRead: isRead,
      isSaved: isSaved,
      selectedSummary: selectedSummary?.localValue,
      summaryVariants: summaryVariants.map(\.localValue)
    )
  }
}

extension SignalFFIBindings.FfiBriefingItem {
  fileprivate var localValue: BriefingItem {
    BriefingItem(
      position: position,
      section: section,
      isStale: isStale,
      story: story.localValue,
      selectedSummary: selectedSummary?.localValue,
      summaryVariants: summaryVariants.map(\.localValue)
    )
  }
}

extension SignalFFIBindings.FfiBriefing {
  fileprivate var localValue: Briefing {
    Briefing(
      date: date,
      generatedAt: SignalFormatters.bridgeDate(generatedAt),
      isStale: isStale,
      items: items.map(\.localValue)
    )
  }
}

extension SignalFFIBindings.FfiSourceOrigin {
  fileprivate var localValue: SourceOrigin {
    switch self {
    case .standard: .standard
    case .personal: .personal
    }
  }
}

extension SignalFFIBindings.FfiSource {
  fileprivate var localValue: Source {
    Source(
      id: id,
      name: name,
      category: category,
      enabled: enabled,
      weight: weight,
      feedURL: feedUrl,
      origin: origin.localValue
    )
  }
}

extension SignalFFIBindings.FfiCredentialSourceKind {
  fileprivate var localValue: CredentialSourceKind {
    switch self {
    case .systemStore: .systemStore
    case .environment: .environment
    }
  }
}

extension SignalFFIBindings.FfiProfileLimits {
  fileprivate var localValue: ProfileLimits {
    ProfileLimits(
      maxSummariesPerRefresh: maxSummariesPerRefresh,
      maxDailyCostMicrousd: maxDailyCostMicrousd,
      inputCostMicrousdPerMillion: inputCostMicrousdPerMillion,
      outputCostMicrousdPerMillion: outputCostMicrousdPerMillion,
      maxOutputTokens: maxOutputTokens,
      timeoutSeconds: timeoutSeconds,
      maxRetries: maxRetries
    )
  }
}

extension SignalFFIBindings.FfiModelProfile {
  fileprivate var localValue: ModelProfile {
    ModelProfile(
      id: id,
      name: name,
      provider: provider.localValue,
      model: model,
      endpoint: endpoint,
      dialect: dialect?.localValue,
      credentialSource: credentialSource.localValue,
      consentedAt: SignalFormatters.bridgeDate(consentedAt),
      enabled: enabled,
      limits: limits.localValue,
      createdAt: SignalFormatters.bridgeDate(createdAt),
      updatedAt: SignalFormatters.bridgeDate(updatedAt)
    )
  }
}

extension SignalFFIBindings.FfiGenerationReport {
  fileprivate var localValue: GenerationReport {
    GenerationReport(
      eligible: eligible,
      generated: generated,
      cacheHits: cacheHits,
      skippedCap: skippedCap,
      skippedBudget: skippedBudget,
      missingCredentials: missingCredentials,
      providerFailures: providerFailures,
      malformedOutputs: malformedOutputs,
      smartFallbacks: smartFallbacks
    )
  }
}

extension SignalFFIBindings.FfiRefreshResult {
  fileprivate var localValue: RefreshResult {
    RefreshResult(
      briefing: briefing.localValue,
      successfulSources: successfulSources,
      failedSources: failedSources,
      generation: generation.localValue,
      revision: revision.localValue
    )
  }
}

extension SignalFFIBindings.CompanionSnapshot {
  fileprivate var localValue: AppSnapshot {
    AppSnapshot(
      revision: revision.localValue,
      status: status.localValue,
      today: today?.localValue,
      latest: latest.map(\.localValue),
      saved: saved.map(\.localValue),
      sources: sources.map(\.localValue),
      modelProfiles: modelProfiles.map(\.localValue),
      defaultModelProfileID: defaultModelProfileId,
      hasUsableAIProfile: hasUsableAiProfile
    )
  }
}

extension FeedSourceInput {
  fileprivate var ffiValue: SignalFFIBindings.AddFeedSourceRequest {
    AddFeedSourceRequest(
      name: name,
      category: category,
      url: url,
      weight: weight,
      enabled: enabled
    )
  }
}

extension ModelCredentialInput {
  fileprivate var ffiValue: SignalFFIBindings.AddCredentialRequest {
    switch self {
    case .systemStore(let secret): .systemStore(secret: secret)
    case .environment(let variable): .environment(variable: variable)
    }
  }
}

extension ProfileLimitsInput {
  fileprivate var ffiValue: SignalFFIBindings.FfiProfileLimitsInput {
    FfiProfileLimitsInput(
      maxSummariesPerRefresh: maxSummariesPerRefresh,
      maxDailyCostUsd: maxDailyCostUSD,
      inputCostUsdPerMillion: inputCostUSDPerMillion,
      outputCostUsdPerMillion: outputCostUSDPerMillion,
      maxOutputTokens: maxOutputTokens,
      timeoutSeconds: timeoutSeconds,
      maxRetries: maxRetries
    )
  }
}

extension ModelProfileInput {
  fileprivate var ffiValue: SignalFFIBindings.AddModelProfileRequest {
    AddModelProfileRequest(
      name: name,
      provider: provider.ffiValue,
      model: model,
      endpoint: endpoint,
      dialect: dialect?.ffiValue,
      credential: credential.ffiValue,
      consentProviderDataSharing: consentProviderDataSharing,
      limits: limits.ffiValue
    )
  }
}

extension SignalFFIBindings.FfiCredentialDeletionStatus {
  fileprivate var localValue: CredentialDeletionStatus {
    switch self {
    case .deleted: .deleted
    case .notApplicable: .notApplicable
    case .deleteFailed: .deleteFailed
    }
  }
}
