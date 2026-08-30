import SwiftUI

public struct WelcomePresentation: Sendable, Equatable {
  public let isPresented: Bool
  public let showsProgress: Bool
  public let primaryActionEnabled: Bool

  public init(phase: AppPhase) {
    isPresented = phase == .welcome || phase == .buildingFirstBriefing
    showsProgress = phase == .buildingFirstBriefing
    primaryActionEnabled = phase == .welcome
  }
}

public enum WelcomeContent {
  public static let primaryAction = "Build My First Briefing"
  public static let localFirstExplanation =
    "Your briefing is built and stored on this Mac. AI is optional and can be configured later."
  public static let refreshDisclosure =
    "Refreshing contacts your enabled source websites to check for new stories."
}

public struct WelcomeView: View {
  @Bindable private var model: AppModel

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    let presentation = WelcomePresentation(phase: model.phase)
    VStack(spacing: 24) {
      Image(systemName: "dot.radiowaves.left.and.right")
        .font(.system(size: 44, weight: .medium))
        .foregroundStyle(.tint)
        .accessibilityHidden(true)

      VStack(spacing: 10) {
        Text("AI Daily Signal")
          .font(.largeTitle.weight(.semibold))
          .accessibilityAddTraits(.isHeader)
          .accessibilitySortPriority(AccessibilityOrder.title.sortPriority)
        Text("A focused daily briefing for understanding what changed in AI.")
          .font(.title3)
          .foregroundStyle(.secondary)
          .multilineTextAlignment(.center)
      }

      Text(WelcomeContent.localFirstExplanation)
        .font(.body)
        .foregroundStyle(.secondary)
        .multilineTextAlignment(.center)

      Button(WelcomeContent.primaryAction) {
        Task { await model.buildFirstBriefing() }
      }
      .buttonStyle(.borderedProminent)
      .controlSize(.large)
      .disabled(!presentation.primaryActionEnabled)
      .accessibilityHint("Initializes the standard source pack and refreshes it")
      .accessibilitySortPriority(AccessibilityOrder.actions.sortPriority)

      if presentation.showsProgress {
        ProgressView("Building your briefing…")
          .controlSize(.small)
      }

      Label(WelcomeContent.refreshDisclosure, systemImage: "network")
        .font(.caption)
        .foregroundStyle(.tertiary)
        .multilineTextAlignment(.center)
        .accessibilitySortPriority(AccessibilityOrder.content.sortPriority)
    }
    .frame(maxWidth: 480)
    .padding(64)
  }
}
