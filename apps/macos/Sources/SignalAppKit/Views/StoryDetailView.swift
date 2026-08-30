import AppKit
import SwiftUI

public enum StoryDetailElement: Sendable, Hashable {
  case metadata
  case title
  case provenance
  case originalExcerpt
  case whatHappened
  case whyItMatters
  case caveat
  case scoreAndSources
  case actions
}

public struct StoryDetailPresentation: Sendable, Equatable {
  public let storyID: String
  public let title: String
  public let metadata: String
  public let stateLabels: [String]
  public let accessibilityMetadata: String
  public let isStale: Bool
  public let provenance: SummaryProvenance
  public let originalExcerpt: String?
  public let whatHappened: String?
  public let whyItMatters: String?
  public let caveat: String?
  public let scoreExplanation: String
  public let sourceNames: [String]
  public let elements: [StoryDetailElement]

  public init(
    story: Story,
    sourceNames: [String],
    isStale: Bool,
    selection: ReadingSummarySelection,
    relativeTo reference: Date = .now
  ) {
    storyID = story.id
    title = story.title
    let source = sourceNames.first ?? "Unknown source"
    metadata = [
      source,
      SignalFormatters.relativeDate(story.publishedAt, relativeTo: reference),
      story.category,
    ].joined(separator: " · ")
    stateLabels = [story.isRead ? "Read" : "Unread", story.isSaved ? "Saved" : "Not saved"]
    accessibilityMetadata = ([metadata] + stateLabels).joined(separator: ", ")
    self.isStale = isStale
    self.sourceNames = sourceNames
    scoreExplanation = String(
      format: "Recency %.2f · source weight %.2f · corroboration %.2f · total %.2f",
      story.score.recency,
      story.score.sourceWeight,
      story.score.corroboration,
      story.score.total
    )

    var ordered: [StoryDetailElement] = [.metadata, .title, .provenance]
    switch selection {
    case .raw:
      provenance = .raw
      originalExcerpt = story.excerpt
      whatHappened = nil
      whyItMatters = nil
      caveat = nil
      ordered.append(.originalExcerpt)
    case .smart:
      provenance = .smart
      originalExcerpt = nil
      whatHappened = story.smartSummary
      whyItMatters = nil
      caveat = nil
      ordered.append(.whatHappened)
    case .ai(let variantID):
      if let variant = story.summaryVariants.first(where: { $0.id == variantID }) {
        provenance = .ai(provider: variant.provider.readerDisplayName, model: variant.model)
        originalExcerpt = nil
        whatHappened = variant.fields.whatHappened
        whyItMatters = variant.fields.whyItMatters
        caveat = variant.fields.caveat
        ordered.append(contentsOf: [.whatHappened, .whyItMatters])
        if variant.fields.caveat != nil { ordered.append(.caveat) }
      } else {
        provenance = .smart
        originalExcerpt = nil
        whatHappened = story.smartSummary
        whyItMatters = nil
        caveat = nil
        ordered.append(.whatHappened)
      }
    }
    ordered.append(contentsOf: [.scoreAndSources, .actions])
    elements = ordered
  }
}

public enum StorySourceURL {
  public static func validated(_ value: String) -> URL? {
    guard let components = URLComponents(string: value),
      let scheme = components.scheme?.lowercased(),
      scheme == "http" || scheme == "https",
      components.host?.isEmpty == false,
      let url = components.url
    else { return nil }
    return url
  }
}

public struct StorySourceActionPresentation: Sendable, Equatable {
  public let url: URL?
  public var isEnabled: Bool { url != nil }

  public init(story: Story) {
    url = StorySourceURL.validated(story.canonicalURL)
  }
}

public struct StorySaveTogglePresentation: Sendable, Equatable {
  public let title: String
  public let help: String

  public init(isSaved: Bool) {
    title = isSaved ? "Remove from Saved" : "Save"
    help = isSaved ? "Remove this story from Saved" : "Save this story"
  }
}

public struct StoryDetailView: View {
  @Bindable private var model: AppModel

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    ScrollView {
      if let story = model.selectedStory {
        ExpandedStoryView(story: story, model: model)
          .padding(.horizontal, 36)
          .padding(.vertical, 32)
          .frame(maxWidth: .infinity)
      } else {
        ContentUnavailableView(
          "Select a story",
          systemImage: "text.page",
          description: Text("Choose a signal from the list to read it here.")
        )
        .frame(maxWidth: .infinity, minHeight: 360)
      }
    }
    .background(Color(nsColor: .textBackgroundColor))
  }
}
