import Foundation

public struct StateRevision: Sendable, Equatable {
  public let dataGeneration: UInt64
  public let sourceConfigRevision: String

  public init(dataGeneration: UInt64, sourceConfigRevision: String) {
    self.dataGeneration = dataGeneration
    self.sourceConfigRevision = sourceConfigRevision
  }
}

public enum CollectionState: Sendable, Equatable {
  case notInitialized
  case ready
}

public struct RefreshMetadata: Sendable, Equatable {
  public let lastRefreshAt: Date?
  public let storyCount: UInt64

  public init(lastRefreshAt: Date?, storyCount: UInt64) {
    self.lastRefreshAt = lastRefreshAt
    self.storyCount = storyCount
  }
}

public struct CollectionStatus: Sendable, Equatable {
  public let state: CollectionState
  public let refresh: RefreshMetadata?

  public init(state: CollectionState, refresh: RefreshMetadata?) {
    self.state = state
    self.refresh = refresh
  }
}

public struct Score: Sendable, Equatable {
  public let recency: Double
  public let sourceWeight: Double
  public let corroboration: Double
  public let total: Double

  public init(recency: Double, sourceWeight: Double, corroboration: Double, total: Double) {
    self.recency = recency
    self.sourceWeight = sourceWeight
    self.corroboration = corroboration
    self.total = total
  }
}

public struct SummaryFields: Sendable, Equatable {
  public let whatHappened: String
  public let whyItMatters: String
  public let caveat: String?

  public init(whatHappened: String, whyItMatters: String, caveat: String?) {
    self.whatHappened = whatHappened
    self.whyItMatters = whyItMatters
    self.caveat = caveat
  }
}

public enum ProviderKind: String, CaseIterable, Sendable, Equatable {
  case openAI
  case anthropic
  case gemini
  case openAICompatible
}

public enum APIDialect: String, CaseIterable, Sendable, Equatable {
  case responses
  case chatCompletions
}

public struct SummaryVariant: Identifiable, Sendable, Equatable {
  public let id: String
  public let storyID: String
  public let profileID: String?
  public let provider: ProviderKind
  public let model: String
  public let dialect: APIDialect?
  public let fields: SummaryFields
  public let generatedAt: Date?

  public init(
    id: String,
    storyID: String,
    profileID: String?,
    provider: ProviderKind,
    model: String,
    dialect: APIDialect?,
    fields: SummaryFields,
    generatedAt: Date?
  ) {
    self.id = id
    self.storyID = storyID
    self.profileID = profileID
    self.provider = provider
    self.model = model
    self.dialect = dialect
    self.fields = fields
    self.generatedAt = generatedAt
  }
}

public struct Story: Identifiable, Sendable, Equatable {
  public let id: String
  public let title: String
  public let canonicalURL: String
  public let excerpt: String
  public let category: String
  public let publishedAt: Date?
  public let sourceIDs: [String]
  public let score: Score
  public let smartSummary: String
  public let isRead: Bool
  public let isSaved: Bool
  public let selectedSummary: SummaryVariant?
  public let summaryVariants: [SummaryVariant]

  public init(
    id: String,
    title: String,
    canonicalURL: String,
    excerpt: String,
    category: String,
    publishedAt: Date?,
    sourceIDs: [String],
    score: Score,
    smartSummary: String,
    isRead: Bool,
    isSaved: Bool,
    selectedSummary: SummaryVariant?,
    summaryVariants: [SummaryVariant]
  ) {
    self.id = id
    self.title = title
    self.canonicalURL = canonicalURL
    self.excerpt = excerpt
    self.category = category
    self.publishedAt = publishedAt
    self.sourceIDs = sourceIDs
    self.score = score
    self.smartSummary = smartSummary
    self.isRead = isRead
    self.isSaved = isSaved
    self.selectedSummary = selectedSummary
    self.summaryVariants = summaryVariants
  }
}

public struct BriefingItem: Identifiable, Sendable, Equatable {
  public var id: String { story.id }
  public let position: UInt32
  public let section: String
  public let isStale: Bool
  public let story: Story
  public let selectedSummary: SummaryVariant?
  public let summaryVariants: [SummaryVariant]

