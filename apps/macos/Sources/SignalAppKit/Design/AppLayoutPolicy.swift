import Foundation
import SwiftUI

public enum AppLayoutMode: String, Sendable, Equatable {
  case expanded
  case rail
  case compact
}

public enum AppLayoutPolicy {
  public static let expandedMinimumWidth: CGFloat = 820
  public static let railMinimumWidth: CGFloat = 560

  public static func mode(for availableWidth: CGFloat) -> AppLayoutMode {
    if availableWidth >= expandedMinimumWidth { return .expanded }
    if availableWidth >= railMinimumWidth { return .rail }
    return .compact
  }

  public static func navigationWidth(for mode: AppLayoutMode) -> CGFloat? {
    switch mode {
    case .expanded: 228
    case .rail: 58
    case .compact: nil
    }
  }
}

public enum ReadingColumnMetrics {
  public static let maximumWidth: CGFloat = 680
  public static let minimumWindowWidth: CGFloat = 420
  public static let minimumWindowHeight: CGFloat = 520

  public static func horizontalPadding(for mode: AppLayoutMode) -> CGFloat {
    switch mode {
    case .expanded: 28
    case .rail: 24
    case .compact: 18
    }
  }
}

public enum SettingsGridMetrics {
  public static let maximumWidth: CGFloat = ReadingColumnMetrics.maximumWidth
  public static let horizontalPadding: CGFloat = 28
  public static let verticalPadding: CGFloat = 20
  public static let headerBottomSpacing: CGFloat = 14
  public static let sectionSpacing: CGFloat = 16
  public static let rowVerticalPadding: CGFloat = 9
}

public enum StoryRowMetrics {
  public static let horizontalPadding: CGFloat = 12
  public static let rankWidth: CGFloat = 28
  public static let columnSpacing: CGFloat = 12

  public static func expandedContentLeadingPadding(hasRank: Bool) -> CGFloat {
    horizontalPadding + (hasRank ? rankWidth + columnSpacing : 0)
  }
}

private struct AppLayoutModeEnvironmentKey: EnvironmentKey {
  static let defaultValue = AppLayoutMode.compact
}

extension EnvironmentValues {
  var appLayoutMode: AppLayoutMode {
    get { self[AppLayoutModeEnvironmentKey.self] }
    set { self[AppLayoutModeEnvironmentKey.self] = newValue }
  }
}
