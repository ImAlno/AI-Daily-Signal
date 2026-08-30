import Testing

@testable import SignalAppKit

@Suite(.serialized)
struct AccessibilityPolicyTests {
  @Test
  func reducedTransparencyUsesOpaqueSurfacesAndVisibleBoundaries() {
    // Break caught: leaving translucent glass behind when the system requests opaque surfaces.
    let policy = VisualPolicy(reduceTransparency: true, increaseContrast: false)

    #expect(policy.readingSurface == .opaque)
    #expect(policy.glassAllowed == false)
    #expect(policy.separatorEmphasis == .standard)
    #expect(policy.boundaryWidth >= 1)
  }

  @Test
  func lightAndDarkAppearancesUseSemanticSystemColors() {
    // Break caught: replacing adaptive AppKit colors with fixed light-only or dark-only values.
    let expected = SemanticPalette(
      readingBackground: .textBackground,
      elevatedBackground: .windowBackground,
      primaryText: .label,
      secondaryText: .secondaryLabel,
      accent: .controlAccent,
      separator: .separator
    )

    #expect(VisualPolicy(appearance: .light).palette == expected)
    #expect(VisualPolicy(appearance: .dark).palette == expected)
  }

  @Test
  func increasedContrastStrengthensBoundariesWithoutMakingProseTranslucent() {
    // Break caught: treating Increase Contrast as an accent-color change instead of stronger edges.
    let standard = VisualPolicy(reduceTransparency: false, increaseContrast: false)
    let increased = VisualPolicy(reduceTransparency: false, increaseContrast: true)

    #expect(standard.separatorEmphasis == .standard)
    #expect(increased.separatorEmphasis == .strong)
    #expect(increased.boundaryWidth > standard.boundaryWidth)
    #expect(increased.readingSurface == .opaque)
  }

  @Test
  func controlsAndKeyboardFocusMeetTheMacAccessibilityFloor() {
    // Break caught: polish that produces undersized targets or suppresses the native focus ring.
    let policy = VisualPolicy()

    #expect(policy.minimumControlDimension >= 28)
    #expect(policy.keyboardFocus == .systemVisible)
  }

  @Test
  func reduceMotionDisablesReaderPolishAnimation() {
    // Break caught: animating disclosure or hover changes after Reduce Motion is enabled.
    #expect(ReaderMotionPresentation(reduceMotion: false).duration == 0.17)
    #expect(ReaderMotionPresentation(reduceMotion: true).duration == nil)
  }

  @Test
  func accessibilitySortPrioritiesRemainStableAndUnique() {
    // Break caught: VoiceOver visiting actions before the story identity and status context.
    #expect(AccessibilityOrder.title.sortPriority == 400)
    #expect(AccessibilityOrder.status.sortPriority == 300)
    #expect(AccessibilityOrder.content.sortPriority == 200)
    #expect(AccessibilityOrder.actions.sortPriority == 100)
    #expect(Set(AccessibilityOrder.allCases.map(\.sortPriority)).count == 4)
  }

  @Test
  func statusesAlwaysPairDistinctSymbolsWithPlainLanguage() {
    // Break caught: making current/stale/error states distinguishable only through tint.
    let statuses = SignalStatus.allCases

    #expect(Set(statuses.map(\.symbolName)).count == statuses.count)
    #expect(Set(statuses.map(\.title)).count == statuses.count)
    #expect(statuses.allSatisfy { !$0.accessibilityLabel.isEmpty })
    #expect(SignalStatus.refreshing.title.contains("Refreshing"))
    #expect(SignalStatus.partiallyStale.title.contains("Partially stale"))
  }

  @Test
  func iconOnlyControlsExposeLabelsAndHelp() {
    // Break caught: adding a visually recognizable glyph that VoiceOver or keyboard users cannot identify.
    let controls = IconControlDescriptor.allCases

    #expect(controls.count == 3)
    #expect(controls.allSatisfy { !$0.label.isEmpty && !$0.help.isEmpty })
    #expect(Set(controls.map(\.label)).count == controls.count)
    #expect(!IconControlDescriptor.compactNavigation.label.isEmpty)
    #expect(!IconControlDescriptor.compactNavigation.help.isEmpty)
  }

  @Test
  func sourceRemovalAccessibilityExplainsItsConfirmationStep() {
    // Break caught: a destructive source action that VoiceOver users cannot identify as confirmed.
    #expect(IconControlDescriptor.removeSource.label == "Remove personal source")
    #expect(IconControlDescriptor.removeSource.help.contains("confirmation"))
  }

  @Test
  func approvedKeyboardCommandsHaveTruthfulDescriptors() {
    // Break caught: a view and its displayed shortcut help drifting onto different key mappings.
    let expected: [(ReadingCommand, String, String)] = [
      (.refresh, "r", "Refresh briefing (⌘R)"),
      (.openSource, "o", "Open selected story source (⌘O)"),
      (.save, "s", "Save selected story (⌘S)"),
      (.settings, ",", "Open Preferences (⌘,)"),
    ]

    for (command, key, help) in expected {
      #expect(command.descriptor.key == key)
      #expect(command.descriptor.modifiers == [.command])
      #expect(command.descriptor.help == help)
    }
  }

  @Test
  func visibleStorySaveToggleHasTruthfulDynamicHelp() {
    // Break caught: describing a destructive remove action as Save Story.
    let save = StorySaveTogglePresentation(isSaved: false)
    let remove = StorySaveTogglePresentation(isSaved: true)

    #expect(save.title == "Save")
    #expect(save.help == "Save this story")
    #expect(remove.title == "Remove from Saved")
    #expect(remove.help == "Remove this story from Saved")
  }
}
