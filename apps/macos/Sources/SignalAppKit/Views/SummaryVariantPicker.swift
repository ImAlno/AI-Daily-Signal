import SwiftUI

public struct SummaryVariantOption: Identifiable, Sendable, Equatable {
  public var id: ReadingSummarySelection { selection }
  public let selection: ReadingSummarySelection
  public let title: String
  public let detail: String
  public let provenance: SummaryProvenance

  public var displayLabel: String {
    if selection == .smart { return provenance.shortLabel }
    return "\(title) — \(detail)"
  }
}

public struct SummaryVariantPickerPresentation: Sendable, Equatable {
  public let options: [SummaryVariantOption]
  public let selection: ReadingSummarySelection

  public init(story: Story, selection: ReadingSummarySelection) {
    let immutableVariants = story.summaryVariants.enumerated().sorted { lhs, rhs in
      let left = lhs.element.generatedAt ?? .distantPast
      let right = rhs.element.generatedAt ?? .distantPast
      return left == right ? lhs.offset < rhs.offset : left > right
    }.map(\.element)

    var values = [
      SummaryVariantOption(
        selection: .raw,
        title: "Raw",
        detail: "Original source excerpt",
        provenance: .raw
      ),
      SummaryVariantOption(
        selection: .smart,
        title: "Smart",
        detail: "Local algorithmic summary",
        provenance: .smart
      ),
    ]
    values.append(
      contentsOf: immutableVariants.map { variant in
        let time =
          variant.generatedAt.map {
            SignalFormatters.relativeDate($0)
          } ?? "Unknown date"
        return SummaryVariantOption(
          selection: .ai(variantID: variant.id),
          title: "\(variant.provider.readerDisplayName) · \(variant.model)",
          detail: "AI-generated · \(time)",
          provenance: .ai(provider: variant.provider.readerDisplayName, model: variant.model)
        )
      })
    options = values
    self.selection = selection
  }
}

public struct SummaryVariantPicker: View {
  @Bindable private var model: AppModel
  private let story: Story

  public init(story: Story, model: AppModel) {
    self.story = story
    self.model = model
  }

  public var body: some View {
    let presentation = SummaryVariantPickerPresentation(
      story: story,
      selection: model.summarySelection(for: story.id)
    )
    Menu {
      ForEach(presentation.options) { option in
        Button {
          choose(option.selection)
        } label: {
          Label(
            option.displayLabel,
            systemImage: option.selection == presentation.selection ? "checkmark" : "circle"
          )
        }
        .disabled(
          isAI(option.selection)
            && model.storyActionState(for: .selectingSummary(storyID: story.id)) != nil
        )
        .accessibilityLabel("\(option.title), \(option.detail)")
      }
    } label: {
      Label("Summary", systemImage: "text.quote")
    }
    .accessibilityHint("Choose the original excerpt, Smart summary, or a cached AI summary")
  }

  private func choose(_ selection: ReadingSummarySelection) {
    switch selection {
    case .raw, .smart:
      model.showSummary(selection, for: story.id)
    case .ai:
      Task { await model.selectSummary(selection, for: story.id) }
    }
  }

  private func isAI(_ selection: ReadingSummarySelection) -> Bool {
    if case .ai = selection { return true }
    return false
  }
}
