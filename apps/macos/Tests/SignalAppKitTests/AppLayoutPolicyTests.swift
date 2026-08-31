import Testing

@testable import SignalAppKit

struct AppLayoutPolicyTests {
  @Test
  func exactLayoutBreakpointsAreStable() {
    #expect(AppLayoutPolicy.mode(for: 559) == .compact)
    #expect(AppLayoutPolicy.mode(for: 560) == .rail)
    #expect(AppLayoutPolicy.mode(for: 819) == .rail)
    #expect(AppLayoutPolicy.mode(for: 820) == .expanded)
  }

  @Test
  func navigationAndReadingMetricsMatchTheApprovedEditorialColumn() {
    // Break caught: letting the reading measure or responsive gutters drift from the approved editorial layout.
    #expect(AppLayoutPolicy.navigationWidth(for: .expanded) == 228)
    #expect(AppLayoutPolicy.navigationWidth(for: .rail) == 58)
    #expect(AppLayoutPolicy.navigationWidth(for: .compact) == nil)
    #expect(ReadingColumnMetrics.maximumWidth == 680)
    #expect(ReadingColumnMetrics.minimumWindowWidth == 420)
    #expect(ReadingColumnMetrics.minimumWindowHeight == 520)
    #expect(ReadingColumnMetrics.horizontalPadding(for: .compact) == 18)
    #expect(ReadingColumnMetrics.horizontalPadding(for: .rail) == 24)
    #expect(ReadingColumnMetrics.horizontalPadding(for: .expanded) == 28)
  }

  @Test(arguments: [AppLayoutMode.expanded, .rail, .compact])
  func navigationPresentationMatchesEachLayoutMode(mode: AppLayoutMode) {
    // Break caught: showing the wrong navigation surface for a responsive layout mode.
    let presentation = AppNavigationPresentation(mode: mode)

    switch mode {
    case .expanded:
      #expect(presentation.persistentNavigationVisible)
      #expect(presentation.showsDestinationTitles)
      #expect(!presentation.usesToolbarMenu)
    case .rail:
      #expect(presentation.persistentNavigationVisible)
      #expect(!presentation.showsDestinationTitles)
      #expect(!presentation.usesToolbarMenu)
    case .compact:
      #expect(!presentation.persistentNavigationVisible)
      #expect(!presentation.showsDestinationTitles)
      #expect(presentation.usesToolbarMenu)
    }
  }
}
