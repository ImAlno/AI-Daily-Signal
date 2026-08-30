import AppKit
import Testing

@testable import SignalAppKit

@Suite(.serialized)
struct AppPresentationTests {
  @Test
  func firstLaunchOpensWelcomeAndCompletedLaunchStaysInMenuBar() {
    // Break caught: either hiding first-run setup or reopening the reading window every launch.
    #expect(AppPresentation(welcomeCompleted: false).launchAction == .openBriefing)
    #expect(AppPresentation(welcomeCompleted: true).launchAction == .remainInMenuBar)
  }

  @Test
  func companionUsesAccessoryActivationWithoutADockIcon() {
    // Break caught: presenting a permanent Dock icon for the menu-bar-first companion.
    #expect(AppPresentation.activationPolicy == .accessory)
  }

  @Test
  func popoverRenderPlanDrivesTheApprovedCompactSurface() {
    // Break caught: rendering an untested feed/action outside the fixed compact popover plan.
    let presentation = MenuBarPresentation(
      phase: .ready,
      snapshot: snapshotWithTwoTodaySignals,
      errorMessage: nil,
      refreshInProgress: false
    )

    #expect(
      presentation.elements == [
        .status,
        .topSignal,
        .refreshOrCancel,
        .openBriefing,
        .settings,
        .quit,
      ])
    #expect(
      presentation.actionSet == [.refreshOrCancel, .openBriefing, .settings, .quit]
    )
    #expect(presentation.scrolling == .fixedContent)
    #expect(presentation.topSignals.count == 1)
    #expect(presentation.topSignals.first?.title == "A signal")
  }

  @Test
  func firstBriefingOperationMakesThePopoverControlCancelable() {
    // Break caught: showing an inoperative Refresh button while the first refresh already owns the model.
    let presentation = MenuBarPresentation(
      phase: .buildingFirstBriefing,
      snapshot: nil,
      errorMessage: nil,
      refreshInProgress: true
    )

    #expect(presentation.refreshControl == .cancel)
  }

  @Test
  func readingWindowExposesExactlyTheApprovedDestinationsAndCommands() {
    // Break caught: adding placeholder destinations or omitting a required keyboard action.
    #expect(
      Destination.allCases == [.today, .latest, .saved, .sources, .models, .settings]
    )
    #expect(Destination.models.title == "Models")
    #expect(Destination.settings.title == "Preferences")
    #expect(ReadingCommand.allCases == [.refresh, .openSource, .save, .settings])
    #expect(ReadingCommand.refresh.keyEquivalent == "r")
    #expect(ReadingCommand.openSource.keyEquivalent == "o")
    #expect(ReadingCommand.save.keyEquivalent == "s")
    #expect(ReadingCommand.settings.keyEquivalent == ",")
  }

  @Test
  func requiredStatusesHaveDistinctSymbolsAndVoiceOverLabels() {
    // Break caught: encoding freshness only with color or collapsing offline and failed states.
    let statuses: [SignalStatus] = [
      .current, .refreshing, .smartFallback, .partiallyStale, .offline, .failed,
      .localDataUnavailable,
    ]

    #expect(Set(statuses.map(\.symbolName)).count == statuses.count)
    #expect(Set(statuses.map(\.accessibilityLabel)).count == statuses.count)
    #expect(statuses.allSatisfy { $0.accessibilityLabel.hasPrefix("AI Daily Signal, ") })
  }

  @Test @MainActor
  func coordinatorClosesAndReopensTheSameWindowThroughTheFrontOrderingSeam() {
    // Break caught: replacing the closed window or failing to bring that retained window forward again.
    let model = AppModel(
      bridge: FakeBridgeClient(snapshot: .fixture),
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    var presentedWindows: [NSWindow] = []
    let coordinator = WindowCoordinator(model: model) { window in
      presentedWindows.append(window)
    }

    coordinator.open(destination: .latest)
    let firstWindow = coordinator.managedWindow
    coordinator.close()
    coordinator.open(destination: .settings)

    #expect(firstWindow != nil)
    #expect(coordinator.managedWindow === firstWindow)
    #expect(coordinator.createdWindowCount == 1)
    #expect(presentedWindows.count == 2)
    #expect(presentedWindows[0] === firstWindow)
    #expect(presentedWindows[1] === firstWindow)
    #expect(model.destination == .settings)

    coordinator.close()
  }

  @Test @MainActor
  func saveCommandUsesTheSelectedStoryAndAppliesTheConfirmedRecord() async {
    // Break caught: making Command-S a no-op or displaying an optimistic save Rust rejected.
    let bridge = FakeBridgeClient(snapshot: .fixture)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "story-1"

    await model.saveSelectedStory()

    #expect(bridge.savedRequests.count == 1)
    #expect(bridge.savedRequests.first?.storyID == "story-1")
    #expect(bridge.savedRequests.first?.saved == true)
    #expect(model.snapshot?.revision == bridge.savedMutationRevision)
    #expect(model.snapshot?.latest.first?.isSaved == true)
    #expect(model.snapshot?.today?.items.first?.story.isSaved == true)
  }

  @Test @MainActor
  func saveCommandNeverRemovesAnAlreadySavedStory() async {
    // Break caught: wiring Command-S to the visible toggle and unsaving an already-saved story.
    let bridge = FakeBridgeClient(snapshot: snapshotWithSavedStory)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    model.selectedStoryID = "story-1"

    await model.saveSelectedStory()

    #expect(bridge.savedRequests.isEmpty)
    #expect(model.selectedStory?.isSaved == true)
    #expect(model.snapshot?.saved.first?.id == "story-1")
  }

  @Test
  func welcomeCopyMakesLocalFirstSetupAndNetworkContactExplicit() {
    // Break caught: making AI sound required or hiding that refresh contacts source websites.
    #expect(WelcomeContent.primaryAction == "Build My First Briefing")
    #expect(WelcomeContent.localFirstExplanation.contains("AI is optional"))
    #expect(WelcomeContent.refreshDisclosure.contains("enabled source websites"))
  }

  @Test @MainActor
  func bridgeConstructionFailureIsBlockingWithoutAnInoperativeRetry() async {
    // Break caught: offering Try Again through a bridge that can never recover in this process.
    let bridge = FakeBridgeClient(snapshot: .fixture)
    bridge.snapshotError = BridgeError.startupUnavailable
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )

    await model.start()
    let presentation = UnavailableContentPresentation(phase: model.phase)
    let menuBarPresentation = MenuBarPresentation(
      phase: model.phase,
      snapshot: model.snapshot,
      errorMessage: model.errorMessage,
      refreshInProgress: false
    )
    let toolbarPresentation = ReadingToolbarPresentation(
      phase: model.phase,
      refreshInProgress: false
    )

    #expect(
      model.phase
        == .startupFailure(
          message:
            "AI Daily Signal could not open its local data. Quit and reopen the app. If the problem continues, make sure this Mac has available storage."
        )
    )
    #expect(presentation?.title == "Local data unavailable")
    #expect(presentation?.action == nil)
    #expect(!presentation.orEmptyMessage.contains("/"))
    #expect(menuBarPresentation.status == .localDataUnavailable)
    #expect(menuBarPresentation.status.title == "Local data unavailable")
    #expect(menuBarPresentation.status.symbolName == "internaldrive.fill")
    #expect(
      menuBarPresentation.status.accessibilityLabel
        == "AI Daily Signal, local data unavailable"
    )
    #expect(menuBarPresentation.refreshControl == .unavailable)
    #expect(menuBarPresentation.actionSet == [.openBriefing, .settings, .quit])
    #expect(toolbarPresentation.refreshControl == .unavailable)
  }

  @Test
  func recoverableFailureRetainsRefreshInTheToolbarAndPopover() {
    // Break caught: making every failure blocking when an ordinary failed refresh can be retried.
    let phase = AppPhase.failure(message: "Something went wrong. Please try again.")
    let menuBarPresentation = MenuBarPresentation(
      phase: phase,
      snapshot: nil,
      errorMessage: "Something went wrong. Please try again.",
      refreshInProgress: false
    )
    let toolbarPresentation = ReadingToolbarPresentation(
      phase: phase,
      refreshInProgress: false
    )

    #expect(menuBarPresentation.status == .failed)
    #expect(menuBarPresentation.status.title == "Refresh failed")
    #expect(menuBarPresentation.refreshControl == .refresh)
    #expect(menuBarPresentation.actionSet.contains(.refreshOrCancel))
    #expect(toolbarPresentation.refreshControl == .refresh)
  }
}

