import Foundation

public enum ReadingSummarySelection: Sendable, Hashable {
  case raw
  case smart
  case ai(variantID: String)
}

public enum SummaryProvenance: Sendable, Equatable {
  case raw
  case smart
  case ai(provider: String, model: String)

  public var shortLabel: String {
    switch self {
    case .raw: "Original excerpt"
    case .smart: "Smart · local algorithmic summary"
    case .ai(let provider, let model): "\(provider) · \(model)"
    }
  }

  public var accessibilityLabel: String {
    switch self {
    case .raw: "Original source excerpt"
    case .smart: "Smart local algorithmic summary"
    case .ai(let provider, let model): "AI-generated summary from \(provider), model \(model)"
    }
  }
}

public struct StoryRowPresentation: Identifiable, Sendable, Equatable {
  public var id: String { storyID }
  public let storyID: String
  public let title: String
  public let primarySource: String
  public let relativeTime: String
  public let category: String
  public let isStale: Bool
  public let isSaved: Bool
  public let isRead: Bool
  public let rank: UInt32?
  public let provenance: SummaryProvenance

  public init(
    story: Story,
    primarySource: String,
    relativeTime: String,
    isStale: Bool,
    rank: UInt32?,
    summarySelection: ReadingSummarySelection
  ) {
    storyID = story.id
    title = story.title
    self.primarySource = primarySource
    self.relativeTime = relativeTime
    category = story.category
    self.isStale = isStale
    isSaved = story.isSaved
    isRead = story.isRead
    self.rank = rank
    provenance = SummaryProvenance(selection: summarySelection, story: story)
  }

  public var accessibilitySummary: String {
    var values = [
      title,
      primarySource,
      relativeTime,
      category,
      provenance.accessibilityLabel,
    ]
    if let rank { values.insert("Rank \(rank)", at: 0) }
    if isStale { values.append("stale") }
    if isSaved { values.append("saved") }
    if isRead { values.append("read") }
    return values.joined(separator: ", ")
  }
}

extension SummaryProvenance {
  fileprivate init(selection: ReadingSummarySelection, story: Story) {
    switch selection {
    case .raw:
      self = .raw
    case .smart:
      self = .smart
    case .ai(let variantID):
      guard let variant = story.summaryVariants.first(where: { $0.id == variantID }) else {
        self = .smart
        return
      }
      self = .ai(provider: variant.provider.readerDisplayName, model: variant.model)
    }
  }
}

extension ProviderKind {
  public var readerDisplayName: String {
    switch self {
    case .openAI: "OpenAI"
    case .anthropic: "Anthropic"
    case .gemini: "Google Gemini"
    case .openAICompatible: "OpenAI-compatible"
    }
  }
}
