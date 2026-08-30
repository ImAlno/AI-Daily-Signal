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
  public let bodyElements: [StoryDetailElement]

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
    bodyElements = ordered.filter { element in
      switch element {
      case .metadata, .title, .provenance: false
      case .originalExcerpt, .whatHappened, .whyItMatters, .caveat, .scoreAndSources, .actions:
        true
      }
    }
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
  @State private var isHovered = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @Environment(\.dynamicTypeSize) private var dynamicTypeSize

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    GeometryReader { proxy in
      ScrollView {
        if let story = model.selectedStory {
          let row = rowPresentation(for: story)
          let header = StoryHeaderPresentation(
            row: row,
            isExpanded: true,
            isHovered: isHovered,
            dynamicTypeSize: dynamicTypeSize
          )
          let transition = ReaderMotionPresentation(reduceMotion: reduceMotion).duration.map {
            Animation.easeOut(duration: $0)
          }

          VStack(alignment: .leading, spacing: 0) {
            StoryHeaderView(presentation: header) {
              model.selectedStoryID = nil
            }
            .animation(transition, value: header.isHovered)
            .onHover { isHovered = $0 }
            SummaryVariantPicker(story: story, model: model)
              .padding(.horizontal, 12)
              .padding(.top, 8)
            ExpandedStoryView(story: story, model: model)
              .padding(.horizontal, 12)
              .padding(.top, 18)
              .padding(.bottom, 22)
          }
          .frame(maxWidth: ReadingColumnMetrics.maximumWidth, alignment: .leading)
          .padding(.horizontal, ReadingColumnMetrics.horizontalPadding(for: proxy.size.width))
          .padding(.vertical, 30)
          .frame(maxWidth: .infinity, alignment: .center)
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

  private func rowPresentation(for story: Story) -> StoryRowPresentation {
    StoryRowPresentation(
      story: story,
      primarySource: ReaderPresentationSupport.primarySource(
        for: story,
        sources: model.snapshot?.sources ?? []
      ),
      relativeTime: SignalFormatters.relativeDate(story.publishedAt),
      isStale: model.isStoryStale(id: story.id),
      rank: model.destination == .today
        ? model.snapshot?.today?.items.first(where: { $0.story.id == story.id })?.position
        : nil,
      summarySelection: model.summarySelection(for: story.id)
    )
  }
}