private var snapshotWithTwoTodaySignals: AppSnapshot {
  let fixture = AppSnapshot.fixture
  let today = fixture.today!
  let first = today.items[0]
  let second = BriefingItem(
    position: 2,
    section: "More Signals",
    isStale: false,
    story: .fixture,
    selectedSummary: .fixture,
    summaryVariants: [.fixture]
  )
  return AppSnapshot(
    revision: fixture.revision,
    status: fixture.status,
    today: Briefing(
      date: today.date,
      generatedAt: today.generatedAt,
      isStale: today.isStale,
      items: [first, second]
    ),
    latest: fixture.latest,
    saved: fixture.saved,
    sources: fixture.sources,
    modelProfiles: fixture.modelProfiles,
    defaultModelProfileID: fixture.defaultModelProfileID,
    hasUsableAIProfile: fixture.hasUsableAIProfile
  )
}

private var snapshotWithSavedStory: AppSnapshot {
  let fixture = AppSnapshot.fixture
  let story = Story(
    id: Story.fixture.id,
    title: Story.fixture.title,
    canonicalURL: Story.fixture.canonicalURL,
    excerpt: Story.fixture.excerpt,
    category: Story.fixture.category,
    publishedAt: Story.fixture.publishedAt,
    sourceIDs: Story.fixture.sourceIDs,
    score: Story.fixture.score,
    smartSummary: Story.fixture.smartSummary,
    isRead: Story.fixture.isRead,
    isSaved: true,
    selectedSummary: Story.fixture.selectedSummary,
    summaryVariants: Story.fixture.summaryVariants
  )
  let item = BriefingItem(
    position: 1,
    section: "Top Signals",
    isStale: false,
    story: story,
    selectedSummary: story.selectedSummary,
    summaryVariants: story.summaryVariants
  )
  return AppSnapshot(
    revision: fixture.revision,
    status: fixture.status,
    today: Briefing(
      date: fixture.today?.date ?? "2026-08-30",
      generatedAt: fixture.today?.generatedAt,
      isStale: false,
      items: [item]
    ),
    latest: [story],
    saved: [story],
    sources: fixture.sources,
    modelProfiles: fixture.modelProfiles,
    defaultModelProfileID: fixture.defaultModelProfileID,
    hasUsableAIProfile: fixture.hasUsableAIProfile
  )
}

extension Optional where Wrapped == UnavailableContentPresentation {
  fileprivate var orEmptyMessage: String { self?.message ?? "" }
}
