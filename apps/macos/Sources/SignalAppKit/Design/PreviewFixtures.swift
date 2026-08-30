import Foundation

public enum AppPhaseKind: String, CaseIterable, Sendable, Equatable, Hashable {
  case loading
  case welcome
  case buildingFirstBriefing
  case startupFailure
  case empty
  case ready
  case refreshing
  case stale
  case offline
  case failure
}

extension AppPhase {
  public var kind: AppPhaseKind {
    switch self {
    case .loading: .loading
    case .welcome: .welcome
    case .buildingFirstBriefing: .buildingFirstBriefing
    case .startupFailure: .startupFailure
    case .empty: .empty
    case .ready: .ready
    case .refreshing: .refreshing
    case .stale: .stale
    case .offline: .offline
    case .failure: .failure
    }
  }
}

public struct PreviewFixture: Sendable, Equatable, Identifiable {
  public let id: String
  public let phase: AppPhase
  public let snapshot: AppSnapshot?
  public let selectedStoryID: String?
  public let appearance: SignalAppearance
  public let reduceTransparency: Bool
  public let increaseContrast: Bool

  public init(
    id: String,
    phase: AppPhase,
    snapshot: AppSnapshot?,
    selectedStoryID: String? = nil,
    appearance: SignalAppearance = .light,
    reduceTransparency: Bool = false,
    increaseContrast: Bool = false
  ) {
    self.id = id
    self.phase = phase
    self.snapshot = snapshot
    self.selectedStoryID = selectedStoryID
    self.appearance = appearance
    self.reduceTransparency = reduceTransparency
    self.increaseContrast = increaseContrast
  }

  public var phaseKind: AppPhaseKind { phase.kind }

  public var message: String? {
    switch phase {
    case .startupFailure(let message), .offline(let message), .failure(let message): message
    default: nil
    }
  }

  public var selectedStory: Story? {
    guard let selectedStoryID, let snapshot else { return nil }
    return snapshot.today?.items.first(where: { $0.story.id == selectedStoryID })?.story
      ?? snapshot.latest.first(where: { $0.id == selectedStoryID })
      ?? snapshot.saved.first(where: { $0.id == selectedStoryID })
  }
}

public enum PreviewFixtures {
  public static let referenceDate = Date(timeIntervalSince1970: 1_788_086_400)

  public static let welcome = PreviewFixture(id: "welcome", phase: .welcome, snapshot: nil)

  public static let empty = PreviewFixture(
    id: "empty",
    phase: .empty,
    snapshot: emptySnapshot
  )

  public static let populated = PreviewFixture(
    id: "populated",
    phase: .ready,
    snapshot: populatedSnapshot,
    selectedStoryID: aiStory.id
  )

  public static let selectedAI = PreviewFixture(
    id: "selected-ai",
    phase: .ready,
    snapshot: populatedSnapshot,
    selectedStoryID: aiStory.id
  )

  public static let smartFallback = PreviewFixture(
    id: "smart-fallback",
    phase: .ready,
    snapshot: smartSnapshot,
    selectedStoryID: smartStory.id
  )

  public static let stalePartialRefresh = PreviewFixture(
    id: "stale-partial-refresh",
    phase: .stale,
    snapshot: staleSnapshot,
    selectedStoryID: aiStory.id
  )

  public static let offlineCachedBriefing = PreviewFixture(
    id: "offline-cached-briefing",
    phase: .offline(message: "Offline. Showing the last cached briefing."),
    snapshot: populatedSnapshot,
    selectedStoryID: aiStory.id
  )

  public static let providerFailure = PreviewFixture(
    id: "provider-failure",
    phase: .failure(message: "AI provider unavailable. Smart summaries remain available."),
    snapshot: smartSnapshot,
    selectedStoryID: smartStory.id
  )

  public static let darkAppearance = PreviewFixture(
    id: "dark-appearance",
    phase: .ready,
    snapshot: populatedSnapshot,
    selectedStoryID: aiStory.id,
    appearance: .dark
  )

  public static let reducedTransparency = PreviewFixture(
    id: "reduced-transparency",
    phase: .ready,
    snapshot: populatedSnapshot,
    selectedStoryID: aiStory.id,
    reduceTransparency: true
  )

