import SwiftUI

public enum SignalStatus: CaseIterable, Sendable, Equatable {
  case current
  case refreshing
  case partiallyStale
  case offline
  case failed
  case settingUp

  public init(phase: AppPhase) {
    switch phase {
    case .ready, .empty: self = .current
    case .refreshing: self = .refreshing
    case .stale: self = .partiallyStale
    case .offline: self = .offline
    case .startupFailure, .failure: self = .failed
    case .loading, .welcome, .buildingFirstBriefing: self = .settingUp
    }
  }

  public var title: String {
    switch self {
    case .current: "Current"
    case .refreshing: "Refreshing"
    case .partiallyStale: "Partially stale"
    case .offline: "Offline"
    case .failed: "Refresh failed"
    case .settingUp: "Setting up"
    }
  }

  public var symbolName: String {
    switch self {
    case .current: "checkmark.circle.fill"
    case .refreshing: "arrow.trianglehead.2.clockwise.rotate.90"
    case .partiallyStale: "clock.badge.exclamationmark"
    case .offline: "wifi.slash"
    case .failed: "exclamationmark.triangle.fill"
    case .settingUp: "dot.radiowaves.left.and.right"
    }
  }

  public var accessibilityLabel: String {
    switch self {
    case .current: "AI Daily Signal, briefing status: current"
    case .refreshing: "AI Daily Signal, briefing status: refreshing"
    case .partiallyStale: "AI Daily Signal, briefing status: partially stale"
    case .offline: "AI Daily Signal, briefing status: offline"
    case .failed: "AI Daily Signal, briefing status: refresh failed"
    case .settingUp: "AI Daily Signal, briefing status: setting up"
    }
  }

  fileprivate var tint: Color {
    switch self {
    case .current: .green
    case .refreshing, .settingUp: .accentColor
    case .partiallyStale: .orange
    case .offline: .secondary
    case .failed: .red
    }
  }
}

public enum MenuBarAction: Sendable, Equatable {
  case refreshOrCancel
  case openBriefing
  case settings
  case quit
}

public enum MenuBarScrolling: Sendable, Equatable {
  case fixedContent
}

public enum MenuBarRefreshControl: Sendable, Equatable {
  case refresh
  case cancel
  case unavailable
}

public enum MenuBarRenderRow: Sendable, Equatable, Hashable {
  case status
  case topSignal
  case explanation
  case primaryActions
  case utilityMenu
}

public struct TopSignalPresentation: Sendable, Equatable {
  public let title: String
  public let source: String
  public let provenance: String
}

public struct MenuBarPresentation: Sendable, Equatable {
  public let status: SignalStatus
  public let lastRefreshText: String
  public let topSignals: [TopSignalPresentation]
  public let errorMessage: String?
  public let rows: [MenuBarRenderRow]
  public let scrolling = MenuBarScrolling.fixedContent
  public let refreshControl: MenuBarRefreshControl

  public init(
    phase: AppPhase,
    snapshot: AppSnapshot?,
    errorMessage: String?,
    refreshInProgress: Bool
  ) {
    status = SignalStatus(phase: phase)
    if let date = snapshot?.status.refresh?.lastRefreshAt {
      lastRefreshText = SignalFormatters.relativeDate(date)
    } else {
      lastRefreshText = "Not refreshed"
    }
    topSignals =
      snapshot?.today?.items.prefix(1).map { item in
        TopSignalPresentation(
          title: item.story.title,
          source: Self.sourceName(for: item.story, in: snapshot),
          provenance: Self.provenance(for: item.story)
        )
      } ?? []
    self.errorMessage = errorMessage
    if case .startupFailure = phase {
      refreshControl = .unavailable
    } else {
      refreshControl = refreshInProgress ? .cancel : .refresh
    }
    rows =
      errorMessage == nil
      ? [.status, .topSignal, .primaryActions, .utilityMenu]
      : [.status, .topSignal, .explanation, .primaryActions, .utilityMenu]
  }

  public var elements: [MenuBarElement] {
    rows.flatMap { row -> [MenuBarElement] in
      switch row {
      case .status: [.status]
      case .topSignal: [.topSignal]
      case .explanation: []
      case .primaryActions:
        refreshControl == .unavailable ? [.openBriefing] : [.refreshOrCancel, .openBriefing]
      case .utilityMenu: [.settings, .quit]
      }
    }
  }

  public var actionSet: [MenuBarAction] {
    elements.compactMap { element -> MenuBarAction? in
      switch element {
      case .status, .topSignal: nil
      case .refreshOrCancel: .refreshOrCancel
      case .openBriefing: .openBriefing
      case .settings: .settings
      case .quit: .quit
      }
    }
  }

  private static func sourceName(for story: Story, in snapshot: AppSnapshot?) -> String {
    guard let sourceID = story.sourceIDs.first else { return "Unknown source" }
    return snapshot?.sources.first(where: { $0.id == sourceID })?.name ?? sourceID
  }

