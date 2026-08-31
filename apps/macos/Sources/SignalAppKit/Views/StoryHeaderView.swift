import AppKit
import SwiftUI

public struct ExpandedStoryHeaderAccessibility: Sendable, Equatable {
  public let titleLabel: String
  public let statusLabel: String
  public let collapseLabel: String
  public let collapseValue: String
  public let collapseHint: String
  public let titleSortPriority: Double
  public let statusSortPriority: Double
}

public enum StoryHeaderAccessibilityPresentation: Sendable, Equatable {
  case collapsed(label: String, value: String, hint: String)
  case expanded(ExpandedStoryHeaderAccessibility)
}

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
    titleLineLimit = isExpanded || dynamicTypeSize.isAccessibilitySize ? nil : 3
  }

  public var title: String { row.title }
  public var chevronSystemImage: String { isExpanded ? "chevron.down" : "chevron.right" }
  public var accessibilityLabel: String { row.accessibilitySummary }
  public var accessibilityValue: String { isExpanded ? "Expanded" : "Collapsed" }
  public var emphasizesSignalLine: Bool { isExpanded || isHovered }
  public var signalRailOpacity: Double { emphasizesSignalLine ? 1 : 0.58 }
  public var showsSelectionSurface: Bool { isExpanded || isHovered }

  public var accessibilityPresentation: StoryHeaderAccessibilityPresentation {
    if isExpanded {
      var status = [
        row.primarySource,
        row.relativeTime,
        row.category,
        row.provenance.accessibilityLabel,
        row.isRead ? "Read" : "Unread",
        row.isSaved ? "Saved" : "Not saved",
      ]
      if row.isStale { status.append("Stale") }
      return .expanded(
        ExpandedStoryHeaderAccessibility(
          titleLabel: title,
          statusLabel: status.joined(separator: ", "),
          collapseLabel: "Collapse signal",
          collapseValue: "Expanded",
          collapseHint: "Collapse this signal",
          titleSortPriority: AccessibilityOrder.title.sortPriority,
          statusSortPriority: AccessibilityOrder.status.sortPriority
        )
      )
    }
    return .collapsed(
      label: accessibilityLabel,
      value: accessibilityValue,
      hint: "Expand this signal"
    )
  }
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

  @ViewBuilder
  public var body: some View {
    switch presentation.accessibilityPresentation {
    case .collapsed(let label, let value, let hint):
      Button(action: action) {
        headerLayout(accessibility: nil) {
          disclosureImage
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background { selectionSurface }
      .accessibilityElement(children: .ignore)
      .accessibilityLabel(label)
      .accessibilityValue(value)
      .accessibilityHint(hint)
    case .expanded(let accessibility):
      headerLayout(accessibility: accessibility) {
        Button(action: action) {
          disclosureImage
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accessibility.collapseLabel)
        .accessibilityValue(accessibility.collapseValue)
        .accessibilityHint(accessibility.collapseHint)
        .accessibilitySortPriority(accessibility.statusSortPriority)
      }
      .padding(.horizontal, 12)
      .padding(.vertical, 12)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background { selectionSurface }
      .accessibilityElement(children: .contain)
    }
  }

  private func headerLayout<Trailing: View>(
    accessibility: ExpandedStoryHeaderAccessibility?,
    @ViewBuilder trailing: () -> Trailing
  ) -> some View {
    HStack(alignment: .top, spacing: 12) {
      if let rank = presentation.row.rank {
        rankRail(rank)
      }

      VStack(alignment: .leading, spacing: 6) {
        headerMetadata(accessibility: accessibility)
        storyTitle(accessibility: accessibility)
        provenance
          .accessibilityHidden(accessibility != nil)
      }
      .frame(maxWidth: .infinity, alignment: .leading)

      trailing()
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }

  @ViewBuilder
  private var selectionSurface: some View {
    if presentation.showsSelectionSurface {
      RoundedRectangle(cornerRadius: 7, style: .continuous)
        .fill(Color(nsColor: .unemphasizedSelectedContentBackgroundColor))
    }
  }

  private var disclosureImage: some View {
    Image(systemName: presentation.chevronSystemImage)
      .font(.system(size: rankSize, weight: .semibold))
      .foregroundStyle(.secondary)
      .frame(
        width: VisualPolicy().minimumControlDimension,
        height: VisualPolicy().minimumControlDimension
      )
      .accessibilityHidden(true)
  }

  @ViewBuilder
  private func headerMetadata(
    accessibility: ExpandedStoryHeaderAccessibility?
  ) -> some View {
    if let accessibility {
      metadataContent
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibility.statusLabel)
        .accessibilitySortPriority(accessibility.statusSortPriority)
    } else {
      metadataContent
    }
  }

  private var metadataContent: some View {
    ViewThatFits(in: .horizontal) {
      HStack(spacing: 5) { metadata }
      VStack(alignment: .leading, spacing: 3) { metadata }
    }
    .font(.system(size: metadataSize))
    .foregroundStyle(.secondary)
  }

  @ViewBuilder
  private var provenance: some View {
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

  @ViewBuilder
  private func storyTitle(
    accessibility: ExpandedStoryHeaderAccessibility?
  ) -> some View {
    if let accessibility {
      titleText
        .textSelection(.enabled)
        .accessibilityLabel(accessibility.titleLabel)
        .accessibilityAddTraits(.isHeader)
        .accessibilitySortPriority(accessibility.titleSortPriority)
    } else {
      titleText
    }
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
    .opacity(presentation.signalRailOpacity)
    .frame(width: 28)
    .accessibilityHidden(true)
  }
}
