import AppKit
import SwiftUI

public struct TodaySectionPresentation: Identifiable, Sendable, Equatable {
  public var id: String { title }
  public let title: String
  public let rows: [StoryRowPresentation]
}

public struct TodayPresentation: Sendable, Equatable {
  public let sections: [TodaySectionPresentation]
  public let emptyState: ReaderEmptyPresentation?

  public init(
    briefing: Briefing?,
    sources: [Source],
    selectionForStory: (String) -> ReadingSummarySelection,
    relativeTo reference: Date = .now
  ) {
    guard let briefing, !briefing.items.isEmpty else {
      sections = []
      emptyState = ReaderEmptyPresentation(
        title: "No briefing yet",
        message: "Refresh to build a finite briefing from your enabled sources.",
        systemImage: "newspaper",
        action: .refresh
      )
      return
    }

    var orderedTitles: [String] = []
    var rowsBySection: [String: [StoryRowPresentation]] = [:]
    for item in briefing.items {
      if rowsBySection[item.section] == nil {
        orderedTitles.append(item.section)
        rowsBySection[item.section] = []
      }
      rowsBySection[item.section]?.append(
        StoryRowPresentation(
          story: item.story,
          primarySource: ReaderPresentationSupport.primarySource(
            for: item.story,
            sources: sources
          ),
          relativeTime: SignalFormatters.relativeDate(
            item.story.publishedAt,
            relativeTo: reference
          ),
          isStale: briefing.isStale || item.isStale,
          rank: item.position,
          summarySelection: selectionForStory(item.story.id)
        )
      )
    }
    sections = orderedTitles.map {
      TodaySectionPresentation(title: $0, rows: rowsBySection[$0] ?? [])
    }
    emptyState = nil
  }
}

public struct TodayView: View {
  @Bindable private var model: AppModel

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    let presentation = TodayPresentation(
      briefing: model.snapshot?.today,
      sources: model.snapshot?.sources ?? [],
      selectionForStory: model.summarySelection(for:)
    )

    Group {
      if let emptyState = presentation.emptyState {
        EmptyStateView(
          title: emptyState.title,
          message: emptyState.message,
          systemImage: emptyState.systemImage,
          actionTitle: "Refresh"
        ) {
          Task { await model.refresh() }
        }
      } else {
        List(selection: $model.selectedStoryID) {
          ForEach(presentation.sections) { section in
            Section {
              ForEach(section.rows) { row in
                StoryRowView(presentation: row)
                  .tag(row.storyID)
                  .contextMenu { storyContextMenu(storyID: row.storyID) }
              }
            } header: {
              Text(section.title)
                .font(.caption)
                .fontWeight(.semibold)
                .textCase(nil)
                .foregroundStyle(.secondary)
                .accessibilityAddTraits(.isHeader)
            }
          }
        }
        .listStyle(.inset)
        .accessibilityLabel("Today's ranked briefing")
      }
    }
  }

  @ViewBuilder
  private func storyContextMenu(storyID: String) -> some View {
    let story = model.story(id: storyID)
    Button("Open Source", systemImage: "safari") {
      if let value = story?.canonicalURL, let url = StorySourceURL.validated(value) {
        NSWorkspace.shared.open(url)
      }
    }
    Button(story?.isSaved == true ? "Remove from Saved" : "Save Story", systemImage: "bookmark") {
      model.selectedStoryID = storyID
      Task { await model.toggleSelectedStorySaved() }
    }
    Button(story?.isRead == true ? "Mark Unread" : "Mark Read", systemImage: "checkmark.circle") {
      model.selectedStoryID = storyID
      Task { await model.toggleSelectedStoryRead() }
    }
  }
}
