import AppKit
import SwiftUI

public enum AppLaunchAction: Sendable, Equatable {
  case openBriefing
  case remainInMenuBar
}

public enum MenuBarElement: Sendable, Equatable {
  case status
  case topSignal
  case refreshOrCancel
  case openBriefing
  case settings
  case quit
}

public struct AppPresentation {
  public static let activationPolicy = NSApplication.ActivationPolicy.accessory

  public let launchAction: AppLaunchAction

  public init(welcomeCompleted: Bool) {
    launchAction = welcomeCompleted ? .remainInMenuBar : .openBriefing
  }
}

@MainActor
public final class WindowCoordinator: NSObject, NSWindowDelegate {
  private let model: AppModel
  private let presentWindow: @MainActor (NSWindow) -> Void

  private(set) var managedWindow: NSWindow?
  private(set) var createdWindowCount = 0

  public init(
    model: AppModel,
    presentWindow: @escaping @MainActor (NSWindow) -> Void = { window in
      NSApplication.shared.activate()
      window.makeKeyAndOrderFront(nil)
    }
  ) {
    self.model = model
    self.presentWindow = presentWindow
  }

  public func open(destination: Destination = .today) {
    model.destination = destination
    let window = managedWindow ?? makeWindow()
    presentWindow(window)
  }

  public func close() {
    managedWindow?.close()
  }

  private func makeWindow() -> NSWindow {
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 1_120, height: 760),
      styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
      backing: .buffered,
      defer: false
    )
    window.title = "AI Daily Signal"
    window.isReleasedWhenClosed = false
    window.toolbarStyle = .unified
    let hostingController = NSHostingController(rootView: ReadingWindowView(model: model))
    // The full-size content root already owns the unified toolbar region. Propagating that region
    // as a safe-area inset would add its height to SwiftUI's 520-point minimum after presentation.
    hostingController.safeAreaRegions = []
    window.contentViewController = hostingController
    window.contentMinSize = NSSize(
      width: ReadingColumnMetrics.minimumWindowWidth,
      height: ReadingColumnMetrics.minimumWindowHeight
    )
    window.delegate = self
    window.center()
    window.setFrameAutosaveName("AI Daily Signal Reading Window")
    managedWindow = window
    createdWindowCount += 1
    return window
  }
}
