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

    return ScrollView {
      VStack(alignment: .leading, spacing: 0) {
        if let error = model.storyActionError {
          Label(error, systemImage: "exclamationmark.triangle")
            .font(.callout)
            .foregroundStyle(.red)
            .padding(.bottom, 20)
            .accessibilityLabel("Story action failed. \(error)")
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
    .background(Color(nsColor: .textBackgroundColor))
  }

  @ViewBuilder
  private func detailElement(
    _ element: StoryDetailElement,
    presentation: StoryDetailPresentation,
    story: Story
  ) -> some View {
    switch element {
    case .metadata:
      HStack(spacing: 8) {
        Text(presentation.metadata)
        if presentation.isStale {
          Label("Stale", systemImage: "clock.badge.exclamationmark")
            .foregroundStyle(.orange)
        }
      }
      .font(.callout)
      .foregroundStyle(.secondary)
      .padding(.bottom, 12)
    case .title:
      Text(presentation.title)
        .font(.system(.largeTitle, design: .serif, weight: .semibold))
        .textSelection(.enabled)
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
      .padding(.top, 4)
      .padding(.bottom, 28)
    case .actions:
      Divider()
        .padding(.bottom, 18)
      SummaryVariantPicker(story: story, model: model)
        .frame(maxWidth: 340, alignment: .leading)
        .padding(.bottom, 14)
      HStack(spacing: 10) {
        Button("Open Source", systemImage: "safari") {
          if let url = StorySourceURL.validated(story.canonicalURL) {
            NSWorkspace.shared.open(url)
          }
        }
        .keyboardShortcut("o", modifiers: .command)

        Button(story.isSaved ? "Remove from Saved" : "Save", systemImage: "bookmark") {
          Task { await model.toggleSelectedStorySaved() }
        }
        .disabled(model.activeStoryAction == .saving(storyID: story.id))

        Button(story.isRead ? "Mark Unread" : "Mark Read", systemImage: "checkmark.circle") {
          Task { await model.toggleSelectedStoryRead() }
        }
        .disabled(model.activeStoryAction == .markingRead(storyID: story.id))

        GenerationPopover(model: model, story: story)
      }
      .controlSize(.regular)
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
    }
  }
}
