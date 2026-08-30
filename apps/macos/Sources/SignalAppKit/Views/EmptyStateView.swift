import SwiftUI

public struct EmptyStateView: View {
  private let title: String
  private let message: String
  private let systemImage: String
  private let actionTitle: String?
  private let action: (() -> Void)?

  public init(
    title: String,
    message: String,
    systemImage: String,
    actionTitle: String? = nil,
    action: (() -> Void)? = nil
  ) {
    self.title = title
    self.message = message
    self.systemImage = systemImage
    self.actionTitle = actionTitle
    self.action = action
  }

  public var body: some View {
    ContentUnavailableView {
      Label(title, systemImage: systemImage)
    } description: {
      Text(message)
    } actions: {
      if let actionTitle, let action {
        Button(actionTitle, action: action)
          .buttonStyle(.borderedProminent)
      }
    }
  }
}