  private static func provenance(for story: Story) -> String {
    guard let summary = story.selectedSummary else { return "Smart summary" }
    return "\(summary.provider.displayName) · \(summary.model)"
  }
}

public struct MenuBarStatusLabel: View {
  @Bindable private var model: AppModel

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    let status = SignalStatus(phase: model.phase)
    Label("AI Daily Signal — \(status.title)", systemImage: status.symbolName)
      .accessibilityLabel(status.accessibilityLabel)
  }
}

public struct MenuBarContentView: View {
  @Bindable private var model: AppModel
  private let openBriefing: () -> Void
  private let openSettings: () -> Void
  private let quit: () -> Void

  public init(
    model: AppModel,
    openBriefing: @escaping () -> Void,
    openSettings: @escaping () -> Void,
    quit: @escaping () -> Void
  ) {
    self.model = model
    self.openBriefing = openBriefing
    self.openSettings = openSettings
    self.quit = quit
  }

  public var body: some View {
    let presentation = MenuBarPresentation(
      phase: model.phase,
      snapshot: model.snapshot,
      errorMessage: model.errorMessage,
      refreshInProgress: model.activeOperationID != nil
    )
    switch presentation.scrolling {
    case .fixedContent:
      VStack(alignment: .leading, spacing: 14) {
        ForEach(presentation.rows, id: \.self) { row in
          render(row, from: presentation)
        }
      }
      .padding(16)
      .frame(width: 350)
    }
  }

  @ViewBuilder
  private func render(_ row: MenuBarRenderRow, from presentation: MenuBarPresentation)
    -> some View
  {
    switch row {
    case .status:
      statusHeader(presentation)
      Divider()
    case .topSignal:
      topSignal(presentation.topSignals.first)
    case .explanation:
      if let errorMessage = presentation.errorMessage {
        Label(errorMessage, systemImage: presentation.status.symbolName)
          .font(.callout)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
    case .primaryActions:
      HStack {
        refreshButton(presentation.refreshControl)
        Spacer()
        Button("Open Briefing", action: openBriefing)
          .buttonStyle(.borderedProminent)
      }
    case .utilityMenu:
      Divider()
      HStack {
        Text("AI Daily Signal")
          .font(.caption)
          .foregroundStyle(.tertiary)
        Spacer()
        Menu {
          Button("Settings", systemImage: "gearshape", action: openSettings)
          Divider()
          Button("Quit AI Daily Signal", systemImage: "power", action: quit)
        } label: {
          Label("More", systemImage: "ellipsis.circle")
            .labelStyle(.iconOnly)
        }
        .menuStyle(.borderlessButton)
        .accessibilityLabel("More actions")
      }
    }
  }

  private func statusHeader(_ presentation: MenuBarPresentation) -> some View {
    HStack(alignment: .firstTextBaseline) {
      Label(presentation.status.title, systemImage: presentation.status.symbolName)
        .font(.headline)
        .foregroundStyle(presentation.status.tint)
        .accessibilityLabel(presentation.status.accessibilityLabel)
      Spacer()
      Text(presentation.lastRefreshText)
        .font(.caption)
        .foregroundStyle(.secondary)
    }
  }

  @ViewBuilder
  private func topSignal(_ signal: TopSignalPresentation?) -> some View {
    if let signal {
      VStack(alignment: .leading, spacing: 5) {
        Text("Top signal")
          .font(.caption.weight(.semibold))
          .foregroundStyle(.secondary)
          .textCase(.uppercase)
        Text(signal.title)
          .font(.headline)
          .lineLimit(3)
        Text("\(signal.source) · \(signal.provenance)")
          .font(.caption)
          .foregroundStyle(.secondary)
          .lineLimit(1)
      }
      .accessibilityElement(children: .combine)
    } else {
      VStack(alignment: .leading, spacing: 4) {
        Text("Top signal")
          .font(.caption.weight(.semibold))
          .foregroundStyle(.secondary)
          .textCase(.uppercase)
        Text("No briefing yet")
          .font(.headline)
        Text("Refresh to check enabled sources.")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
    }
  }

  @ViewBuilder
  private func refreshButton(_ control: MenuBarRefreshControl) -> some View {
    switch control {
    case .cancel:
      Button("Cancel Refresh", systemImage: "xmark") {
        model.cancelRefresh()
      }
    case .refresh:
      Button("Refresh", systemImage: "arrow.clockwise") {
        Task { await model.refresh() }
      }
    case .unavailable:
      EmptyView()
    }
  }
}

extension ProviderKind {
  fileprivate var displayName: String {
    switch self {
    case .openAI: "OpenAI"
    case .anthropic: "Anthropic"
    case .gemini: "Gemini"
    case .openAICompatible: "Compatible provider"
    }
  }
}
