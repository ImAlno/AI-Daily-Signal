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
    let staleIDs = snapshot.map(ReaderPresentationSupport.staleStoryIDs(in:)) ?? []
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
        GeometryReader { proxy in
          ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
              BriefingHeaderView(
                presentation: BriefingHeaderPresentation(
                  destination: model.destination,
                  snapshot: model.snapshot,
                  calendarDate: Date.now.formatted(
                    .dateTime.weekday(.wide).month(.wide).day().year()
                  )
                )
              )
              ForEach(presentation.rows) { row in
                SignalDisclosureView(presentation: row, model: model)
              }
            }
            .frame(maxWidth: ReadingColumnMetrics.maximumWidth, alignment: .leading)
            .padding(.horizontal, ReadingColumnMetrics.horizontalPadding(for: proxy.size.width))
            .padding(.vertical, 30)
            .frame(maxWidth: .infinity, alignment: .center)
          }
          .background(Color(nsColor: .textBackgroundColor))
        }
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
}

enum ReaderPresentationSupport {
  static func staleStoryIDs(in snapshot: AppSnapshot) -> Set<String> {
    guard let today = snapshot.today else { return [] }
    if today.isStale {
      return Set(today.items.map { $0.story.id })
    }
    return Set(today.items.filter(\.isStale).map { $0.story.id })
  }

  static func primarySource(for story: Story, sources: [Source]) -> String {
    guard let sourceID = story.sourceIDs.first else { return "Unknown source" }
    return sources.first(where: { $0.id == sourceID })?.name ?? sourceID
  }

  static func sourceNames(for story: Story, sources: [Source]) -> [String] {
    let byID = Dictionary(uniqueKeysWithValues: sources.map { ($0.id, $0.name) })
    return story.sourceIDs.map { byID[$0] ?? $0 }
  }
}
