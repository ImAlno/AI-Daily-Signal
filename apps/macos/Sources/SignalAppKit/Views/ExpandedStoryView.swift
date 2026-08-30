import AppKit
import SwiftUI

public struct ExpandedStoryView: View {
  private let story: Story
  @Bindable private var model: AppModel

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
      ForEach(presentation.elements, id: \.self) { element in
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
        .font(.title2.weight(.semibold))
        .textSelection(.enabled)
        .accessibilityAddTraits(.isHeader)
        .accessibilitySortPriority(AccessibilityOrder.title.sortPriority)
        .padding(.bottom, 14)
    case .provenance:
      Text(presentation.provenance.shortLabel)
        .font(.caption)
        .foregroundStyle(.secondary)
        .accessibilityLabel(presentation.provenance.accessibilityLabel)
        .accessibilitySortPriority(AccessibilityOrder.status.sortPriority)
        .padding(.bottom, 24)
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
        actionContent(axis: .horizontal)
        actionContent(axis: .vertical)
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
