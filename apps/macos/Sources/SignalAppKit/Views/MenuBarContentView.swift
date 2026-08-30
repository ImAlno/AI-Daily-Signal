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
    case .failure: self = .failed
    case .loading, .welcome: self = .settingUp
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
    case .current: "Briefing status: current"
    case .refreshing: "Briefing status: refreshing"
    case .partiallyStale: "Briefing status: partially stale"
    case .offline: "Briefing status: offline"
    case .failed: "Briefing status: refresh failed"
    case .settingUp: "Briefing status: setting up"
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
    VStack(alignment: .leading, spacing: 14) {
      statusHeader
      Divider()
      topSignal
      if let errorMessage = model.errorMessage {
        Label(errorMessage, systemImage: SignalStatus(phase: model.phase).symbolName)
          .font(.callout)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
      HStack {
        refreshButton
        Spacer()
        Button("Open Briefing", action: openBriefing)
          .buttonStyle(.borderedProminent)
      }
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
    .padding(16)
    .frame(width: 350)
  }

  private var statusHeader: some View {
    let status = SignalStatus(phase: model.phase)
    return HStack(alignment: .firstTextBaseline) {
      Label(status.title, systemImage: status.symbolName)
        .font(.headline)
        .foregroundStyle(status.tint)
        .accessibilityLabel(status.accessibilityLabel)
      Spacer()
      Text(lastRefreshText)
        .font(.caption)
        .foregroundStyle(.secondary)
    }
  }

  @ViewBuilder
  private var topSignal: some View {
    if let item = model.snapshot?.today?.items.first {
      VStack(alignment: .leading, spacing: 5) {
        Text("Top signal")
          .font(.caption.weight(.semibold))
          .foregroundStyle(.secondary)
          .textCase(.uppercase)
        Text(item.story.title)
          .font(.headline)
          .lineLimit(3)
        Text("\(sourceName(for: item.story)) · \(provenance(for: item.story))")
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

  private var refreshButton: some View {
    Group {
      if model.phase == .refreshing {
        Button("Cancel Refresh", systemImage: "xmark") {
          model.cancelRefresh()
        }
      } else {
        Button("Refresh", systemImage: "arrow.clockwise") {
          Task { await model.refresh() }
        }
      }
    }
  }

  private var lastRefreshText: String {
    guard let date = model.snapshot?.status.refresh?.lastRefreshAt else { return "Not refreshed" }
    return SignalFormatters.relativeDate(date)
  }

  private func sourceName(for story: Story) -> String {
    guard let sourceID = story.sourceIDs.first else { return "Unknown source" }
    return model.snapshot?.sources.first(where: { $0.id == sourceID })?.name ?? sourceID
  }

  private func provenance(for story: Story) -> String {
    guard let summary = story.selectedSummary else { return "Smart summary" }
    return "\(summary.provider.displayName) · \(summary.model)"
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
