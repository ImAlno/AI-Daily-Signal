import SwiftUI

public struct TodaySectionPresentation: Identifiable, Sendable, Equatable {
  public let id: String
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

    var values: [TodaySectionPresentation] = []
    for item in briefing.items {
      let row = StoryRowPresentation(
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
      if let last = values.last, last.title == item.section {
        values[values.count - 1] = TodaySectionPresentation(
          id: last.id,
          title: last.title,
          rows: last.rows + [row]
        )
      } else {
        values.append(
          TodaySectionPresentation(
            id: "\(values.count)-\(item.section)",
            title: item.section,
            rows: [row]
          )
        )
      }
    }
    sections = values
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
              ForEach(presentation.sections) { section in
                Text(section.title)
                  .font(.caption.weight(.semibold))
                  .foregroundStyle(.secondary)
                  .padding(.top, 12)
                  .padding(.bottom, 6)
                  .accessibilityAddTraits(.isHeader)
                ForEach(section.rows) { row in
                  SignalDisclosureView(presentation: row, model: model)
                }
              }
            }
            .frame(maxWidth: ReadingColumnMetrics.maximumWidth, alignment: .leading)
            .padding(.horizontal, ReadingColumnMetrics.horizontalPadding(for: proxy.size.width))
            .padding(.vertical, 30)
            .frame(maxWidth: .infinity, alignment: .center)
          }
          .background(Color(nsColor: .textBackgroundColor))
        }
        .accessibilityLabel("Today's ranked briefing")
      }
    }
  }
}
