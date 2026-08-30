import Foundation

@testable import SignalAppKit

final class FakeBridgeClient: BridgeClient, @unchecked Sendable {
  private let lock = NSLock()
  private var snapshots: [AppSnapshot]
  private var revisions: [StateRevision]
  private var snapshotIndex = 0
  private var revisionIndex = 0
  private var pendingRefreshes: [CheckedContinuation<RefreshResult, any Error>] = []
  private var storedRefreshIdentifiers: [String] = []
  private var storedCancelledIdentifiers: [String] = []
  private var storedSnapshotCalls = 0
  private var storedModelInputs: [ModelProfileInput] = []

  var snapshotError: (any Error)?
  var refreshError: (any Error)?
  var modelError: (any Error)?

  init(snapshot: AppSnapshot, revisions: [StateRevision] = []) {
    snapshots = [snapshot]
    self.revisions = revisions.isEmpty ? [snapshot.revision] : revisions
  }

  var refreshIdentifiers: [String] {
    lock.withLock { storedRefreshIdentifiers }
  }

  var cancelledIdentifiers: [String] {
    lock.withLock { storedCancelledIdentifiers }
  }

  var snapshotCalls: Int {
    lock.withLock { storedSnapshotCalls }
  }

  var modelInputs: [ModelProfileInput] {
    lock.withLock { storedModelInputs }
  }

  func enqueueSnapshot(_ snapshot: AppSnapshot) {
    lock.withLock { snapshots.append(snapshot) }
  }

  func snapshot() async throws -> AppSnapshot {
    try lock.withLock {
      storedSnapshotCalls += 1
      if let snapshotError { throw snapshotError }
      let result = snapshots[min(snapshotIndex, snapshots.count - 1)]
      snapshotIndex += 1
      return result
    }
  }

  func stateRevision() async throws -> StateRevision {
    lock.withLock {
      let result = revisions[min(revisionIndex, revisions.count - 1)]
      revisionIndex += 1
      return result
    }
  }

  func refresh(operationID: String, ai: Bool) async throws -> RefreshResult {
    if let error = lock.withLock({ () -> (any Error)? in
      storedRefreshIdentifiers.append(operationID)
      return refreshError
    }) {
      throw error
    }
    return try await withCheckedThrowingContinuation { continuation in
      lock.withLock { pendingRefreshes.append(continuation) }
    }
  }

  func finishRefresh(with result: RefreshResult) {
    let continuations = lock.withLock { () -> [CheckedContinuation<RefreshResult, any Error>] in
      defer { pendingRefreshes.removeAll() }
      return pendingRefreshes
    }
    for continuation in continuations {
      continuation.resume(returning: result)
    }
  }

  func cancelOperation(id: String) -> Bool {
    let (matched, continuations) = lock.withLock {
      () -> (Bool, [CheckedContinuation<RefreshResult, any Error>]) in
      storedCancelledIdentifiers.append(id)
      let matched = storedRefreshIdentifiers.contains(id)
      guard matched else { return (false, []) }
      defer { pendingRefreshes.removeAll() }
      return (true, pendingRefreshes)
    }
    for continuation in continuations {
      continuation.resume(throwing: BridgeError.cancelled)
    }
    return matched
  }

  func setSaved(storyID: String, saved: Bool) async throws -> Story { .fixture }
  func setRead(storyID: String, read: Bool) async throws -> Story { .fixture }
  func selectSummary(storyID: String, variantID: String) async throws -> SummaryVariant { .fixture }
  func regenerate(storyID: String, profile: String?, force: Bool) async throws -> GenerationResult {
    GenerationResult(story: .fixture, selectedSummary: .fixture, revision: .fixture)
  }
  func addSource(_ input: FeedSourceInput) async throws -> Source { .fixture }
  func setSourceEnabled(id: String, enabled: Bool) async throws -> Source { .fixture }
  func removeSource(id: String) async throws -> Source { .fixture }

  func addModel(_ input: ModelProfileInput) async throws -> ModelProfile {
    if let error = lock.withLock({ () -> (any Error)? in
      storedModelInputs.append(input)
      return modelError
    }) {
      throw error
    }
    return .fixture
  }

