import AppKit
import SignalAppKit

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
  let environment = AppEnvironment()

  func applicationWillFinishLaunching(_ notification: Notification) {
    NSApplication.shared.setActivationPolicy(AppPresentation.activationPolicy)
  }

  func applicationDidFinishLaunching(_ notification: Notification) {
    let launchAction = AppPresentation(
      welcomeCompleted: environment.preferences.welcomeCompleted
    ).launchAction
    Task {
      await environment.model.start()
      if launchAction == .openBriefing {
        environment.windowCoordinator.open()
      }
    }
  }

  func applicationDidBecomeActive(_ notification: Notification) {
    environment.model.setActive(true)
  }

  func applicationDidResignActive(_ notification: Notification) {
    environment.model.setActive(false)
  }

  func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    false
  }
}
