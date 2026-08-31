import SwiftUI

public struct PreferencesPresentation: Sendable, Equatable {
  public let title = "Preferences"
  public let guidance = "Review how the companion stores data and works with the CLI."
  public let storage = "On this Mac"
  public let aiSummaries: String
  public let launchBehavior = "Menu bar companion"
  public let cliCompatibility = "Shares local data and configuration"

  public init(hasUsableAIProfile: Bool) {
    aiSummaries = hasUsableAIProfile ? "Enabled" : "Optional"
  }
}

public struct SettingsView: View {
  private let model: AppModel
  @Environment(\.appLayoutMode) private var layoutMode

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    let presentation = PreferencesPresentation(
      hasUsableAIProfile: model.snapshot?.hasUsableAIProfile == true
    )
    ScrollView {
      LazyVStack(alignment: .leading, spacing: 0) {
        SettingsPageHeaderView(title: presentation.title, message: presentation.guidance)
          .padding(.bottom, SettingsGridMetrics.headerBottomSpacing)

        settingsSection("Briefing") {
          settingsRow("Storage", value: presentation.storage)
          settingsRow("AI summaries", value: presentation.aiSummaries)
        }
        settingsSection("Companion") {
          settingsRow("Launch behavior", value: presentation.launchBehavior)
          settingsRow("CLI", value: presentation.cliCompatibility)
        }
      }
      .frame(maxWidth: SettingsGridMetrics.maximumWidth, alignment: .leading)
      .padding(.horizontal, SettingsGridMetrics.horizontalPadding(for: layoutMode))
      .padding(.vertical, SettingsGridMetrics.verticalPadding)
      .frame(maxWidth: .infinity, alignment: .center)
    }
    .background(Color(nsColor: .textBackgroundColor))
  }

  private func settingsSection<Content: View>(
    _ title: String,
    @ViewBuilder content: () -> Content
  ) -> some View {
    VStack(alignment: .leading, spacing: 0) {
      Text(title)
        .font(.caption.weight(.medium))
        .foregroundStyle(.secondary)
        .padding(.top, SettingsGridMetrics.sectionSpacing)
        .padding(.bottom, 5)
        .accessibilityAddTraits(.isHeader)
      content()
    }
  }

  private func settingsRow(_ title: String, value: String) -> some View {
    VStack(spacing: 0) {
      LabeledContent(title, value: value)
        .padding(.vertical, SettingsGridMetrics.rowVerticalPadding)
      Divider()
    }
  }
}
