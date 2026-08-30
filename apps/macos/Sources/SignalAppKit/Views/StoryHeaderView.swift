import AppKit
import SwiftUI

public struct StoryHeaderPresentation: Sendable, Equatable {
  public let row: StoryRowPresentation
  public let isExpanded: Bool
  public let isHovered: Bool
  public let titleLineLimit: Int?

  public init(
    row: StoryRowPresentation,
    isExpanded: Bool,
    isHovered: Bool,
    dynamicTypeSize: DynamicTypeSize
  ) {
    self.row = row
    self.isExpanded = isExpanded
    self.isHovered = isHovered
    titleLineLimit = dynamicTypeSize.isAccessibilitySize ? nil : 3
  }

  public var title: String { row.title }
  public var chevronSystemImage: String { isExpanded ? "chevron.down" : "chevron.right" }
  public var accessibilityLabel: String { row.accessibilitySummary }
  public var accessibilityValue: String { isExpanded ? "Expanded" : "Collapsed" }
  public var emphasizesSignalLine: Bool { isExpanded || isHovered }
  public var showsSelectionSurface: Bool { isExpanded || isHovered }
}

public struct StoryHeaderView: View {
  private let presentation: StoryHeaderPresentation
  private let action: () -> Void

  @ScaledMetric(relativeTo: .headline) private var collapsedTitleSize = 15.0
  @ScaledMetric(relativeTo: .title2) private var expandedTitleSize = 21.0
  @ScaledMetric(relativeTo: .caption) private var metadataSize = 12.0
  @ScaledMetric(relativeTo: .caption2) private var rankSize = 11.0

  public init(presentation: StoryHeaderPresentation, action: @escaping () -> Void) {
    self.presentation = presentation
    self.action = action
  }

  public var body: some View {
    Button(action: action) {
      HStack(alignment: .top, spacing: 12) {
        if let rank = presentation.row.rank {
          rankRail(rank)
        }

        VStack(alignment: .leading, spacing: 6) {
          ViewThatFits(in: .horizontal) {
            HStack(spacing: 5) { metadata }
            VStack(alignment: .leading, spacing: 3) { metadata }
          }
          .font(.system(size: metadataSize))
          .foregroundStyle(.secondary)

          storyTitle

          HStack(spacing: 8) {
            Text(presentation.row.provenance.shortLabel)
              .font(.system(size: metadataSize))
              .foregroundStyle(.secondary)
            if presentation.row.isSaved {
              Label("Saved", systemImage: "bookmark.fill")
                .labelStyle(.iconOnly)
                .foregroundStyle(.tint)
                .accessibilityLabel("Saved")
            }
          }
        }
        .frame(maxWidth: .infinity, alignment: .leading)

        Image(systemName: presentation.chevronSystemImage)
          .font(.system(size: rankSize, weight: .semibold))
          .foregroundStyle(.secondary)
          .frame(width: 16)
          .frame(minHeight: VisualPolicy().minimumControlDimension)
          .accessibilityHidden(true)
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(.horizontal, 12)
      .padding(.vertical, presentation.isExpanded ? 12 : 9)
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background {
      if presentation.showsSelectionSurface {
        RoundedRectangle(cornerRadius: 7, style: .continuous)
          .fill(Color(nsColor: .unemphasizedSelectedContentBackgroundColor))
      }
    }
    .accessibilityElement(children: .ignore)
    .accessibilityLabel(presentation.accessibilityLabel)
    .accessibilityValue(presentation.accessibilityValue)
    .accessibilityHint(presentation.isExpanded ? "Collapse this signal" : "Expand this signal")
  }

  @ViewBuilder
  private var metadata: some View {
    Text(presentation.row.primarySource)
      .foregroundStyle(.primary)
    Text("·")
    Text(presentation.row.relativeTime)
    Text("·")
    Text(presentation.row.category)
    if presentation.row.isStale {
      Label("Stale", systemImage: "clock.badge.exclamationmark")
        .foregroundStyle(.orange)
    }
  }

  @ViewBuilder
  private var storyTitle: some View {
    if presentation.isExpanded {
      titleText.textSelection(.enabled)
    } else {
      titleText
    }
  }

  private var titleText: some View {
    Text(presentation.title)
      .font(
        .system(
          size: presentation.isExpanded ? expandedTitleSize : collapsedTitleSize,
          weight: presentation.isExpanded || !presentation.row.isRead ? .semibold : .regular
        )
      )
      .foregroundStyle(.primary)
      .multilineTextAlignment(.leading)
      .lineLimit(presentation.titleLineLimit)
  }

  private func rankRail(_ rank: UInt32) -> some View {
    VStack(spacing: 5) {
      Text(String(format: "%02d", rank))
        .font(.system(size: rankSize, weight: .medium, design: .monospaced))
        .foregroundStyle(.tint)
      Rectangle()
        .fill(
          presentation.emphasizesSignalLine
            ? Color.accentColor
            : Color(nsColor: .separatorColor)
        )
        .frame(width: 1, height: presentation.isExpanded ? 52 : 36)
    }
    .frame(width: 28)
    .accessibilityHidden(true)
  }
}
