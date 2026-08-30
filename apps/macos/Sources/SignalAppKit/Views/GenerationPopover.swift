import SwiftUI

public struct GenerationPopoverPresentation: Sendable, Equatable {
  public let profiles: [ModelProfile]
  public let defaultProfileID: String?
  public let selectedProfileID: String?
  public let requiresExplicitSelection: Bool
  public let canGenerate: Bool

  public init(
    profiles: [ModelProfile],
    defaultProfileID: String?,
    selectedProfileID: String?
  ) {
    self.profiles = profiles.filter(\.enabled)
    self.defaultProfileID = defaultProfileID
    self.selectedProfileID = selectedProfileID
    let enabledIDs = Set(self.profiles.map(\.id))
    requiresExplicitSelection = defaultProfileID.map { !enabledIDs.contains($0) } ?? true
    if let selectedProfileID {
      canGenerate = enabledIDs.contains(selectedProfileID)
    } else {
      canGenerate = defaultProfileID.map(enabledIDs.contains) ?? false
    }
  }
}

public struct GenerationPopover: View {
  @Bindable private var model: AppModel
  private let story: Story
  @State private var isPresented = false
  @State private var selectedProfileID = ""
  @State private var force = false

  public init(model: AppModel, story: Story) {
    self.model = model
    self.story = story
  }

  public var body: some View {
    Button("Regenerate…", systemImage: "arrow.triangle.2.circlepath") {
      isPresented = true
    }
    .disabled(model.storyActionState(for: .regenerating(storyID: story.id)) != nil)
    .help("Regenerate this story with a configured AI model profile")
    .popover(isPresented: $isPresented, arrowEdge: .bottom) {
      popoverContent
        .padding(18)
        .frame(width: 320)
    }
  }

  private var popoverContent: some View {
    let selected = selectedProfileID.isEmpty ? nil : selectedProfileID
    let presentation = GenerationPopoverPresentation(
      profiles: model.snapshot?.modelProfiles ?? [],
      defaultProfileID: model.snapshot?.defaultModelProfileID,
      selectedProfileID: selected
    )

    return VStack(alignment: .leading, spacing: 14) {
      Text("Regenerate summary")
        .font(.headline)
        .accessibilityAddTraits(.isHeader)
        .accessibilitySortPriority(AccessibilityOrder.title.sortPriority)
      Text(
        "Creates an AI-generated variant through the selected model profile. Provider costs may apply."
      )
      .font(.callout)
      .foregroundStyle(.secondary)
      .fixedSize(horizontal: false, vertical: true)

      Picker("Model profile", selection: $selectedProfileID) {
        if let defaultProfileID = presentation.defaultProfileID,
          let profile = presentation.profiles.first(where: { $0.id == defaultProfileID })
        {
          Text("Default — \(profile.name)").tag("")
        } else {
          Text("Choose a profile").tag("")
        }
        ForEach(presentation.profiles) { profile in
          Text("\(profile.name) — \(profile.provider.readerDisplayName) · \(profile.model)")
            .tag(profile.id)
        }
      }

      Toggle("Force a new variant", isOn: $force)
      Text("Force bypasses a matching cached variant.")
        .font(.caption)
        .foregroundStyle(.secondary)

      HStack {
        Spacer()
        Button("Cancel") { isPresented = false }
        Button("Regenerate") {
          isPresented = false
          Task {
            await model.regenerateSelectedStory(profileID: selected, force: force)
          }
        }
        .buttonStyle(.borderedProminent)
        .disabled(!presentation.canGenerate)
      }
      .accessibilitySortPriority(AccessibilityOrder.actions.sortPriority)
    }
  }
}