  public static let increasedContrast = PreviewFixture(
    id: "increased-contrast",
    phase: .ready,
    snapshot: populatedSnapshot,
    selectedStoryID: aiStory.id,
    increaseContrast: true
  )

  public static let loading = PreviewFixture(id: "loading", phase: .loading, snapshot: nil)
  public static let buildingFirstBriefing = PreviewFixture(
    id: "building-first-briefing",
    phase: .buildingFirstBriefing,
    snapshot: nil
  )
  public static let startupFailure = PreviewFixture(
    id: "startup-failure",
    phase: .startupFailure(message: "Local data is unavailable."),
    snapshot: nil
  )
  public static let refreshing = PreviewFixture(
    id: "refreshing",
    phase: .refreshing,
    snapshot: populatedSnapshot,
    selectedStoryID: aiStory.id
  )

  public static let all: [PreviewFixture] = [
    welcome,
    empty,
    populated,
    selectedAI,
    smartFallback,
    stalePartialRefresh,
    offlineCachedBriefing,
    providerFailure,
    darkAppearance,
    reducedTransparency,
    increasedContrast,
    loading,
    buildingFirstBriefing,
    startupFailure,
    refreshing,
  ]

  private static let generatedAt = referenceDate.addingTimeInterval(-900)
  private static let publishedAt = referenceDate.addingTimeInterval(-3_600)

  private static let aiVariant = SummaryVariant(
    id: "preview-variant-ai",
    storyID: "preview-story-ai",
    profileID: "preview-profile-local",
    provider: .openAI,
    model: "editorial-summary-v1",
    dialect: .responses,
    fields: SummaryFields(
      whatHappened:
        "A research group released a reproducible evaluation for small reasoning models.",
      whyItMatters:
        "The benchmark makes local model trade-offs easier to compare without paid APIs.",
      caveat: "The initial results cover a limited set of hardware configurations."
    ),
    generatedAt: generatedAt
  )

  private static let aiStory = Story(
    id: "preview-story-ai",
    title: "A reproducible benchmark sharpens the case for smaller reasoning models",
    canonicalURL: "https://research.example.test/small-model-benchmark",
    excerpt: "The evaluation compares accuracy, latency, and memory on consumer hardware.",
    category: "Research",
    publishedAt: publishedAt,
    sourceIDs: ["preview-source-research"],
    score: Score(recency: 0.93, sourceWeight: 0.86, corroboration: 0.71, total: 0.87),
    smartSummary: "A new benchmark compares small reasoning models on consumer hardware.",
    isRead: false,
    isSaved: true,
    selectedSummary: aiVariant,
    summaryVariants: [aiVariant]
  )

  private static let smartStory = Story(
    id: "preview-story-smart",
    title: "Open tooling makes local inference measurements easier to reproduce",
    canonicalURL: "https://engineering.example.test/local-inference-tooling",
    excerpt: "The project publishes measurement scripts and documented test fixtures.",
    category: "Tools",
    publishedAt: publishedAt.addingTimeInterval(-1_800),
    sourceIDs: ["preview-source-engineering"],
    score: Score(recency: 0.82, sourceWeight: 0.79, corroboration: 0.62, total: 0.76),
    smartSummary: "Open scripts make local inference measurements reproducible.",
    isRead: true,
    isSaved: false,
    selectedSummary: nil,
    summaryVariants: []
  )

  private static let sources = [
    Source(
      id: "preview-source-research",
      name: "Research Notes",
      category: "Research",
      enabled: true,
      weight: 0.86,
      feedURL: "https://research.example.test/feed.xml",
      origin: .standard
    ),
    Source(
      id: "preview-source-engineering",
      name: "Engineering Ledger",
      category: "Tools",
      enabled: true,
      weight: 0.79,
      feedURL: "https://engineering.example.test/atom.xml",
      origin: .personal
    ),
  ]