  public init(
    position: UInt32,
    section: String,
    isStale: Bool,
    story: Story,
    selectedSummary: SummaryVariant?,
    summaryVariants: [SummaryVariant]
  ) {
    self.position = position
    self.section = section
    self.isStale = isStale
    self.story = story
    self.selectedSummary = selectedSummary
    self.summaryVariants = summaryVariants
  }
}

public struct Briefing: Sendable, Equatable {
  public let date: String
  public let generatedAt: Date?
  public let isStale: Bool
  public let items: [BriefingItem]

  public init(date: String, generatedAt: Date?, isStale: Bool, items: [BriefingItem]) {
    self.date = date
    self.generatedAt = generatedAt
    self.isStale = isStale
    self.items = items
  }
}

public enum SourceOrigin: String, Sendable, Equatable {
  case standard
  case personal
}

public struct Source: Identifiable, Sendable, Equatable, CustomDebugStringConvertible,
  CustomReflectable
{
  public let id: String
  public let name: String
  public let category: String
  public let enabled: Bool
  public let weight: Double
  public let feedURL: String
  public let origin: SourceOrigin

  public init(
    id: String, name: String, category: String, enabled: Bool, weight: Double, feedURL: String,
    origin: SourceOrigin
  ) {
    self.id = id
    self.name = name
    self.category = category
    self.enabled = enabled
    self.weight = weight
    self.feedURL = feedURL
    self.origin = origin
  }

  public var debugDescription: String {
    "Source(id: \(id.debugDescription), enabled: \(enabled), origin: \(origin.rawValue), feedURL: <redacted>)"
  }

  public var customMirror: Mirror {
    Mirror(self, children: ["source": "<redacted>"])
  }
}

public enum CredentialSourceKind: String, Sendable, Equatable {
  case systemStore
  case environment
}

public struct ProfileLimits: Sendable, Equatable {
  public let maxSummariesPerRefresh: UInt32
  public let maxDailyCostMicrousd: UInt64?
  public let inputCostMicrousdPerMillion: UInt64?
  public let outputCostMicrousdPerMillion: UInt64?
  public let maxOutputTokens: UInt32
  public let timeoutSeconds: UInt64
  public let maxRetries: UInt32

  public init(
    maxSummariesPerRefresh: UInt32,
    maxDailyCostMicrousd: UInt64?,
    inputCostMicrousdPerMillion: UInt64?,
    outputCostMicrousdPerMillion: UInt64?,
    maxOutputTokens: UInt32,
    timeoutSeconds: UInt64,
    maxRetries: UInt32
  ) {
    self.maxSummariesPerRefresh = maxSummariesPerRefresh
    self.maxDailyCostMicrousd = maxDailyCostMicrousd
    self.inputCostMicrousdPerMillion = inputCostMicrousdPerMillion
    self.outputCostMicrousdPerMillion = outputCostMicrousdPerMillion
    self.maxOutputTokens = maxOutputTokens
    self.timeoutSeconds = timeoutSeconds
    self.maxRetries = maxRetries
  }
}

public struct ModelProfile: Identifiable, Sendable, Equatable {
  public let id: String
  public let name: String
  public let provider: ProviderKind
  public let model: String
  public let endpoint: String?
  public let dialect: APIDialect?
  public let credentialSource: CredentialSourceKind
  public let consentedAt: Date?
  public let enabled: Bool
  public let limits: ProfileLimits
  public let createdAt: Date?
  public let updatedAt: Date?

  public init(
    id: String,
    name: String,
    provider: ProviderKind,
    model: String,
    endpoint: String?,
    dialect: APIDialect?,
    credentialSource: CredentialSourceKind,
    consentedAt: Date?,
    enabled: Bool,
    limits: ProfileLimits,
    createdAt: Date?,
    updatedAt: Date?
  ) {
    self.id = id
    self.name = name
    self.provider = provider
    self.model = model
    self.endpoint = endpoint
    self.dialect = dialect
    self.credentialSource = credentialSource
    self.consentedAt = consentedAt
    self.enabled = enabled
    self.limits = limits
    self.createdAt = createdAt
    self.updatedAt = updatedAt
  }
}

public struct AppSnapshot: Sendable, Equatable {
  public let revision: StateRevision
  public let status: CollectionStatus
  public let today: Briefing?
  public let latest: [Story]
  public let saved: [Story]
  public let sources: [Source]
  public let modelProfiles: [ModelProfile]
  public let defaultModelProfileID: String?
  public let hasUsableAIProfile: Bool

