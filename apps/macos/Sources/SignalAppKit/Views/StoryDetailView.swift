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
    Group {
      if let story = model.selectedStory {
        detail(story)
      } else {
        ContentUnavailableView(
          "Select a story",
          systemImage: "text.page",
          description: Text("Choose a signal from the list to read it here.")
        )
      }
    }
    .background(Color(nsColor: .textBackgroundColor))
  }

  private func detail(_ story: Story) -> some View {
    let sources = model.snapshot?.sources ?? []
    let presentation = StoryDetailPresentation(
      story: story,
      sourceNames: ReaderPresentationSupport.sourceNames(for: story, sources: sources),
      isStale: model.isStoryStale(id: story.id),
      selection: model.summarySelection(for: story.id)
    )

    return SignalReadingSurface {
      ScrollView {
        VStack(alignment: .leading, spacing: 0) {
          if let error = model.storyActionError {
            Label(error, systemImage: "exclamationmark.triangle")
              .font(.callout)
              .foregroundStyle(.red)
              .padding(.bottom, 20)
              .accessibilityLabel("Story action failed. \(error)")
              .accessibilitySortPriority(AccessibilityOrder.status.sortPriority)
          }
          ForEach(presentation.elements, id: \.self) { element in
            detailElement(element, presentation: presentation, story: story)
          }
        }
        .frame(maxWidth: 720, alignment: .leading)
        .padding(.horizontal, 42)
        .padding(.vertical, 36)
        .frame(maxWidth: .infinity, alignment: .center)
      }
    }
  }

  @ViewBuilder
  private func detailElement(
    _ element: StoryDetailElement,
    presentation: StoryDetailPresentation,
    story: Story
  ) -> some View {
    switch element {
    case .metadata:
      ViewThatFits(in: .horizontal) {
        metadataContent(presentation, axis: .horizontal)
        metadataContent(presentation, axis: .vertical)
      }
      .font(.callout)
      .foregroundStyle(.secondary)
      .accessibilityElement(children: .ignore)
      .accessibilityLabel(presentation.accessibilityMetadata)
      .accessibilitySortPriority(AccessibilityOrder.status.sortPriority)
      .padding(.bottom, 12)
    case .title:
      Text(presentation.title)
        .font(.system(.largeTitle, design: .serif, weight: .semibold))
        .textSelection(.enabled)
        .accessibilityAddTraits(.isHeader)
        .accessibilitySortPriority(AccessibilityOrder.title.sortPriority)
        .padding(.bottom, 18)
    case .provenance:
      Text(presentation.provenance.shortLabel)
        .font(.caption)
        .fontWeight(.medium)
        .padding(.horizontal, 10)
        .padding(.vertical, 5)
        .background(Color(nsColor: .controlBackgroundColor), in: Capsule())
        .overlay(Capsule().stroke(Color(nsColor: .separatorColor), lineWidth: 0.5))
        .accessibilityLabel(presentation.provenance.accessibilityLabel)
        .accessibilitySortPriority(AccessibilityOrder.status.sortPriority)
        .padding(.bottom, 28)
    case .originalExcerpt:
      readingSection("Original excerpt", body: presentation.originalExcerpt)
    case .whatHappened:
      readingSection("What happened", body: presentation.whatHappened)
    case .whyItMatters:
      readingSection("Why it matters", body: presentation.whyItMatters)
    case .caveat:
      readingSection("Caveat", body: presentation.caveat)
    case .scoreAndSources:
      DisclosureGroup("Why this ranked here") {
        VStack(alignment: .leading, spacing: 8) {
          Text(presentation.scoreExplanation)
          Text("Sources: \(presentation.sourceNames.joined(separator: ", "))")
        }
        .font(.callout)
        .foregroundStyle(.secondary)
        .padding(.top, 8)
      }
      .font(.callout)
      .accessibilitySortPriority(AccessibilityOrder.content.sortPriority)
      .padding(.top, 4)
      .padding(.bottom, 28)
    case .actions:
      Divider()
        .padding(.bottom, 18)
      SummaryVariantPicker(story: story, model: model)
        .frame(maxWidth: 340, alignment: .leading)
        .padding(.bottom, 14)
      ViewThatFits(in: .horizontal) {
        actionContent(story, axis: .horizontal)
        actionContent(story, axis: .vertical)
      }
      .controlSize(.regular)
      .accessibilitySortPriority(AccessibilityOrder.actions.sortPriority)
    }
  }

  @ViewBuilder
  private func metadataContent(
    _ presentation: StoryDetailPresentation,
    axis: Axis
  ) -> some View {
    let content = Group {
      Text(presentation.metadata)
        .fixedSize(horizontal: false, vertical: true)
      ForEach(presentation.stateLabels, id: \.self) { label in
        Text(label)
      }
      if presentation.isStale {
        Label("Stale", systemImage: "clock.badge.exclamationmark")
          .foregroundStyle(.orange)
      }
    }
    if axis == .horizontal {
      HStack(spacing: 8) { content }
    } else {
      VStack(alignment: .leading, spacing: 5) { content }
    }
  }

  @ViewBuilder
  private func actionContent(_ story: Story, axis: Axis) -> some View {
    let source = StorySourceActionPresentation(story: story)
    let save = StorySaveTogglePresentation(isSaved: story.isSaved)
    let content = Group {
      Button("Open Source", systemImage: "safari") {
        if let url = source.url {
          NSWorkspace.shared.open(url)
        }
      }
      .keyboardShortcut("o", modifiers: .command)
      .disabled(!source.isEnabled)
      .help(ReadingCommand.openSource.descriptor.help)

      Button(save.title, systemImage: "bookmark") {
        Task { await model.toggleSelectedStorySaved() }
      }
      .disabled(model.storyActionState(for: .saving(storyID: story.id)) != nil)
      .help(save.help)

      Button(story.isRead ? "Mark Unread" : "Mark Read", systemImage: "checkmark.circle") {
        Task { await model.toggleSelectedStoryRead() }
      }
      .disabled(model.storyActionState(for: .markingRead(storyID: story.id)) != nil)

      GenerationPopover(model: model, story: story)
    }
    if axis == .horizontal {
      HStack(spacing: 10) { content }
    } else {
      VStack(alignment: .leading, spacing: 10) { content }
    }
  }

  @ViewBuilder
  private func readingSection(_ title: String, body: String?) -> some View {
    if let body, !body.isEmpty {
      VStack(alignment: .leading, spacing: 8) {
        Text(title)
          .font(.title3)
          .fontWeight(.semibold)
          .accessibilityAddTraits(.isHeader)
        Text(body)
          .font(.body)
          .lineSpacing(4)
          .textSelection(.enabled)
      }
      .padding(.bottom, 26)
      .accessibilitySortPriority(AccessibilityOrder.content.sortPriority)
    }
  }
}
