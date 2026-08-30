import Foundation

public enum SignalAppearance: String, CaseIterable, Sendable, Equatable {
  case light
  case dark
}

public enum SemanticColorToken: String, CaseIterable, Sendable, Equatable {
  case textBackground
  case windowBackground
  case label
  case secondaryLabel
  case controlAccent
  case separator
}

public struct SemanticPalette: Sendable, Equatable {
  public let readingBackground: SemanticColorToken
  public let elevatedBackground: SemanticColorToken
  public let primaryText: SemanticColorToken
  public let secondaryText: SemanticColorToken
  public let accent: SemanticColorToken
  public let separator: SemanticColorToken

  public init(
    readingBackground: SemanticColorToken,
    elevatedBackground: SemanticColorToken,
    primaryText: SemanticColorToken,
    secondaryText: SemanticColorToken,
    accent: SemanticColorToken,
    separator: SemanticColorToken
  ) {
    self.readingBackground = readingBackground
    self.elevatedBackground = elevatedBackground
    self.primaryText = primaryText
    self.secondaryText = secondaryText
    self.accent = accent
    self.separator = separator
  }
}

public enum ReadingSurface: Sendable, Equatable {
  case opaque
}

public enum SeparatorEmphasis: Sendable, Equatable {
  case standard
  case strong
}

public enum KeyboardFocusPresentation: Sendable, Equatable {
  case systemVisible
}

public struct ReaderMotionPresentation: Sendable, Equatable {
  public let duration: Double?

  public init(reduceMotion: Bool) {
    duration = reduceMotion ? nil : 0.17
  }
}

public struct VisualPolicy: Sendable, Equatable {
  public let appearance: SignalAppearance
  public let reduceTransparency: Bool
  public let increaseContrast: Bool
  public let palette: SemanticPalette
  public let readingSurface = ReadingSurface.opaque
  public let glassAllowed: Bool
  public let separatorEmphasis: SeparatorEmphasis
  public let boundaryWidth: Double
  public let minimumControlDimension: Double = 28
  public let keyboardFocus = KeyboardFocusPresentation.systemVisible

  public init(
    reduceTransparency: Bool = false,
    increaseContrast: Bool = false,
    appearance: SignalAppearance = .light
  ) {
    self.appearance = appearance
    self.reduceTransparency = reduceTransparency
    self.increaseContrast = increaseContrast
    palette = SemanticPalette(
      readingBackground: .textBackground,
      elevatedBackground: .windowBackground,
      primaryText: .label,
      secondaryText: .secondaryLabel,
      accent: .controlAccent,
      separator: .separator
    )
    glassAllowed = !reduceTransparency
    separatorEmphasis = increaseContrast ? .strong : .standard
    boundaryWidth = increaseContrast ? 2 : 1
  }
}

public enum AccessibilityOrder: CaseIterable, Sendable, Equatable {
  case title
  case status
  case content
  case actions

  public var sortPriority: Double {
    switch self {
    case .title: 400
    case .status: 300
    case .content: 200
    case .actions: 100
    }
  }
}

public enum IconControlDescriptor: CaseIterable, Sendable, Equatable {
  case compactNavigation
  case moreActions
  case removeSource

  public var label: String {
    switch self {
    case .compactNavigation: "Choose section"
    case .moreActions: "More actions"
    case .removeSource: "Remove personal source"
    }
  }

  public var help: String {
    switch self {
    case .compactNavigation: "Open the app navigation menu"
    case .moreActions: "Open Settings or quit AI Daily Signal"
    case .removeSource: "Remove this personal source after confirmation"
    }
  }
}

public enum KeyboardCommandModifier: String, Sendable, Equatable, Hashable {
  case command
}

public struct KeyboardCommandDescriptor: Sendable, Equatable {
  public let key: String
  public let modifiers: Set<KeyboardCommandModifier>
  public let help: String

  public init(key: String, modifiers: Set<KeyboardCommandModifier>, help: String) {
    self.key = key
    self.modifiers = modifiers
    self.help = help
  }
}
