import AppKit
import SwiftUI

public enum ReaderEmptyAction: Sendable, Equatable {
  case refresh
  case showToday
}

public struct ReaderEmptyPresentation: Sendable, Equatable {
  public let title: String
  public let message: String
  public let systemImage: String
  public let action: ReaderEmptyAction
}

public enum StoryListKind: Sendable, Equatable {
  case latest
  case saved
}

public struct StoryListPresentation: Sendable, Equatable {
  public let kind: StoryListKind
  public let rows: [StoryRowPresentation]
  public let emptyState: ReaderEmptyPresentation?
  public let isFinite = true

  public init(
    kind: StoryListKind,
    stories: [Story],
    sources: [Source],
    staleStoryIDs: Set<String>,
    selectionForStory: (String) -> ReadingSummarySelection,
    relativeTo reference: Date = .now
  ) {
    self.kind = kind
    rows = stories.map { story in
      StoryRowPresentation(
        story: story,
        primarySource: ReaderPresentationSupport.primarySource(for: story, sources: sources),
        relativeTime: SignalFormatters.relativeDate(story.publishedAt, relativeTo: reference),
        isStale: staleStoryIDs.contains(story.id),
        rank: nil,
        summarySelection: selectionForStory(story.id)
      )
    }
    if rows.isEmpty {
      switch kind {
      case .latest:
        emptyState = ReaderEmptyPresentation(
          title: "No recent stories",
          message: "Refresh to check your enabled sources for new stories.",
          systemImage: "clock",
          action: .refresh
        )
      case .saved:
        emptyState = ReaderEmptyPresentation(
          title: "No saved stories",
          message: "Save a story from Today or Latest to keep it here.",
          systemImage: "bookmark",
          action: .showToday
        )
      }
    } else {
      emptyState = nil
    }
  }
}

public struct StoryListView: View {
  @Bindable private var model: AppModel
  private let kind: StoryListKind

  public init(kind: StoryListKind, model: AppModel) {
    self.kind = kind
    self.model = model
  }

  public var body: some View {
    let snapshot = model.snapshot
    let stories = kind == .latest ? snapshot?.latest ?? [] : snapshot?.saved ?? []
    let staleIDs = Set(
      snapshot?.today?.items.filter(\.isStale).map { $0.story.id } ?? []
    )
    let presentation = StoryListPresentation(
      kind: kind,
      stories: stories,
      sources: snapshot?.sources ?? [],
      staleStoryIDs: staleIDs,
      selectionForStory: model.summarySelection(for:)
    )

    Group {
      if let emptyState = presentation.emptyState {
        emptyView(emptyState)
      } else {
        List(selection: $model.selectedStoryID) {
          ForEach(presentation.rows) { row in
            StoryRowView(presentation: row)
              .tag(row.storyID)
              .contextMenu { storyContextMenu(storyID: row.storyID) }
          }
        }
        .listStyle(.inset)
        .accessibilityLabel(kind == .latest ? "Latest stories" : "Saved stories")
      }
    }
  }

  @ViewBuilder
  private func emptyView(_ presentation: ReaderEmptyPresentation) -> some View {
    switch presentation.action {
    case .refresh:
      EmptyStateView(
        title: presentation.title,
        message: presentation.message,
        systemImage: presentation.systemImage,
        actionTitle: "Refresh"
      ) {
        Task { await model.refresh() }
      }
    case .showToday:
      EmptyStateView(
        title: presentation.title,
        message: presentation.message,
        systemImage: presentation.systemImage,
        actionTitle: "Show Today"
      ) {
        model.destination = .today
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

enum ReaderPresentationSupport {
  static func primarySource(for story: Story, sources: [Source]) -> String {
    guard let sourceID = story.sourceIDs.first else { return "Unknown source" }
    return sources.first(where: { $0.id == sourceID })?.name ?? sourceID
  }

  static func sourceNames(for story: Story, sources: [Source]) -> [String] {
    let byID = Dictionary(uniqueKeysWithValues: sources.map { ($0.id, $0.name) })
    return story.sourceIDs.map { byID[$0] ?? $0 }
  }
}
