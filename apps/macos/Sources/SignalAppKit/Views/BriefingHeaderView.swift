import SwiftUI

public struct BriefingHeaderPresentation: Sendable, Equatable {
  public let title: String
  public let dateText: String
  public let signalCount: Int
  public let enabledSourceCount: Int
  public let metadataText: String

  public init(destination: Destination, snapshot: AppSnapshot?, calendarDate: String) {
    title = destination.title
    dateText = calendarDate
    switch destination {
    case .today: signalCount = snapshot?.today?.items.count ?? 0
    case .latest: signalCount = snapshot?.latest.count ?? 0
    case .saved: signalCount = snapshot?.saved.count ?? 0
    case .sources, .models, .settings: signalCount = 0
    }
    enabledSourceCount = snapshot?.sources.filter(\.enabled).count ?? 0
    metadataText = "\(signalCount) \(signalCount == 1 ? "signal" : "signals") · "
      + "\(enabledSourceCount) \(enabledSourceCount == 1 ? "source" : "sources")"
  }
}

public struct BriefingHeaderView: View {
  private let presentation: BriefingHeaderPresentation

  public init(presentation: BriefingHeaderPresentation) {
    self.presentation = presentation
  }

  public var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      Text(presentation.dateText)
        .font(.caption)
        .foregroundStyle(.secondary)
      Text(presentation.title)
        .font(.largeTitle.weight(.semibold))
        .accessibilityAddTraits(.isHeader)
      Text(presentation.metadataText)
        .font(.callout)
        .foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(.bottom, 22)
  }
}
