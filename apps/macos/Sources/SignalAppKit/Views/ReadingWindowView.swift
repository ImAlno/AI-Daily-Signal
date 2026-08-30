import AppKit
import SwiftUI

public enum ReadingCommand: CaseIterable, Sendable, Equatable {
  case refresh
  case openSource
  case save
  case settings

  public var keyEquivalent: KeyEquivalent {
    switch self {
    case .refresh: "r"
    case .openSource: "o"
    case .save: "s"
    case .settings: ","
    }
  }

  public var descriptor: KeyboardCommandDescriptor {
    switch self {
    case .refresh:
      KeyboardCommandDescriptor(
        key: "r", modifiers: [.command], help: "Refresh briefing (⌘R)"
      )
    case .openSource:
      KeyboardCommandDescriptor(
        key: "o", modifiers: [.command], help: "Open selected story source (⌘O)"
      )
    case .save:
      KeyboardCommandDescriptor(
        key: "s", modifiers: [.command], help: "Save selected story (⌘S)"
      )
    case .settings:
      KeyboardCommandDescriptor(
        key: ",", modifiers: [.command], help: "Open Settings (⌘,)"
      )
    }
  }
}

public struct ReadingToolbarPresentation: Sendable, Equatable {
  public let refreshControl: RefreshControlPresentation

  public init(phase: AppPhase, refreshInProgress: Bool) {
    if case .startupFailure = phase {
      refreshControl = .unavailable
    } else {
      refreshControl = refreshInProgress ? .cancel : .refresh
    }
  }
}

public enum UnavailableContentAction: Sendable, Equatable {
  case retry

  public var title: String { "Try Again" }
}

public struct UnavailableContentPresentation: Sendable, Equatable {
  public let title: String
  public let message: String
  public let systemImage: String
  public let action: UnavailableContentAction?

  public init?(phase: AppPhase) {
    switch phase {
    case .startupFailure(let message):
      title = "Local data unavailable"
      self.message = message
      systemImage = "internaldrive"
      action = nil
    case .offline(let message):
      title = "Offline"
      self.message = message
      systemImage = "wifi.slash"
      action = .retry
    case .failure(let message):
      title = "AI Daily Signal unavailable"
      self.message = message
      systemImage = "exclamationmark.triangle.fill"
      action = .retry
    default:
      return nil
    }
  }
}

