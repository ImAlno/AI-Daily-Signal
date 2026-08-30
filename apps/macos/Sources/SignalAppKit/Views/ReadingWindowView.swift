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
  @State private var columnVisibility = NavigationSplitViewVisibility.all

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
      let navigationPresentation = AppNavigationPresentation(mode: mode)
      let navigationWidth = AppLayoutPolicy.navigationWidth(for: mode) ?? 58

      NavigationSplitView(columnVisibility: $columnVisibility) {
        AppNavigationView(mode: mode, selection: destinationSelection)
          .navigationSplitViewColumnWidth(
            min: navigationWidth,
            ideal: navigationWidth,
            max: navigationWidth
          )
      } detail: {
        destinationContent
          .navigationTitle(model.destination.title)
          .background(Color(nsColor: .textBackgroundColor))
      }
      .onAppear {
        columnVisibility = navigationPresentation.persistentNavigationVisible ? .all : .detailOnly
      }
      .onChange(of: mode) { _, value in
        columnVisibility =
          AppNavigationPresentation(mode: value).persistentNavigationVisible ? .all : .detailOnly
      }
      .toolbar {
        toolbarContent(mode: mode)
      }
    }
    .safeAreaInset(edge: .top) {
      if model.errorMessage == nil, model.activeOperationID == nil, let notice = model.refreshNotice {
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
  private func toolbarContent(mode: AppLayoutMode) -> some ToolbarContent {
    if AppNavigationPresentation(mode: mode).usesToolbarMenu {
      ToolbarItem(placement: .navigation) {
        Menu {
          ForEach(Destination.allCases, id: \.self) { destination in
            Button(destination.title, systemImage: destination.systemImage) {
              model.destination = destination
            }
          }
        } label: {
          Label(model.destination.title, systemImage: "sidebar.left")
        }
        .accessibilityLabel(IconControlDescriptor.compactNavigation.label)
        .help(IconControlDescriptor.compactNavigation.help)
      }
    }

    ToolbarItem(placement: .primaryAction) {
      refreshToolbarButton(
        ReadingToolbarPresentation(
          phase: model.phase,
          refreshInProgress: model.activeOperationID != nil
        ).refreshControl
      )
    }

    if AppNavigationPresentation(mode: mode).usesToolbarMenu {
      ToolbarItem(placement: .primaryAction) {
        Menu {
          if storyCommandsAvailable {
            openSourceToolbarButton
            saveToolbarButton
          }
          preferencesToolbarButton
        } label: {
          Label("More", systemImage: "ellipsis.circle")
        }
        .help("Open story actions and Preferences")
      }
    } else {
      ToolbarItemGroup(placement: .primaryAction) {
        if storyCommandsAvailable {
          openSourceToolbarButton
          saveToolbarButton
        }
        preferencesToolbarButton
      }
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
