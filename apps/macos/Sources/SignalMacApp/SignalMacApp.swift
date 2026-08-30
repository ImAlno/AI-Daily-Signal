import AppKit
import SignalAppKit
import SwiftUI

@main
struct SignalMacApp: App {
  @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

  var body: some Scene {
    MenuBarExtra {
      MenuBarContentView(
        model: appDelegate.environment.model,
        openBriefing: {
          appDelegate.environment.windowCoordinator.open()
        },
        openSettings: {
          appDelegate.environment.windowCoordinator.open(destination: .settings)
        },
        quit: {
          NSApplication.shared.terminate(nil)
        }
      )
    } label: {
      MenuBarStatusLabel(model: appDelegate.environment.model)
    }
    .menuBarExtraStyle(.window)
  }
}
