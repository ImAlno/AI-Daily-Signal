import AppKit
import SwiftUI

public enum AppLaunchAction: Sendable, Equatable {
  case openBriefing
  case remainInMenuBar
}

public enum MenuBarElement: CaseIterable, Sendable, Equatable {
  case status
  case topSignal
  case refreshOrCancel
  case openBriefing
  case settings
  case quit
}

public struct AppPresentation {
  public static let activationPolicy = NSApplication.ActivationPolicy.accessory
  public static let menuBarAllowsStoryFeed = false

  public let launchAction: AppLaunchAction

  public init(welcomeCompleted: Bool) {
    launchAction = welcomeCompleted ? .remainInMenuBar : .openBriefing
  }
}

@MainActor
public final class WindowCoordinator: NSObject, NSWindowDelegate {
  private let model: AppModel
  private let activatesApplication: Bool

  private(set) var managedWindow: NSWindow?
  private(set) var createdWindowCount = 0

  public init(model: AppModel, activatesApplication: Bool = true) {
    self.model = model
    self.activatesApplication = activatesApplication
  }

  public func open(destination: Destination = .today) {
    model.destination = destination
    let window = managedWindow ?? makeWindow()
    guard activatesApplication else { return }
    NSApplication.shared.activate()
    window.makeKeyAndOrderFront(nil)
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
    window.minSize = NSSize(width: 860, height: 600)
    window.isReleasedWhenClosed = false
    window.toolbarStyle = .unified
    window.contentViewController = NSHostingController(rootView: ReadingWindowView(model: model))
    window.delegate = self
    window.center()
    window.setFrameAutosaveName("AI Daily Signal Reading Window")
    managedWindow = window
    createdWindowCount += 1
    return window
  }
}
