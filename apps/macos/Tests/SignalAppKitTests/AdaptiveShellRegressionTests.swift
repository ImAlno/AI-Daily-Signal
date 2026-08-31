import AppKit
import SwiftUI
import Testing

@testable import SignalAppKit

@Suite(.serialized)
struct AdaptiveShellRegressionTests {
  @Test @MainActor
  func readingGuttersFollowTheShellModeAtExactBoundaries() async throws {
    // Break caught: recomputing the shell breakpoint from the narrower post-navigation detail pane.
    let boundaries: [(windowWidth: CGFloat, padding: CGFloat)] = [
      (560, 24),
      (820, 28),
    ]

    for boundary in boundaries {
      let model = AppModel(
        bridge: FakeBridgeClient(snapshot: .fixture),
        preferences: MemoryAppPreferences(welcomeCompleted: true)
      )
      await model.start()
      model.selectedStoryID = nil
      let mode = AppLayoutPolicy.mode(for: boundary.windowWidth)
      let navigationWidth = try #require(AppLayoutPolicy.navigationWidth(for: mode))
      let hosted = host(
        AnyView(
          ReadingDestinationLayout(mode: mode) {
            TodayView(model: model)
          }
        ),
        size: NSSize(width: boundary.windowWidth - navigationWidth, height: 620)
      )
      let storyTarget = try #require(
        descendants(of: NSView.self, in: hosted).first { view in
          String(describing: type(of: view)).contains("FocusRingView")
            && view.frame.height > 50
            && view.frame.width > 200
        }
      )
      let storyFrame = storyTarget.convert(storyTarget.bounds, to: hosted)

      #expect(abs(storyFrame.minX - boundary.padding) < 1)
    }
  }

  @Test @MainActor
  func settingsGuttersFollowCompactAndRailShellModesAtTheirBoundaries() async throws {
    // Break caught: settings pages retaining the expanded 28-point gutter in narrower shells.
    let boundaries: [(mode: AppLayoutMode, detailWidth: CGFloat, padding: CGFloat)] = [
      (.compact, ReadingColumnMetrics.minimumWindowWidth, 18),
      (
        .rail,
        AppLayoutPolicy.railMinimumWidth
          - (AppLayoutPolicy.navigationWidth(for: .rail) ?? 0),
        24
      ),
    ]

    for boundary in boundaries {
      let model = AppModel(
        bridge: FakeBridgeClient(snapshot: .fixture),
        preferences: MemoryAppPreferences(welcomeCompleted: true)
      )
      await model.start()
      let hosted = host(
        AnyView(
          ReadingDestinationLayout(mode: boundary.mode) {
            SourcesView(model: model)
          }
        ),
        size: NSSize(width: boundary.detailWidth, height: 620)
      )
      let rowControl = try #require(
        descendants(of: NSControl.self, in: hosted).first { control in
          control.frame.width < 80 && control.frame.height < 80
        }
      )
      let controlFrame = rowControl.convert(rowControl.bounds, to: hosted)

      #expect(abs(hosted.bounds.maxX - controlFrame.maxX - boundary.padding) < 1)
    }
  }

  @Test @MainActor
  func expandedSignalConsumesSeparateIdentityWhileCollapsedSignalRemainsCombined() async throws {
    // Break caught: retaining the collapsed combined button when the expanded article needs a
    // navigable title, status, selector, sections, and actions.
    let model = AppModel(
      bridge: FakeBridgeClient(snapshot: .fixture),
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    let row = StoryRowPresentation(
      story: .fixture,
      primarySource: "Example",
      relativeTime: "now",
      isStale: false,
      rank: 1,
      summarySelection: .ai(variantID: "variant-1")
    )

    model.selectedStoryID = nil
    let collapsed = host(
      AnyView(SignalDisclosureView(presentation: row, model: model)),
      size: NSSize(width: 680, height: 220)
    )
    let collapsedControls = descendants(of: NSView.self, in: collapsed).filter {
      String(describing: type(of: $0)).contains("FocusRingView")
    }
    let collapsedSelectableTitles = descendants(of: NSTextField.self, in: collapsed).filter {
      $0.stringValue == row.title
    }
    let collapsedControl = try #require(collapsedControls.first)
    #expect(collapsedControls.count == 1)
    #expect(collapsedControl.frame.width > 600)
    #expect(collapsedSelectableTitles.isEmpty)

    model.selectedStoryID = row.storyID
    let expanded = host(
      AnyView(SignalDisclosureView(presentation: row, model: model)),
      size: NSSize(width: 680, height: 620)
    )
    let expandedControls = descendants(of: NSView.self, in: expanded).filter {
      String(describing: type(of: $0)).contains("FocusRingView")
    }
    let collapseControl = try #require(
      expandedControls.first {
        $0.frame.width == VisualPolicy().minimumControlDimension
          && $0.frame.height == VisualPolicy().minimumControlDimension
          && $0.frame.maxX > 650
      }
    )
    let expandedSelectableText = descendants(of: NSTextField.self, in: expanded)
    let title = try #require(expandedSelectableText.first { $0.stringValue == row.title })
    let whatHappened = try #require(
      expandedSelectableText.first { $0.stringValue == "What" }
    )
    let whyItMatters = try #require(
      expandedSelectableText.first { $0.stringValue == "Why" }
    )

    let titleFrame = title.convert(title.bounds, to: expanded)
    let whatHappenedFrame = whatHappened.convert(whatHappened.bounds, to: expanded)
    let whyItMattersFrame = whyItMatters.convert(whyItMatters.bounds, to: expanded)
    #expect(titleFrame.minY < whatHappenedFrame.minY)
    #expect(whatHappenedFrame.minY < whyItMattersFrame.minY)
    #expect(abs(titleFrame.minX - whatHappenedFrame.minX) < 1)
    #expect(collapseControl.frame.width < collapsedControl.frame.width)

    let expandedLayers = descendantLayers(of: try #require(expanded.layer))
    #expect(
      !expandedLayers.contains {
        $0.cornerRadius >= 6
          && ($0.backgroundColor?.alpha ?? 0) > 0
          && $0.frame.width > 300
          && $0.frame.height > 30
      },
      "Expanded reading content must remain inline instead of gaining a rounded story card"
    )

    let unrankedRow = StoryRowPresentation(
      story: .fixture,
      primarySource: "Example",
      relativeTime: "now",
      isStale: false,
      rank: nil,
      summarySelection: .ai(variantID: "variant-1")
    )
    let unranked = host(
      AnyView(SignalDisclosureView(presentation: unrankedRow, model: model)),
      size: NSSize(width: 680, height: 620)
    )
    let unrankedLayers = descendantLayers(of: try #require(unranked.layer))
    #expect(
      unrankedLayers.contains {
        ($0.backgroundColor?.alpha ?? 0) > 0
          && $0.frame.width <= 3
          && $0.frame.height >= 20
      },
      "Expanded unranked stories need a thin signal indicator instead of a card"
    )
  }

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

  @Test @MainActor
  func railNavigationMakesTheCurrentDestinationVisiblySelected() throws {
    // Break caught: returning the active rail item to a prominent rounded selection tile.
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
    settleLayout()
    window.setContentSize(NSSize(width: 760, height: 620))
    settleLayout()

    let hosted = try #require(window.contentView)
    let selectedPresentation = AppNavigationItemPresentation(
      destination: .today,
      selection: .today
    )
    let unselectedPresentation = AppNavigationItemPresentation(
      destination: .latest,
      selection: .today
    )
    let sidebar = try #require(
      descendants(of: NSView.self, in: hosted).first {
        String(describing: type(of: $0)).contains("SidebarStyleContext")
      }
    )

    let targets = descendants(of: NSView.self, in: sidebar).filter {
      String(describing: type(of: $0)).contains("KeyViewProxy")
    }
    let selectedTarget = try #require(targets.first)
    let layers = descendantLayers(of: try #require(sidebar.layer))

    #expect(selectedPresentation.accessibilityTraits.contains(.isSelected))
    #expect(!unselectedPresentation.accessibilityTraits.contains(.isSelected))
    #expect(
      !layers.contains {
        $0.frame.equalTo(selectedTarget.frame)
          && ($0.backgroundColor?.alpha ?? 0) > 0
      },
      "Rail selection must not fill the full 36 by 36 hit target"
    )
  }

  @Test @MainActor
  func settingsPagesUseContinuousRowStacksInsteadOfInsetOrGroupedContainers() async {
    // Break caught: restoring inset List or grouped Form containers beneath aligned page headers.
    let model = AppModel(
      bridge: FakeBridgeClient(snapshot: .fixture),
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()

    let pages: [(title: String, view: AnyView)] = [
      ("Sources", AnyView(SourcesView(model: model))),
      ("Models", AnyView(ModelsSettingsView(model: model))),
      ("Preferences", AnyView(SettingsView(model: model))),
    ]

    for page in pages {
      let hosted = host(page.view, size: NSSize(width: 680, height: 620))
      #expect(
        descendants(of: NSTableView.self, in: hosted).isEmpty,
        "\(page.title) must not use an inset table-backed composition"
      )
      let roundedContainers = descendantLayers(of: hosted.layer ?? CALayer()).filter {
        $0.cornerRadius >= 6
          && ($0.backgroundColor?.alpha ?? 0) > 0
          && $0.frame.width > 180
          && $0.frame.height > 30
      }
      #expect(
        roundedContainers.isEmpty,
        "\(page.title) must not use prominent grouped-card containers"
      )
    }
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
  hostingView.wantsLayer = true
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

@MainActor
private func descendantLayers(of root: CALayer) -> [CALayer] {
  [root] + (root.sublayers ?? []).flatMap(descendantLayers(of:))
}
