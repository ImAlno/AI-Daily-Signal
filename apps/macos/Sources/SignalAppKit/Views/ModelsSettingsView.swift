import SwiftUI

public struct ModelProfileRowPresentation: Sendable, Equatable {
  public let isDefault: Bool
  public let canSetDefault: Bool
  public let consentLabel: String
  public let credentialLabel: String
  public let canTest: Bool
  public let endpointHost: String?

  public init(profile: ModelProfile, isDefault: Bool) {
    self.isDefault = isDefault
    canSetDefault = profile.enabled && profile.consentedAt != nil && !isDefault
    canTest = profile.enabled && profile.consentedAt != nil
    consentLabel = profile.consentedAt == nil ? "Consent missing" : "Provider sharing approved"
    switch profile.credentialSource {
    case .systemStore: credentialLabel = "Keychain"
    case .environment: credentialLabel = "Environment variable"
    }
    endpointHost = profile.endpoint.flatMap { URLComponents(string: $0)?.host }
  }
}

public enum ModelsSettingsAction: Sendable, Equatable {
  case add
  case test
  case setDefault
  case remove
}

public enum ModelsSettingsRenderPlan {
  public static let creationAction = ModelsSettingsAction.add
  public static let profileActions: [ModelsSettingsAction] = [.test, .setDefault, .remove]
  public static let testsAutomatically = false
  public static let testRequiresConfirmation = true
  public static let removeRequiresConfirmation = true
}

public enum ModelPaidTestPresentation {
  public static let requiresConfirmation = true
  public static let disclosure = ModelSettingsCopy.paidTestDisclosure
}

public struct ModelRemovalPresentation: Sendable, Equatable {
  public let cleanupWarning: String?

  public init(credentialDeletion: CredentialDeletionStatus) {
    cleanupWarning =
      credentialDeletion == .deleteFailed ? ModelSettingsCopy.credentialCleanupWarning : nil
  }
}