  func setDefaultModel(_ selector: String) async throws -> ModelProfile { .fixture }
  func testModel(_ selector: String) async throws -> ModelTestResult {
    ModelTestResult(profile: .fixture, costMayApply: true)
  }
  func removeModel(_ selector: String) async throws -> ModelRemovalResult {
    ModelRemovalResult(profile: .fixture, credentialDeletion: .deleted)
  }
}

struct DetailedFakeError: Error, CustomStringConvertible, Sendable {
  let description: String
}

extension StateRevision {
  static let fixture = StateRevision(dataGeneration: 1, sourceConfigRevision: "source-a")
}

extension SummaryVariant {
  static let fixture = SummaryVariant(
    id: "variant-1",
    storyID: "story-1",
    profileID: "profile-1",
    provider: .openAI,
    model: "model-1",
    dialect: .responses,
    fields: SummaryFields(whatHappened: "What", whyItMatters: "Why", caveat: nil),
    generatedAt: Date(timeIntervalSince1970: 1_700_000_000)
  )
}

extension Story {
  static let fixture = Story(
    id: "story-1",
    title: "A signal",
    canonicalURL: "https://example.test/story",
    excerpt: "Original text",
    category: "research",
    publishedAt: Date(timeIntervalSince1970: 1_700_000_000),
    sourceIDs: ["source-1"],
    score: Score(recency: 1, sourceWeight: 0.8, corroboration: 0.5, total: 2.3),
    smartSummary: "Smart summary",
    isRead: false,
    isSaved: false,
    selectedSummary: .fixture,
    summaryVariants: [.fixture]
  )
}

extension Source {
  static let fixture = Source(
    id: "source-1",
    name: "Example",
    category: "research",
    enabled: true,
    weight: 0.8,
    feedURL: "https://example.test/feed.xml",
    origin: .standard
  )
}

extension ModelProfile {
  static let fixture = ModelProfile(
    id: "profile-1",
    name: "Example model",
    provider: .openAI,
    model: "model-1",
    endpoint: nil,
    dialect: .responses,
    credentialSource: .systemStore,
    consentedAt: Date(timeIntervalSince1970: 1_700_000_000),
    enabled: true,
    limits: ProfileLimits(
      maxSummariesPerRefresh: 5,
      maxDailyCostMicrousd: nil,
      inputCostMicrousdPerMillion: nil,
      outputCostMicrousdPerMillion: nil,
      maxOutputTokens: 384,
      timeoutSeconds: 30,
      maxRetries: 2
    ),
    createdAt: Date(timeIntervalSince1970: 1_700_000_000),
    updatedAt: Date(timeIntervalSince1970: 1_700_000_000)
  )
}

extension AppSnapshot {
  static let fixture = AppSnapshot(
    revision: .fixture,
    status: CollectionStatus(
      state: .ready,
      refresh: RefreshMetadata(
        lastRefreshAt: Date(timeIntervalSince1970: 1_700_000_000),
        storyCount: 1
      )
    ),
    today: Briefing(
      date: "2026-08-30",
      generatedAt: Date(timeIntervalSince1970: 1_700_000_000),
      isStale: false,
      items: [
        BriefingItem(
          position: 1,
          section: "Top Signals",
          isStale: false,
          story: .fixture,
          selectedSummary: .fixture,
          summaryVariants: [.fixture]
        )
      ]
    ),
    latest: [.fixture],
    saved: [],
    sources: [.fixture],
    modelProfiles: [.fixture],
    defaultModelProfileID: "profile-1",
    hasUsableAIProfile: true
  )
}

extension RefreshResult {
  static let fixture = RefreshResult(
    briefing: AppSnapshot.fixture.today!,
    successfulSources: 1,
    failedSources: 0,
    generation: GenerationReport(
      eligible: 1,
      generated: 1,
      cacheHits: 0,
      skippedCap: 0,
      skippedBudget: 0,
      missingCredentials: 0,
      providerFailures: 0,
      malformedOutputs: 0,
      smartFallbacks: 0
    ),
    revision: .fixture
  )
}
