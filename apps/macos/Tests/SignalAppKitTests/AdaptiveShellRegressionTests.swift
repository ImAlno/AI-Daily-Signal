import AppKit
import SwiftUI
import Testing

@testable import SignalAppKit

@Suite(.serialized)
struct AdaptiveShellRegressionTests {
  @Test @MainActor
  func compactShellHasNoSidebarToggleAfterLayoutSettles() throws {
    // Break caught: the system sidebar toggle can reveal the icon rail below the compact breakpoint.
    let model = AppModel(
      bridge: FakeBridgeClient(snapshot: .fixture),
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    let coordinator = WindowCoordinator(model: model) { window in
      window.makeKeyAndOrderFront(nil)
    }

    coordinator.open(destination: .today)
    let window = try #require(coordinator.managedWindow)
    defer { coordinator.close() }
    window.setContentSize(NSSize(width: 480, height: 620))
    settleLayout()

    let toolbarItems = window.toolbar?.items ?? []
    let sidebarAction = #selector(NSSplitViewController.toggleSidebar(_:))
    #expect(
      !toolbarItems.contains {
        $0.itemIdentifier.rawValue == "com.apple.SwiftUI.navigationSplitView.toggleSidebar"
          || $0.action == sidebarAction
      }
    )
  }

  @Test @MainActor
  func inlineEditorsHaveOneVerticalScrollingOwnerAtMinimumWindowSize() {
    // Break caught: wrapping a scrolling Form in another vertical ScrollView traps wheel and focus scrolling.
    let sourceModel = AppModel(
      bridge: FakeBridgeClient(snapshot: .fixture),
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    sourceModel.presentSourceEditor()
    let modelModel = AppModel(
      bridge: FakeBridgeClient(snapshot: .fixture),
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    modelModel.presentModelEditor()

    let editors: [(String, AnyView)] = [
      ("Sources", AnyView(SourcesView(model: sourceModel))),
      ("Models", AnyView(ModelsSettingsView(model: modelModel))),
    ]

    for (name, editor) in editors {
      let hosted = host(editor, size: NSSize(width: 420, height: 520))

      let scrollingOwners = descendants(of: NSScrollView.self, in: hosted)
        .filter { !$0.isHidden && $0.frame.width > 0 && $0.frame.height > 0 }
      #expect(scrollingOwners.count == 1, "\(name) has \(scrollingOwners.count) scroll owners")
    }
  }

  @Test
  func railNavigationMarksOnlyTheCurrentDestinationSelected() {
    // Break caught: color is the only indication of the current destination in the icon rail.
    let selected = AppNavigationItemPresentation(
      destination: .today,
      selection: .today
    )
    let unselected = AppNavigationItemPresentation(
      destination: .latest,
      selection: .today
    )

    #expect(selected.accessibilityTraits.contains(.isSelected))
    #expect(!unselected.accessibilityTraits.contains(.isSelected))
  }

  @Test
  func compactNavigationExposesTheCurrentDestinationAsItsValue() {
    // Break caught: the compact menu announces no current destination after replacing its label.
    let today = CompactNavigationPresentation(selection: .today)
    let latest = CompactNavigationPresentation(selection: .latest)

    #expect(today.title == "Today")
    #expect(today.accessibilityLabel == IconControlDescriptor.compactNavigation.label)
    #expect(today.accessibilityValue == "Today")
    #expect(latest.title == "Latest")
    #expect(latest.accessibilityValue == "Latest")
  }
}

@MainActor
private func host(_ view: AnyView, size: NSSize) -> NSHostingView<AnyView> {
  let hostingView = NSHostingView(rootView: view)
  hostingView.frame = NSRect(origin: .zero, size: size)
  hostingView.layoutSubtreeIfNeeded()
  settleLayout()
  return hostingView
}

@MainActor
private func settleLayout() {
  RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))
}

@MainActor
private func descendants<T: NSView>(of type: T.Type, in root: NSView) -> [T] {
  root.subviews.flatMap { view in
    ((view as? T).map { [$0] } ?? []) + descendants(of: type, in: view)
  }
}