public struct ModelsSettingsView: View {
  @Bindable private var model: AppModel
  @State private var pendingTest: ModelProfile?
  @State private var pendingRemoval: ModelProfile?

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    VStack(spacing: 0) {
      if model.inlineEditorRoute == .addModel {
        ModelProfileEditorView(model: model)
          .padding(.horizontal, 28)
          .padding(.vertical, 24)
          .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
      } else {
        modelList
      }
    }
    .toolbar {
      ToolbarItem(placement: .primaryAction) {
        Button("Add Model", systemImage: "plus") {
          model.presentModelEditor()
        }
        .disabled(
          model.inlineEditorRoute == .addModel
            || model.modelActionState(for: .adding) != nil
        )
        .help("Add model profile")
      }
    }
    .confirmationDialog(
      pendingTest.map { "Test \($0.name)?" } ?? "Test model profile?",
      isPresented: testConfirmation,
      titleVisibility: .visible
    ) {
      if let profile = pendingTest {
        Button("Run Test") {
          pendingTest = nil
          Task { await model.testModel(id: profile.id, confirmedCost: true) }
        }
      }
      Button("Cancel", role: .cancel) { pendingTest = nil }
    } message: {
      Text(ModelPaidTestPresentation.disclosure)
    }
    .confirmationDialog(
      pendingRemoval.map { "Remove \($0.name)?" } ?? "Remove model profile?",
      isPresented: removalConfirmation,
      titleVisibility: .visible
    ) {
      if let profile = pendingRemoval {
        Button("Remove Model", role: .destructive) {
          pendingRemoval = nil
          Task { await model.removeModel(id: profile.id, confirmed: true) }
        }
      }
      Button("Cancel", role: .cancel) { pendingRemoval = nil }
    } message: {
      Text(ModelSettingsCopy.removalHistoryDisclosure)
    }
    .alert(
      "Keychain credential remains",
      isPresented: cleanupWarningPresentation
    ) {
      Button("OK") { model.dismissCredentialCleanupWarning() }
    } message: {
      Text(model.credentialCleanupWarning ?? ModelSettingsCopy.credentialCleanupWarning)
    }
  }

  private var modelList: some View {
    List {
      Section("Models") {
        let profiles = model.snapshot?.modelProfiles ?? []
        if profiles.isEmpty {
          Label {
            VStack(alignment: .leading, spacing: 3) {
              Text("No model profiles")
              Text("Raw and Smart summaries remain available without AI.")
                .font(.caption)
                .foregroundStyle(.secondary)
            }
          } icon: {
            Image(systemName: "cpu")
              .foregroundStyle(.secondary)
          }
          .padding(.vertical, 6)
        } else {
          ForEach(profiles) { profile in
            profileRow(profile)
          }
        }
      }
    }
    .listStyle(.inset)
  }

  private var testConfirmation: Binding<Bool> {
    Binding(
      get: { pendingTest != nil },
      set: { if !$0 { pendingTest = nil } }
    )
  }

  private var removalConfirmation: Binding<Bool> {
    Binding(
      get: { pendingRemoval != nil },
      set: { if !$0 { pendingRemoval = nil } }
    )
  }

  private var cleanupWarningPresentation: Binding<Bool> {
    Binding(
      get: { model.credentialCleanupWarning != nil },
      set: { if !$0 { model.dismissCredentialCleanupWarning() } }
    )
  }

  private func profileRow(_ profile: ModelProfile) -> some View {
    let isDefault = model.snapshot?.defaultModelProfileID == profile.id
    let presentation = ModelProfileRowPresentation(profile: profile, isDefault: isDefault)
    let isBusy = profileIsBusy(profile.id)
    return ViewThatFits(in: .horizontal) {
      HStack(alignment: .center, spacing: 16) {
        profileInformation(profile, presentation: presentation)
        Spacer(minLength: 8)
        profileControls(profile, presentation: presentation, isBusy: isBusy)
      }
      VStack(alignment: .leading, spacing: 10) {
        profileInformation(profile, presentation: presentation)
        profileControls(profile, presentation: presentation, isBusy: isBusy)
          .frame(maxWidth: .infinity, alignment: .trailing)
      }
    }
    .padding(.vertical, 5)
  }

  private func profileInformation(
    _ profile: ModelProfile,
    presentation: ModelProfileRowPresentation
  ) -> some View {
    VStack(alignment: .leading, spacing: 4) {
      HStack(spacing: 7) {
        Text(profile.name).font(.headline)
        if presentation.isDefault {
          Text("Default")
            .font(.caption)
            .foregroundStyle(.secondary)
            .accessibilityLabel("Default model profile")
        }
      }
      Text("\(profile.provider.settingsTitle) · \(profile.model)")
        .font(.subheadline)
        .foregroundStyle(.secondary)
      Text(
        "\(presentation.credentialLabel) · \(presentation.consentLabel) · \(profile.limits.maxSummariesPerRefresh) summaries · \(profile.limits.maxOutputTokens) tokens"
      )
      .font(.caption)
      .foregroundStyle(.secondary)
      if let host = presentation.endpointHost {
        Text("Compatible endpoint: \(host)")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
      if let error = model.modelActionError(for: profile.id) {
        Label(error, systemImage: "exclamationmark.circle")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }

  private func profileControls(
    _ profile: ModelProfile,
    presentation: ModelProfileRowPresentation,
    isBusy: Bool
  ) -> some View {
    HStack(spacing: 10) {
      if isBusy {
        ProgressView()
          .controlSize(.small)
          .accessibilityLabel("Updating \(profile.name)")
      }
      Button("Test") { pendingTest = profile }
        .disabled(isBusy || !presentation.canTest)
      if presentation.canSetDefault {
        Button("Use as Default") {
          Task { await model.setDefaultModel(id: profile.id) }
        }
        .disabled(isBusy)
      }
      Button("Remove", role: .destructive) { pendingRemoval = profile }
        .disabled(isBusy)
    }
  }

  private func profileIsBusy(_ profileID: String) -> Bool {
    model.modelActionState(for: .settingDefault(profileID: profileID)) != nil
      || model.modelActionState(for: .testing(profileID: profileID)) != nil
      || model.modelActionState(for: .removing(profileID: profileID)) != nil
  }
}
