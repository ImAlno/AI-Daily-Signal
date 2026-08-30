import Foundation

public enum Destination: String, CaseIterable, Sendable, Equatable {
  case today
  case latest
  case saved
  case sources
  case settings
}

@MainActor
public protocol AppPreferences: AnyObject {
  var welcomeCompleted: Bool { get set }
  var selectedDestination: Destination { get set }
}

@MainActor
public final class UserDefaultsAppPreferences: AppPreferences {
  private enum Key {
    static let welcomeCompleted = "welcomeCompleted"
    static let selectedDestination = "selectedDestination"
  }

  private let defaults: UserDefaults

  public init(defaults: UserDefaults = .standard) {
    self.defaults = defaults
  }

  public var welcomeCompleted: Bool {
    get { defaults.bool(forKey: Key.welcomeCompleted) }
    set { defaults.set(newValue, forKey: Key.welcomeCompleted) }
  }

  public var selectedDestination: Destination {
    get {
      defaults.string(forKey: Key.selectedDestination)
        .flatMap(Destination.init(rawValue:)) ?? .today
    }
    set { defaults.set(newValue.rawValue, forKey: Key.selectedDestination) }
  }
}

@MainActor
public final class MemoryAppPreferences: AppPreferences {
  public var welcomeCompleted: Bool
  public var selectedDestination: Destination

  public init(welcomeCompleted: Bool = false, selectedDestination: Destination = .today) {
    self.welcomeCompleted = welcomeCompleted
    self.selectedDestination = selectedDestination
  }
}
