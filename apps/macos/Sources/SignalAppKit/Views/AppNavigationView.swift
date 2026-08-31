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

struct AppNavigationItemPresentation: Sendable, Equatable {
  let title: String
  let systemImage: String
  let isSelected: Bool

  init(destination: Destination, selection: Destination?) {
    title = destination.title
    systemImage = destination.systemImage
    isSelected = destination == selection
  }

  var accessibilityTraits: AccessibilityTraits {
    isSelected ? [.isSelected] : []
  }
}

struct CompactNavigationPresentation: Sendable, Equatable {
  let title: String
  let accessibilityLabel: String
  let accessibilityValue: String

  init(selection: Destination) {
    title = selection.title
    accessibilityLabel = IconControlDescriptor.compactNavigation.label
    accessibilityValue = selection.title
  }
}

struct CompactNavigationPicker: View {
  @Binding private var selection: Destination

  init(selection: Binding<Destination>) {
    _selection = selection
  }

  var body: some View {
    let presentation = CompactNavigationPresentation(selection: selection)
    Picker(selection: $selection) {
      ForEach(Destination.allCases, id: \.self) { destination in
        Label(destination.title, systemImage: destination.systemImage)
          .tag(destination)
      }
    } label: {
      Label(presentation.title, systemImage: "sidebar.left")
    }
    .pickerStyle(.menu)
    .accessibilityLabel(presentation.accessibilityLabel)
    .accessibilityValue(presentation.accessibilityValue)
    .help(IconControlDescriptor.compactNavigation.help)
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
          let presentation = AppNavigationItemPresentation(
            destination: destination,
            selection: selection
          )
          Button {
            selection = destination
          } label: {
            Image(systemName: presentation.systemImage)
              .frame(width: 36, height: 36)
          }
          .buttonStyle(.plain)
          .overlay(alignment: .leading) {
            if presentation.isSelected {
              Capsule()
                .fill(Color.accentColor)
                .frame(width: 2, height: 18)
            }
          }
          .foregroundStyle(
            presentation.isSelected
              ? Color.accentColor
              : Color(nsColor: .secondaryLabelColor)
          )
          .accessibilityLabel(presentation.title)
          .accessibilityAddTraits(presentation.accessibilityTraits)
          .help(presentation.title)
        }
        Spacer()
      }
      .padding(.vertical, 12)
      .frame(maxWidth: .infinity)
    }
  }
}