  public init(
    revision: StateRevision,
    status: CollectionStatus,
    today: Briefing?,
    latest: [Story],
    saved: [Story],
    sources: [Source],
    modelProfiles: [ModelProfile],
    defaultModelProfileID: String?,
    hasUsableAIProfile: Bool
  ) {
    self.revision = revision
    self.status = status
    self.today = today
    self.latest = latest
    self.saved = saved
    self.sources = sources
    self.modelProfiles = modelProfiles
    self.defaultModelProfileID = defaultModelProfileID
    self.hasUsableAIProfile = hasUsableAIProfile
  }

  public func containsStory(id: String) -> Bool {
    today?.items.contains(where: { $0.story.id == id }) == true
      || latest.contains(where: { $0.id == id })
      || saved.contains(where: { $0.id == id })
  }
}

public struct StoryMutationResult: Sendable, Equatable {
  public let story: Story
  public let revision: StateRevision

  public init(story: Story, revision: StateRevision) {
    self.story = story
    self.revision = revision
  }
}

public struct SourceMutationResult: Sendable, Equatable {
  public let source: Source
  public let revision: StateRevision

  public init(source: Source, revision: StateRevision) {
    self.source = source
    self.revision = revision
  }
}

public struct FeedSourceInput: Sendable, Equatable, CustomDebugStringConvertible, CustomReflectable {
  public let name: String
  public let category: String
  public let url: String
  public let weight: Double
  public let enabled: Bool

  public init(name: String, category: String, url: String, weight: Double, enabled: Bool) {
    self.name = name
    self.category = category
    self.url = url
    self.weight = weight
    self.enabled = enabled
  }

  public var debugDescription: String {
    "FeedSourceInput(weight: \(weight), enabled: \(enabled), url: <redacted>)"
  }

  public var customMirror: Mirror {
    Mirror(self, children: ["source request": "<redacted>"])
  }
}

public enum ModelCredentialInput: Sendable, Equatable, CustomDebugStringConvertible {
  case systemStore(secret: String)
  case environment(variable: String)

  public var debugDescription: String {
    switch self {
    case .systemStore: "ModelCredentialInput.systemStore(<redacted>)"
    case .environment: "ModelCredentialInput.environment(<redacted variable name>)"
    }
  }
}

public struct ProfileLimitsInput: Sendable, Equatable {
  public let maxSummariesPerRefresh: UInt32
  public let maxDailyCostUSD: String?
  public let inputCostUSDPerMillion: String?
  public let outputCostUSDPerMillion: String?
  public let maxOutputTokens: UInt32
  public let timeoutSeconds: UInt64
  public let maxRetries: UInt32

  public init(
    maxSummariesPerRefresh: UInt32,
    maxDailyCostUSD: String?,
    inputCostUSDPerMillion: String?,
    outputCostUSDPerMillion: String?,
    maxOutputTokens: UInt32,
    timeoutSeconds: UInt64,
    maxRetries: UInt32
  ) {
    self.maxSummariesPerRefresh = maxSummariesPerRefresh
    self.maxDailyCostUSD = maxDailyCostUSD
    self.inputCostUSDPerMillion = inputCostUSDPerMillion
    self.outputCostUSDPerMillion = outputCostUSDPerMillion
    self.maxOutputTokens = maxOutputTokens
    self.timeoutSeconds = timeoutSeconds
    self.maxRetries = maxRetries
  }
}

public struct ModelProfileInput: Sendable, Equatable, CustomDebugStringConvertible {
  public let name: String
  public let provider: ProviderKind
  public let model: String
  public let endpoint: String?
  public let dialect: APIDialect?
  public let credential: ModelCredentialInput
  public let consentProviderDataSharing: Bool
  public let limits: ProfileLimitsInput

  public init(
    name: String,
    provider: ProviderKind,
    model: String,
    endpoint: String?,
    dialect: APIDialect?,
    credential: ModelCredentialInput,
    consentProviderDataSharing: Bool,
    limits: ProfileLimitsInput
  ) {
    self.name = name
    self.provider = provider
    self.model = model
    self.endpoint = endpoint
    self.dialect = dialect
    self.credential = credential
    self.consentProviderDataSharing = consentProviderDataSharing
    self.limits = limits
  }

