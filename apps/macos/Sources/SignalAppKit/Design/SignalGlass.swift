import AppKit
import SwiftUI

public struct SignalGlass<Content: View>: View {
  @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
  @Environment(\.colorSchemeContrast) private var colorSchemeContrast
  @Environment(\.colorScheme) private var colorScheme
  private let content: Content

  public init(@ViewBuilder content: () -> Content) {
    self.content = content()
  }

  public var body: some View {
    let policy = VisualPolicy(
      reduceTransparency: reduceTransparency,
      increaseContrast: colorSchemeContrast == .increased,
      appearance: colorScheme == .dark ? .dark : .light
    )

    if policy.glassAllowed {
      content
        .glassEffect(.regular.interactive(), in: RoundedRectangle(cornerRadius: 10))
        .overlay {
          RoundedRectangle(cornerRadius: 10)
            .stroke(.separator, lineWidth: policy.boundaryWidth)
        }
    } else {
      content
        .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 10))
        .overlay {
          RoundedRectangle(cornerRadius: 10)
            .stroke(.separator, lineWidth: policy.boundaryWidth)
        }
    }
  }
}

public struct SignalGlassControlGroup<Content: View>: View {
  @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
  private let spacing: CGFloat
  private let content: Content

  public init(spacing: CGFloat = 8, @ViewBuilder content: () -> Content) {
    self.spacing = spacing
    self.content = content()
  }

  public var body: some View {
    if reduceTransparency {
      content
    } else {
      GlassEffectContainer(spacing: spacing) {
        content
      }
    }
  }
}

public struct SignalReadingSurface<Content: View>: View {
  @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
  @Environment(\.colorSchemeContrast) private var colorSchemeContrast
  @Environment(\.colorScheme) private var colorScheme
  private let content: Content

  public init(@ViewBuilder content: () -> Content) {
    self.content = content()
  }

  public var body: some View {
    let policy = VisualPolicy(
      reduceTransparency: reduceTransparency,
      increaseContrast: colorSchemeContrast == .increased,
      appearance: colorScheme == .dark ? .dark : .light
    )

    content
      .background(Color(nsColor: .textBackgroundColor))
      .overlay(alignment: .leading) {
        Rectangle()
          .fill(.separator)
          .frame(width: policy.boundaryWidth)
          .accessibilityHidden(true)
      }
  }
}
