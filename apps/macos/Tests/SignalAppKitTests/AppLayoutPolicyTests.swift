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
  func navigationAndReadingMetricsMatchTheApprovedShell() {
    #expect(AppLayoutPolicy.navigationWidth(for: .expanded) == 228)
    #expect(AppLayoutPolicy.navigationWidth(for: .rail) == 58)
    #expect(AppLayoutPolicy.navigationWidth(for: .compact) == nil)
    #expect(ReadingColumnMetrics.maximumWidth == 720)
    #expect(ReadingColumnMetrics.minimumWindowWidth == 420)
    #expect(ReadingColumnMetrics.minimumWindowHeight == 520)
    #expect(ReadingColumnMetrics.horizontalPadding(for: 480) == 20)
    #expect(ReadingColumnMetrics.horizontalPadding(for: 760) == 28)
    #expect(ReadingColumnMetrics.horizontalPadding(for: 1_100) == 36)
  }
}
