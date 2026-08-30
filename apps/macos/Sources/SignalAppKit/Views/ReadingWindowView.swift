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
        key: ",", modifiers: [.command], help: "Open Preferences (⌘,)"
      )
    }
  }
}

public struct ReadingToolbarPresentation: Sendable, Equatable {
  public let refreshControl: RefreshControlPresentation
  public let windowTitle = "AI Daily Signal"
  public let directCommands: [ReadingCommand] = [.refresh]

  public init(phase: AppPhase, refreshInProgress: Bool) {
    if case .startupFailure = phase {
      refreshControl = .unavailable
    } else {
      refreshControl = refreshInProgress ? .cancel : .refresh
    }
  }

  public func overflowCommands(storyCommandsAvailable: Bool) -> [ReadingCommand] {
    storyCommandsAvailable ? [.openSource, .save, .settings] : [.settings]
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
    .frame(
      minWidth: ReadingColumnMetrics.minimumWindowWidth,
      minHeight: ReadingColumnMetrics.minimumWindowHeight
    )
  }

  private var readingShell: some View {
    GeometryReader { proxy in
      let mode = AppLayoutPolicy.mode(for: proxy.size.width)
      let navigationWidth = AppLayoutPolicy.navigationWidth(for: mode) ?? 58
      let toolbarPresentation = ReadingToolbarPresentation(
        phase: model.phase,
        refreshInProgress: model.activeOperationID != nil
      )

      NavigationSplitView(columnVisibility: columnVisibility(for: mode)) {
        AppNavigationView(mode: mode, selection: destinationSelection)
          .navigationSplitViewColumnWidth(
            min: navigationWidth,
            ideal: navigationWidth,
            max: navigationWidth
          )
          .toolbar(removing: .sidebarToggle)
      } detail: {
        destinationContent
          .navigationTitle(toolbarPresentation.windowTitle)
          .background(Color(nsColor: .textBackgroundColor))
      }
      .toolbar {
        toolbarContent(mode: mode, presentation: toolbarPresentation)
      }
    }
    .safeAreaInset(edge: .top) {
      if model.errorMessage == nil, model.activeOperationID == nil,
        let notice = model.refreshNotice
      {
        let presentation = RefreshNoticePresentation(notice: notice)
        Label(presentation.message, systemImage: presentation.status.symbolName)
          .font(.callout)
          .foregroundStyle(.secondary)
          .frame(maxWidth: .infinity, alignment: .leading)
          .padding(.horizontal, 16)
          .padding(.vertical, 9)
          .background(.bar)
          .accessibilityLabel(presentation.accessibilityLabel)
          .accessibilitySortPriority(AccessibilityOrder.status.sortPriority)
      }
    }
  }

  @ToolbarContentBuilder
  private func toolbarContent(
    mode: AppLayoutMode,
    presentation: ReadingToolbarPresentation
  ) -> some ToolbarContent {
    if AppNavigationPresentation(mode: mode).usesToolbarMenu {
      ToolbarItem(placement: .navigation) {
        CompactNavigationPicker(selection: destinationPickerSelection)
      }
    }

    ToolbarItem(placement: .primaryAction) {
      ForEach(presentation.directCommands, id: \.self) { command in
        toolbarButton(for: command, presentation: presentation)
      }
    }

    ToolbarItem(placement: .primaryAction) {
      Menu {
        ForEach(
          presentation.overflowCommands(storyCommandsAvailable: storyCommandsAvailable),
          id: \.self
        ) { command in
          toolbarButton(for: command, presentation: presentation)
        }
      } label: {
        Label("More", systemImage: "ellipsis.circle")
      }
      .help("Open story actions and Preferences")
    }
  }

  private var storyCommandsAvailable: Bool {
    [.today, .latest, .saved].contains(model.destination)
  }

  private var openSourceToolbarButton: some View {
    Button("Open Source", systemImage: "safari") {
      openSelectedSource()
    }
    .keyboardShortcut(ReadingCommand.openSource.keyEquivalent, modifiers: .command)
    .disabled(
      model.selectedStory.map { !StorySourceActionPresentation(story: $0).isEnabled } ?? true
    )
    .help(ReadingCommand.openSource.descriptor.help)
  }

  private var saveToolbarButton: some View {
    Button("Save Story", systemImage: "bookmark") {
      Task { await model.saveSelectedStory() }
    }
    .keyboardShortcut(ReadingCommand.save.keyEquivalent, modifiers: .command)
    .disabled(
      model.selectedStory.map {
        $0.isSaved || model.storyActionState(for: .saving(storyID: $0.id)) != nil
      } ?? true
    )
    .help(ReadingCommand.save.descriptor.help)
  }

  private var preferencesToolbarButton: some View {
    Button("Preferences", systemImage: "gearshape") {
      model.destination = .settings
    }
    .keyboardShortcut(ReadingCommand.settings.keyEquivalent, modifiers: .command)
    .help(ReadingCommand.settings.descriptor.help)
  }

  @ViewBuilder
  private func toolbarButton(
    for command: ReadingCommand,
    presentation: ReadingToolbarPresentation
  ) -> some View {
    switch command {
    case .refresh:
      refreshToolbarButton(presentation.refreshControl)
    case .openSource:
      openSourceToolbarButton
    case .save:
      saveToolbarButton
    case .settings:
      preferencesToolbarButton
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

  private var destinationPickerSelection: Binding<Destination> {
    Binding(
      get: { model.destination },
      set: { model.destination = $0 }
    )
  }

  private func columnVisibility(
    for mode: AppLayoutMode
  ) -> Binding<NavigationSplitViewVisibility> {
    let visibility: NavigationSplitViewVisibility =
      AppNavigationPresentation(mode: mode).persistentNavigationVisible ? .all : .detailOnly
    return Binding(
      get: { visibility },
      set: { _ in }
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
    case .models:
      ModelsSettingsView(model: model)
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
    case .models: "Models"
    case .settings: "Preferences"
    }
  }

  var systemImage: String {
    switch self {
    case .today: "sun.max"
    case .latest: "clock"
    case .saved: "bookmark"
    case .sources: "dot.radiowaves.left.and.right"
    case .models: "cpu"
    case .settings: "gearshape"
    }
  }
}
