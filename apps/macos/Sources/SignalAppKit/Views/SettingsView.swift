import SwiftUI

public struct PreferencesPresentation: Sendable, Equatable {
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

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    let presentation = PreferencesPresentation(
      hasUsableAIProfile: model.snapshot?.hasUsableAIProfile == true
    )
    Form {
      Section("Briefing") {
        LabeledContent("Storage", value: presentation.storage)
        LabeledContent("AI summaries", value: presentation.aiSummaries)
      }
      Section("Companion") {
        LabeledContent("Launch behavior", value: presentation.launchBehavior)
        LabeledContent("CLI", value: presentation.cliCompatibility)
      }
    }
    .formStyle(.grouped)
    .frame(maxWidth: ReadingColumnMetrics.maximumWidth)
    .padding(.horizontal, 24)
    .padding(.vertical, 20)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
  }
}
