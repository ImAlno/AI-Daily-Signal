import AppKit
import SwiftUI

public struct SignalDisclosurePresentation: Sendable, Equatable {
  public let isExpanded: Bool

  public init(storyID: String, selectedStoryID: String?) {
    isExpanded = storyID == selectedStoryID
  }
}

public struct SignalDisclosureView: View {
  private let presentation: StoryRowPresentation
  @Bindable private var model: AppModel
  @State private var isHovered = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @Environment(\.dynamicTypeSize) private var dynamicTypeSize

  public init(presentation: StoryRowPresentation, model: AppModel) {
    self.presentation = presentation
    self.model = model
  }

  public var body: some View {
    let disclosure = SignalDisclosurePresentation(
      storyID: presentation.storyID,
      selectedStoryID: model.selectedStoryID
    )
    let header = StoryHeaderPresentation(
      row: presentation,
      isExpanded: disclosure.isExpanded,
      isHovered: isHovered,
      dynamicTypeSize: dynamicTypeSize
    )
    let transition = ReaderMotionPresentation(reduceMotion: reduceMotion).duration.map {
      Animation.easeOut(duration: $0)
    }
    VStack(alignment: .leading, spacing: 0) {
      StoryHeaderView(presentation: header) {
        model.selectedStoryID = disclosure.isExpanded ? nil : presentation.storyID
      }
      .animation(transition, value: header.isExpanded)
      .animation(transition, value: header.isHovered)
      .onHover { isHovered = $0 }

      if disclosure.isExpanded, let story = model.story(id: presentation.storyID) {
        SummaryVariantPicker(story: story, model: model)
          .padding(
            .leading,
            StoryRowMetrics.expandedContentLeadingPadding(hasRank: presentation.rank != nil)
          )
          .padding(.trailing, StoryRowMetrics.horizontalPadding)
          .padding(.top, 6)
        ExpandedStoryView(story: story, model: model)
          .padding(
            .leading,
            StoryRowMetrics.expandedContentLeadingPadding(hasRank: presentation.rank != nil)
          )
          .padding(.trailing, StoryRowMetrics.horizontalPadding)
          .padding(.top, 14)
          .padding(.bottom, 18)
      }
      Divider()
    }
    .contextMenu { storyContextMenu }
  }

  @ViewBuilder
  private var storyContextMenu: some View {
    let story = model.story(id: presentation.storyID)
    Button("Open Source", systemImage: "safari") {
      if let value = story?.canonicalURL, let url = StorySourceURL.validated(value) {
        NSWorkspace.shared.open(url)
      }
    }
    .disabled(story.map { !StorySourceActionPresentation(story: $0).isEnabled } ?? true)

    Button(story?.isSaved == true ? "Remove from Saved" : "Save Story", systemImage: "bookmark") {
      model.selectedStoryID = presentation.storyID
      Task { await model.toggleSelectedStorySaved() }
    }
    .disabled(
      story.map { model.storyActionState(for: .saving(storyID: $0.id)) != nil } ?? true
    )

    Button(story?.isRead == true ? "Mark Unread" : "Mark Read", systemImage: "checkmark.circle") {
      model.selectedStoryID = presentation.storyID
      Task { await model.toggleSelectedStoryRead() }
    }
    .disabled(
      story.map { model.storyActionState(for: .markingRead(storyID: $0.id)) != nil } ?? true
    )
  }
}
