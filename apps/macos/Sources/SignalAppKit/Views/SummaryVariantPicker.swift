import SwiftUI

public struct SummaryVariantOption: Identifiable, Sendable, Equatable {
  public var id: ReadingSummarySelection { selection }
  public let selection: ReadingSummarySelection
  public let title: String
  public let detail: String
  public let provenance: SummaryProvenance
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
        detail: "Local signal summary",
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
    Picker("Summary", selection: selectionBinding) {
      ForEach(presentation.options) { option in
        VStack(alignment: .leading) {
          Text(option.title)
          Text(option.detail)
        }
        .tag(option.selection)
        .accessibilityLabel("\(option.title), \(option.detail)")
      }
    }
    .pickerStyle(.menu)
    .disabled(model.activeStoryAction == .selectingSummary(storyID: story.id))
    .accessibilityHint("Choose the original excerpt, Smart summary, or a cached AI summary")
  }

  private var selectionBinding: Binding<ReadingSummarySelection> {
    Binding(
      get: { model.summarySelection(for: story.id) },
      set: { selection in
        switch selection {
        case .raw, .smart:
          model.showSummary(selection, for: story.id)
        case .ai:
          Task { await model.selectSummary(selection, for: story.id) }
        }
      }
    )
  }
}