  private static let profile = ModelProfile(
    id: "preview-profile-local",
    name: "Editorial summaries",
    provider: .openAI,
    model: "editorial-summary-v1",
    endpoint: nil,
    dialect: .responses,
    credentialSource: .systemStore,
    consentedAt: referenceDate.addingTimeInterval(-86_400),
    enabled: true,
    limits: ProfileLimits(
      maxSummariesPerRefresh: 4,
      maxDailyCostMicrousd: 25_000,
      inputCostMicrousdPerMillion: 150_000,
      outputCostMicrousdPerMillion: 600_000,
      maxOutputTokens: 384,
      timeoutSeconds: 30,
      maxRetries: 2
    ),
    createdAt: referenceDate.addingTimeInterval(-172_800),
    updatedAt: referenceDate.addingTimeInterval(-86_400)
  )

  private static let populatedSnapshot = snapshot(
    stories: [aiStory, smartStory],
    isStale: false,
    hasAI: true
  )
  private static let smartSnapshot = snapshot(
    stories: [smartStory],
    isStale: false,
    hasAI: false
  )
  private static let staleSnapshot = snapshot(
    stories: [aiStory, smartStory],
    isStale: true,
    hasAI: true
  )
  private static let emptySnapshot = AppSnapshot(
    revision: StateRevision(dataGeneration: 40, sourceConfigRevision: "preview-sources-v1"),
    status: CollectionStatus(
      state: .ready,
      refresh: RefreshMetadata(lastRefreshAt: generatedAt, storyCount: 0)
    ),
    today: Briefing(date: "2026-08-30", generatedAt: generatedAt, isStale: false, items: []),
    latest: [],
    saved: [],
    sources: sources,
    modelProfiles: [],
    defaultModelProfileID: nil,
    hasUsableAIProfile: false
  )

  private static func snapshot(stories: [Story], isStale: Bool, hasAI: Bool) -> AppSnapshot {
    let items = stories.enumerated().map { index, story in
      BriefingItem(
        position: UInt32(index + 1),
        section: index == 0 ? "Top Signals" : "Worth Watching",
        isStale: isStale && index == 0,
        story: story,
        selectedSummary: story.selectedSummary,
        summaryVariants: story.summaryVariants
      )
    }
    return AppSnapshot(
      revision: StateRevision(dataGeneration: 42, sourceConfigRevision: "preview-sources-v1"),
      status: CollectionStatus(
        state: .ready,
        refresh: RefreshMetadata(lastRefreshAt: generatedAt, storyCount: UInt64(stories.count))
      ),
      today: Briefing(
        date: "2026-08-30",
        generatedAt: generatedAt,
        isStale: isStale,
        items: items
      ),
      latest: stories,
      saved: stories.filter(\.isSaved),
      sources: sources,
      modelProfiles: hasAI ? [profile] : [],
      defaultModelProfileID: hasAI ? profile.id : nil,
      hasUsableAIProfile: hasAI
    )
  }
}

public enum PreviewFixtureSecurityAudit {
  private static let forbiddenLabels: Set<String> = [
    "secret", "keychainsecret", "credentialreference", "apikey", "password",
    "rawproviderbody", "localpath",
  ]
  private static let secretMarkers = [
    "sk-", "authorization: bearer ", "-----begin private key-----",
  ]

  public static func findings<T>(in value: T) -> [String] {
    var findings: [String] = []
    inspect(value, path: "fixture", findings: &findings)
    return findings
  }

  private static func inspect(_ value: Any, path: String, findings: inout [String]) {
    let mirror = Mirror(reflecting: value)
    if mirror.displayStyle == .optional {
      if let child = mirror.children.first {
        inspect(child.value, path: path, findings: &findings)
      }
      return
    }
    if let text = value as? String {
      let normalized = text.lowercased()
      if let marker = secretMarkers.first(where: normalized.contains) {
        findings.append("\(path) contains secret marker \(marker)")
      }
      return
    }
    for (index, child) in mirror.children.enumerated() {
      let label = child.label ?? "\(index)"
      let normalizedLabel = label.lowercased().filter(\.isLetter)
      let childPath = "\(path).\(label)"
      if forbiddenLabels.contains(normalizedLabel) {
        findings.append("\(childPath) is a forbidden secret-bearing field")
      }
      inspect(child.value, path: childPath, findings: &findings)
    }
  }
}
