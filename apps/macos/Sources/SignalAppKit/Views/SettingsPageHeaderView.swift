import SwiftUI

public struct SettingsPageHeaderView: View {
  public let title: String
  public let message: String
  @ScaledMetric(relativeTo: .largeTitle) private var titleSize = 30.0

  public init(title: String, message: String) {
    self.title = title
    self.message = message
  }

  public var body: some View {
    VStack(alignment: .leading, spacing: 5) {
      Text(title)
        .font(.system(size: titleSize, weight: .semibold))
        .accessibilityAddTraits(.isHeader)
        .accessibilitySortPriority(AccessibilityOrder.title.sortPriority)
      Text(message)
        .font(.callout)
        .foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}
