import Foundation
import SwiftUI

public enum SourceEditorCopy {
  public static let title = "Add Personal Source"
  public static let guidance = "Add an RSS or Atom feed to include it in future briefings."
}

public struct SourceEditorDraft: Sendable, Equatable, CustomDebugStringConvertible,
  CustomReflectable
{
  public var name: String
  public var feedURL: String
  public var category: String
  public var weight: Double
  public var enabled: Bool

  public init(
    name: String = "",
    feedURL: String = "",
    category: String = "",
    weight: Double = 0.8,
    enabled: Bool = true
  ) {
    self.name = name
    self.feedURL = feedURL
    self.category = category
    self.weight = weight
    self.enabled = enabled
  }

  public var debugDescription: String {
    "SourceEditorDraft(weight: \(weight), enabled: \(enabled), feedURL: <redacted>)"
  }

  public var customMirror: Mirror {
    Mirror(self, children: ["source editor draft": "<redacted>"])
  }

  public var validationMessage: String? {
    if name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      || category.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      || feedURL.isEmpty
    {
      return "Complete every field."
    }
    guard weight.isFinite, (0.0...1.0).contains(weight) else {
      return "Enter a weight from 0 to 1."
    }
    return nil
  }

  public var input: FeedSourceInput? {
    guard validationMessage == nil else { return nil }
    return FeedSourceInput(
      name: name.trimmingCharacters(in: .whitespacesAndNewlines),
      category: category.trimmingCharacters(in: .whitespacesAndNewlines),
      url: feedURL,
      weight: weight,
      enabled: enabled
    )
  }
}

public struct SourceEditorPresentation: Sendable, Equatable {
  public let validationMessage: String?
  public let canSave: Bool

  public init(
    draft: SourceEditorDraft,
    isSaving: Bool,
    revealsValidation: Bool = false
  ) {
    validationMessage = revealsValidation ? draft.validationMessage : nil
    canSave = !isSaving && draft.input != nil
  }
}

public enum SourceWeightParser {
  public static func parse(_ text: String, locale: Locale) -> Double? {
    guard !text.isEmpty else { return nil }
    let formatter = NumberFormatter()
    formatter.locale = locale
    formatter.numberStyle = .decimal
    formatter.isLenient = false
    var parsed: AnyObject?
    var range = NSRange(location: 0, length: text.utf16.count)
    do {
      try formatter.getObjectValue(&parsed, for: text, range: &range)
    } catch {
      return nil
    }
    guard range.location == 0,
      range.length == text.utf16.count,
      let number = parsed as? NSNumber
    else { return nil }
    return number.doubleValue
  }
}

public struct SourceEditorView: View, CustomDebugStringConvertible, CustomReflectable {
  private enum Field: Hashable {
    case name
    case feedURL
    case category
    case weight
  }

  @Bindable private var model: AppModel
  @State private var draft = SourceEditorDraft()
  @State private var weightText = "0.8"
  @State private var revealsValidation = false
  @FocusState private var focusedField: Field?
  @Environment(\.locale) private var locale

  public init(model: AppModel) {
    self.model = model
  }

  nonisolated public var debugDescription: String { "SourceEditorView(state: <redacted>)" }

  nonisolated public var customMirror: Mirror {
    Mirror(reflecting: "<redacted source editor view>")
  }

  public var body: some View {
    let isSaving = model.sourceActionState(for: .adding) != nil
    let presentation = SourceEditorPresentation(
      draft: validatedDraft,
      isSaving: isSaving,
      revealsValidation: revealsValidation
    )

    VStack(alignment: .leading, spacing: 16) {
      SettingsPageHeaderView(
        title: SourceEditorCopy.title,
        message: SourceEditorCopy.guidance
      )
      HStack {
        Spacer()
        Button("Cancel") {
          model.dismissSourceEditor()
        }
        .keyboardShortcut(.cancelAction)
        Button {
          save()
        } label: {
          if isSaving {
            ProgressView()
              .controlSize(.small)
              .accessibilityLabel("Adding source")
          } else {
            Text("Add Source")
          }
        }
        .keyboardShortcut(.defaultAction)
        .disabled(!presentation.canSave)
      }

      sourceForm(presentation: presentation)
        .formStyle(.columns)
        .disabled(isSaving)
    }
    .frame(maxWidth: SettingsGridMetrics.maximumWidth, alignment: .leading)
    .task {
      focusedField = .name
    }
    .onChange(of: draft.name) { _, _ in revealsValidation = true }
    .onChange(of: draft.feedURL) { _, _ in revealsValidation = true }
    .onChange(of: draft.category) { _, _ in revealsValidation = true }
    .onChange(of: weightText) { _, _ in revealsValidation = true }
  }

  private func sourceForm(presentation: SourceEditorPresentation) -> some View {
    Form {
      Section("Feed") {
        TextField("Name", text: $draft.name, prompt: Text("Publication or project"))
          .focused($focusedField, equals: .name)
          .textContentType(.name)
        TextField("Feed URL", text: $draft.feedURL, prompt: Text("https://example.com/feed.xml"))
          .focused($focusedField, equals: .feedURL)
          .textContentType(.URL)
        TextField("Category", text: $draft.category, prompt: Text("Research"))
          .focused($focusedField, equals: .category)
      }

      Section {
        TextField("Weight", text: $weightText, prompt: Text("0.8"))
          .focused($focusedField, equals: .weight)
          .accessibilityHint("Enter a number from 0 to 1")
        Toggle("Enabled", isOn: $draft.enabled)
      } header: {
        Text("Briefing")
      } footer: {
        Text("Weight controls how strongly this feed contributes to story ranking.")
      }

      if let message = presentation.validationMessage ?? model.sourceEditorError {
        Section {
          Label(message, systemImage: "exclamationmark.circle")
            .foregroundStyle(.secondary)
            .accessibilityLabel("Source form error: \(message)")
        }
      }
    }
  }

  private func save() {
    revealsValidation = true
    guard let input = validatedDraft.input else { return }
    Task {
      if await model.addSource(input) {
        model.dismissSourceEditor()
      }
    }
  }

  private var validatedDraft: SourceEditorDraft {
    SourceEditorDraft(
      name: draft.name,
      feedURL: draft.feedURL,
      category: draft.category,
      weight: SourceWeightParser.parse(weightText, locale: locale) ?? .nan,
      enabled: draft.enabled
    )
  }
}