public struct ReadingWindowView: View {
  @Bindable private var model: AppModel

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    let welcome = WelcomePresentation(phase: model.phase)
    Group {
      if welcome.isPresented {
        WelcomeView(model: model)
      } else {
        readingShell
      }
    }
    .frame(minWidth: 860, minHeight: 600)
  }

  private var readingShell: some View {
    let toolbarPresentation = ReadingToolbarPresentation(
      phase: model.phase,
      refreshInProgress: model.activeOperationID != nil
    )
    return NavigationSplitView {
      List(selection: destinationSelection) {
        ForEach(Destination.allCases, id: \.self) { destination in
          Label(destination.title, systemImage: destination.systemImage)
            .tag(destination)
        }
      }
      .navigationTitle("AI Daily Signal")
      .navigationSplitViewColumnWidth(min: 180, ideal: 220)
    } detail: {
      Group {
        switch model.destination {
        case .today, .latest, .saved:
          HSplitView {
            destinationContent
              .frame(minWidth: 320, idealWidth: 390, maxWidth: 520)
            StoryDetailView(model: model)
              .frame(minWidth: 440, idealWidth: 680)
          }
        case .sources, .settings:
          destinationContent
        }
      }
      .navigationTitle(model.destination.title)
    }
    .toolbar {
      ToolbarItemGroup(placement: .primaryAction) {
        refreshToolbarButton(toolbarPresentation.refreshControl)
        Button("Open Source", systemImage: "safari") {
          openSelectedSource()
        }
        .keyboardShortcut(ReadingCommand.openSource.keyEquivalent, modifiers: .command)
        .disabled(
          model.selectedStory.map { !StorySourceActionPresentation(story: $0).isEnabled } ?? true
        )
        .help(ReadingCommand.openSource.descriptor.help)

        Button(
          model.selectedStory?.isSaved == true ? "Remove from Saved" : "Save Story",
          systemImage: model.selectedStory?.isSaved == true ? "bookmark.slash" : "bookmark"
        ) {
          Task { await model.toggleSelectedStorySaved() }
        }
        .keyboardShortcut(ReadingCommand.save.keyEquivalent, modifiers: .command)
        .disabled(
          model.selectedStory.map {
            model.storyActionState(for: .saving(storyID: $0.id)) != nil
          } ?? true
        )
        .help(ReadingCommand.save.descriptor.help)

        Button("Settings", systemImage: "gearshape") {
          model.destination = .settings
        }
        .keyboardShortcut(ReadingCommand.settings.keyEquivalent, modifiers: .command)
        .help(ReadingCommand.settings.descriptor.help)
      }
    }
  }

  private var destinationSelection: Binding<Destination?> {
    Binding(
      get: { model.destination },
      set: { destination in
        if let destination { model.destination = destination }
      }
    )
  }

  @ViewBuilder
  private var destinationContent: some View {
    if model.phase == .loading {
      ProgressView("Loading your briefing…")
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    } else if model.snapshot == nil,
      let presentation = UnavailableContentPresentation(phase: model.phase)
    {
      unavailableContent(presentation)
    } else {
      VStack(spacing: 0) {
        if let error = model.errorMessage {
          Label(error, systemImage: SignalStatus(phase: model.phase).symbolName)
            .font(.callout)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 20)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityLabel(
              "\(SignalStatus(phase: model.phase).title). \(error)"
            )
            .accessibilitySortPriority(AccessibilityOrder.status.sortPriority)
          Divider()
        }
        destinationBody
      }
    }
  }

  @ViewBuilder
  private func unavailableContent(_ presentation: UnavailableContentPresentation) -> some View {
    if presentation.action == .retry {
      EmptyStateView(
        title: presentation.title,
        message: presentation.message,
        systemImage: presentation.systemImage,
        actionTitle: UnavailableContentAction.retry.title
      ) {
        Task { await model.reloadSnapshot() }
      }
    } else {
      EmptyStateView(
        title: presentation.title,
        message: presentation.message,
        systemImage: presentation.systemImage
      )
    }
  }

  @ViewBuilder
  private var destinationBody: some View {
    switch model.destination {
    case .today:
      TodayView(model: model)
    case .latest:
      StoryListView(kind: .latest, model: model)
    case .saved:
      StoryListView(kind: .saved, model: model)
    case .sources:
      SourcesView(model: model)
    case .settings:
      SettingsView(model: model)
    }
  }

  @ViewBuilder
  private func refreshToolbarButton(_ control: RefreshControlPresentation) -> some View {
    switch control {
    case .cancel:
      Button("Cancel Refresh", systemImage: "xmark") {
        model.cancelRefresh()
      }
      .keyboardShortcut(ReadingCommand.refresh.keyEquivalent, modifiers: .command)
      .help("Cancel the current refresh (⌘R)")
    case .refresh:
      Button("Refresh", systemImage: "arrow.clockwise") {
        Task { await model.refresh() }
      }
      .keyboardShortcut(ReadingCommand.refresh.keyEquivalent, modifiers: .command)
      .help(ReadingCommand.refresh.descriptor.help)
    case .unavailable:
      EmptyView()
    }
  }

  private func openSelectedSource() {
    guard let value = model.selectedStory?.canonicalURL,
      let url = StorySourceURL.validated(value)
    else { return }
    NSWorkspace.shared.open(url)
  }
}

extension Destination {
  public var title: String {
    switch self {
    case .today: "Today"
    case .latest: "Latest"
    case .saved: "Saved"
    case .sources: "Sources"
    case .settings: "Settings"
    }
  }

  fileprivate var systemImage: String {
    switch self {
    case .today: "sun.max"
    case .latest: "clock"
    case .saved: "bookmark"
    case .sources: "dot.radiowaves.left.and.right"
    case .settings: "gearshape"
    }
  }
}
