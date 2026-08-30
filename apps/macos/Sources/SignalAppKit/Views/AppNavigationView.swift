import AppKit
import SwiftUI

public struct AppNavigationPresentation: Sendable, Equatable {
  public let persistentNavigationVisible: Bool
  public let showsDestinationTitles: Bool
  public let usesToolbarMenu: Bool

  public init(mode: AppLayoutMode) {
    persistentNavigationVisible = mode != .compact
    showsDestinationTitles = mode == .expanded
    usesToolbarMenu = mode == .compact
  }
}

public struct AppNavigationView: View {
  private let mode: AppLayoutMode
  @Binding private var selection: Destination?

  public init(mode: AppLayoutMode, selection: Binding<Destination?>) {
    self.mode = mode
    _selection = selection
  }

  public var body: some View {
    if AppNavigationPresentation(mode: mode).showsDestinationTitles {
      List(Destination.allCases, id: \.self, selection: $selection) { destination in
        Label(destination.title, systemImage: destination.systemImage)
          .tag(destination)
      }
      .navigationTitle("AI Daily Signal")
    } else {
      VStack(spacing: 8) {
        ForEach(Destination.allCases, id: \.self) { destination in
          Button {
            selection = destination
          } label: {
            Image(systemName: destination.systemImage)
              .frame(
                minWidth: VisualPolicy().minimumControlDimension,
                minHeight: VisualPolicy().minimumControlDimension
              )
          }
          .buttonStyle(.plain)
          .foregroundStyle(
            selection == destination
              ? Color.accentColor
              : Color(nsColor: .secondaryLabelColor)
          )
          .accessibilityLabel(destination.title)
          .help(destination.title)
        }
        Spacer()
      }
      .padding(.vertical, 12)
      .frame(maxWidth: .infinity)
    }
  }
}
