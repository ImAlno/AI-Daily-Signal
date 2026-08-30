import AppKit
import SwiftUI

public struct ExpandedStoryView: View {
  private let story: Story
  @Bindable private var model: AppModel
  @ScaledMetric(relativeTo: .subheadline) private var sectionLabelSize = 13.0
  @ScaledMetric(relativeTo: .body) private var bodySize = 15.0
  @ScaledMetric(relativeTo: .body) private var bodyLineSpacing = 5.0

  public init(story: Story, model: AppModel) {
    self.story = story
    self.model = model
  }

  public var body: some View {
    let presentation = StoryDetailPresentation(
      story: story,
      sourceNames: ReaderPresentationSupport.sourceNames(
        for: story,
        sources: model.snapshot?.sources ?? []
      ),
      isStale: model.isStoryStale(id: story.id),
      selection: model.summarySelection(for: story.id)
    )

    VStack(alignment: .leading, spacing: 0) {
      if let error = model.storyActionError {
        Label(error, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
          .padding(.bottom, 16)
          .accessibilityLabel("Story action failed. \(error)")
          .accessibilitySortPriority(AccessibilityOrder.status.sortPriority)
      }
      ForEach(presentation.bodyElements, id: \.self) { element in
        detailElement(element, presentation: presentation)
      }
    }
    .frame(maxWidth: ReadingColumnMetrics.maximumWidth, alignment: .leading)
  }

  @ViewBuilder
  private func detailElement(
    _ element: StoryDetailElement,
    presentation: StoryDetailPresentation
  ) -> some View {
    switch element {
    case .metadata, .title, .provenance:
      EmptyView()
    case .originalExcerpt:
      readingSection("Original excerpt", body: presentation.originalExcerpt)
    case .whatHappened:
      readingSection("What happened", body: presentation.whatHappened)
    case .whyItMatters:
      readingSection("Why it matters", body: presentation.whyItMatters)
    case .caveat:
      readingSection("Caveat", body: presentation.caveat)
    case .scoreAndSources:
      DisclosureGroup {
        VStack(alignment: .leading, spacing: 8) {
          Text(presentation.scoreExplanation)
          Text("Sources: \(presentation.sourceNames.joined(separator: ", "))")
        }
        .font(.system(size: bodySize))
        .foregroundStyle(.secondary)
        .textSelection(.enabled)
        .padding(.top, 8)
      } label: {
        Text("Why this ranked here")
          .font(.system(size: sectionLabelSize, weight: .semibold))
          .foregroundStyle(.secondary)
      }
      .accessibilitySortPriority(AccessibilityOrder.content.sortPriority)
      .padding(.bottom, 24)
    case .actions:
      Divider()
        .padding(.bottom, 16)
      ViewThatFits(in: .horizontal) {
        actionContent(axis: .horizontal)
        actionContent(axis: .vertical)
      }
      .controlSize(.regular)
      .accessibilitySortPriority(AccessibilityOrder.actions.sortPriority)
    }
  }

  @ViewBuilder
  private func actionContent(axis: Axis) -> some View {
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
          .font(.system(size: sectionLabelSize, weight: .semibold))
          .foregroundStyle(.secondary)
          .accessibilityAddTraits(.isHeader)
        Text(body)
          .font(.system(size: bodySize))
          .lineSpacing(bodyLineSpacing)
          .textSelection(.enabled)
      }
      .padding(.bottom, 24)
      .accessibilitySortPriority(AccessibilityOrder.content.sortPriority)
    }
  }
}
