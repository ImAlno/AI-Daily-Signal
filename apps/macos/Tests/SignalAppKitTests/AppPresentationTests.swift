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
  func popoverContainsOnlyTheApprovedCompactElements() {
    // Break caught: turning the popover into a scrollable story feed or adding deferred actions.
    #expect(
      MenuBarElement.allCases == [
        .status,
        .topSignal,
        .refreshOrCancel,
        .openBriefing,
        .settings,
        .quit,
      ])
    #expect(!AppPresentation.menuBarAllowsStoryFeed)
  }

  @Test
  func readingWindowExposesExactlyTheApprovedDestinationsAndCommands() {
    // Break caught: adding placeholder destinations or omitting a required keyboard action.
    #expect(Destination.allCases == [.today, .latest, .saved, .sources, .settings])
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
      .current, .refreshing, .partiallyStale, .offline, .failed,
    ]

    #expect(Set(statuses.map(\.symbolName)).count == statuses.count)
    #expect(Set(statuses.map(\.accessibilityLabel)).count == statuses.count)
  }

  @Test @MainActor
  func coordinatorReusesOneWindowAndRoutesEveryOpenToTheSharedModel() {
    // Break caught: building another model/window graph each time Open Briefing is chosen.
    let model = AppModel(
      bridge: FakeBridgeClient(snapshot: .fixture),
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    let coordinator = WindowCoordinator(model: model, activatesApplication: false)

    coordinator.open(destination: .latest)
    let firstWindow = coordinator.managedWindow
    coordinator.open(destination: .settings)

    #expect(firstWindow != nil)
    #expect(coordinator.managedWindow === firstWindow)
    #expect(coordinator.createdWindowCount == 1)
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

    await model.toggleSelectedStorySaved()

    #expect(bridge.savedRequests.count == 1)
    #expect(bridge.savedRequests.first?.storyID == "story-1")
    #expect(bridge.savedRequests.first?.saved == true)
    #expect(model.snapshot?.latest.first?.isSaved == true)
    #expect(model.snapshot?.today?.items.first?.story.isSaved == true)
  }

  @Test
  func welcomeCopyMakesLocalFirstSetupAndNetworkContactExplicit() {
    // Break caught: making AI sound required or hiding that refresh contacts source websites.
    #expect(WelcomeContent.primaryAction == "Build My First Briefing")
    #expect(WelcomeContent.localFirstExplanation.contains("AI is optional"))
    #expect(WelcomeContent.refreshDisclosure.contains("enabled source websites"))
  }
}