  public var debugDescription: String {
    "ModelProfileInput(name: \(name.debugDescription), provider: \(provider.rawValue), model: \(model.debugDescription), credential: <redacted>)"
  }
}

public struct GenerationReport: Sendable, Equatable {
  public let eligible: UInt64
  public let generated: UInt64
  public let cacheHits: UInt64
  public let skippedCap: UInt64
  public let skippedBudget: UInt64
  public let missingCredentials: UInt64
  public let providerFailures: UInt64
  public let malformedOutputs: UInt64
  public let smartFallbacks: UInt64

  public init(
    eligible: UInt64,
    generated: UInt64,
    cacheHits: UInt64,
    skippedCap: UInt64,
    skippedBudget: UInt64,
    missingCredentials: UInt64,
    providerFailures: UInt64,
    malformedOutputs: UInt64,
    smartFallbacks: UInt64
  ) {
    self.eligible = eligible
    self.generated = generated
    self.cacheHits = cacheHits
    self.skippedCap = skippedCap
    self.skippedBudget = skippedBudget
    self.missingCredentials = missingCredentials
    self.providerFailures = providerFailures
    self.malformedOutputs = malformedOutputs
    self.smartFallbacks = smartFallbacks
  }
}

public struct RefreshResult: Sendable, Equatable {
  public let briefing: Briefing
  public let successfulSources: UInt64
  public let failedSources: UInt64
  public let generation: GenerationReport
  public let revision: StateRevision

  public init(
    briefing: Briefing,
    successfulSources: UInt64,
    failedSources: UInt64,
    generation: GenerationReport,
    revision: StateRevision
  ) {
    self.briefing = briefing
    self.successfulSources = successfulSources
    self.failedSources = failedSources
    self.generation = generation
    self.revision = revision
  }
}

public struct RefreshNotice: Sendable, Equatable {
  public let failedSources: UInt64
  public let providerFailures: UInt64
  public let malformedOutputs: UInt64
  public let smartFallbacks: UInt64

  public init(
    failedSources: UInt64,
    providerFailures: UInt64,
    malformedOutputs: UInt64,
    smartFallbacks: UInt64
  ) {
    self.failedSources = failedSources
    self.providerFailures = providerFailures
    self.malformedOutputs = malformedOutputs
    self.smartFallbacks = smartFallbacks
  }

  public init?(result: RefreshResult) {
    self.init(
      failedSources: result.failedSources,
      providerFailures: result.generation.providerFailures,
      malformedOutputs: result.generation.malformedOutputs,
      smartFallbacks: result.generation.smartFallbacks
    )
    guard failedSources > 0 || providerFailures > 0 || malformedOutputs > 0 || smartFallbacks > 0
    else { return nil }
  }
}

public struct GenerationResult: Sendable, Equatable {
  public let story: Story
  public let selectedSummary: SummaryVariant?
  public let revision: StateRevision

  public init(story: Story, selectedSummary: SummaryVariant?, revision: StateRevision) {
    self.story = story
    self.selectedSummary = selectedSummary
    self.revision = revision
  }
}

public struct ModelMutationResult: Sendable, Equatable {
  public let profile: ModelProfile
  public let revision: StateRevision

  public init(profile: ModelProfile, revision: StateRevision) {
    self.profile = profile
    self.revision = revision
  }
}

public struct ModelTestResult: Sendable, Equatable {
  public let profile: ModelProfile
  public let costMayApply: Bool
  public let revision: StateRevision

  public init(profile: ModelProfile, costMayApply: Bool, revision: StateRevision) {
    self.profile = profile
    self.costMayApply = costMayApply
    self.revision = revision
  }
}

public enum CredentialDeletionStatus: String, Sendable, Equatable {
  case deleted
  case notApplicable
  case deleteFailed
}

public struct ModelRemovalResult: Sendable, Equatable {
  public let profile: ModelProfile
  public let credentialDeletion: CredentialDeletionStatus
  public let revision: StateRevision

  public init(
    profile: ModelProfile,
    credentialDeletion: CredentialDeletionStatus,
    revision: StateRevision
  ) {
    self.profile = profile
    self.credentialDeletion = credentialDeletion
    self.revision = revision
  }
}
