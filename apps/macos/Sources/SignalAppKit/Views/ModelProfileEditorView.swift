import SwiftUI

public enum ModelCredentialMode: String, CaseIterable, Sendable, Equatable {
  case keychain
  case environment

  public var title: String {
    switch self {
    case .keychain: "Keychain"
    case .environment: "Environment variable"
    }
  }
}

public struct ModelProviderFieldPolicy: Sendable, Equatable {
  public let showsEndpointAndDialect: Bool

  public init(provider: ProviderKind) {
    showsEndpointAndDialect = provider == .openAICompatible
  }
}

public enum ModelSettingsCopy {
  public static let consentDisclosure =
    "I understand that approved story content is sent to the selected provider for AI summaries."
  public static let consentRequired = "Confirm provider data sharing before adding this model."
  public static let paidTestDisclosure =
    "This test sends synthetic text and may incur provider cost."
  public static let removalHistoryDisclosure =
    "Historical summaries are retained after this profile is removed."
  public static let credentialCleanupWarning =
    "The model profile was removed, but its Keychain credential could not be deleted. Remove it from Keychain Access."
}

public struct ModelProfileEditorDraft: Sendable, CustomDebugStringConvertible {
  public var name: String
  public var provider: ProviderKind
  public var model: String
  public var endpoint: String
  public var dialect: APIDialect
  public var credentialMode: ModelCredentialMode
  public var keychainSecret: String
  public var environmentVariable: String
  public var maxSummariesText: String
  public var maxOutputTokensText: String
  public var timeoutSecondsText: String
  public var maxRetriesText: String
  public var budget: BudgetFieldsDraft
  public var consentsToProviderDataSharing: Bool

  public init(
    name: String = "",
    provider: ProviderKind = .openAI,
    model: String = "",
    endpoint: String = "",
    dialect: APIDialect = .responses,
    credentialMode: ModelCredentialMode = .keychain,
    keychainSecret: String = "",
    environmentVariable: String = "",
    maxSummariesText: String = "5",
    maxOutputTokensText: String = "384",
    timeoutSecondsText: String = "30",
    maxRetriesText: String = "2",
    budget: BudgetFieldsDraft = BudgetFieldsDraft(),
    consentsToProviderDataSharing: Bool = false
  ) {
    self.name = name
    self.provider = provider
    self.model = model
    self.endpoint = endpoint
    self.dialect = dialect
    self.credentialMode = credentialMode
    self.keychainSecret = keychainSecret
    self.environmentVariable = environmentVariable
    self.maxSummariesText = maxSummariesText
    self.maxOutputTokensText = maxOutputTokensText
    self.timeoutSecondsText = timeoutSecondsText
    self.maxRetriesText = maxRetriesText
    self.budget = budget
    self.consentsToProviderDataSharing = consentsToProviderDataSharing
  }

  public var validationMessage: String? {
    if name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      || model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    {
      return "Enter a profile name and model identifier."
    }
    if ModelProviderFieldPolicy(provider: provider).showsEndpointAndDialect && endpoint.isEmpty {
      return "Enter an endpoint for the OpenAI-compatible provider."
    }
    switch credentialMode {
    case .keychain where keychainSecret.isEmpty:
      return "Enter a credential to store in Keychain."
    case .environment where environmentVariable.isEmpty:
      return "Enter an environment variable name."
    default:
      break
    }
    guard let summaries = UInt32(maxSummariesText), summaries > 0,
      let outputTokens = UInt32(maxOutputTokensText), outputTokens > 0,
      let timeout = UInt64(timeoutSecondsText), timeout > 0,
      UInt32(maxRetriesText) != nil
    else {
      return "Enter valid whole-number request limits."
    }
    guard budget.isEmpty || budget.isComplete else {
      return BudgetFieldsPresentation.allTogetherMessage
    }
    guard consentsToProviderDataSharing else {
      return ModelSettingsCopy.consentRequired
    }
    return nil
  }

  public mutating func takeInput() -> ModelProfileInput? {
    guard validationMessage == nil,
      let summaries = UInt32(maxSummariesText),
      let outputTokens = UInt32(maxOutputTokensText),
      let timeout = UInt64(timeoutSecondsText),
      let retries = UInt32(maxRetriesText)
    else { return nil }

    let credential: ModelCredentialInput
    switch credentialMode {
    case .keychain:
      credential = .systemStore(secret: keychainSecret)
      keychainSecret.removeAll(keepingCapacity: false)
    case .environment:
      credential = .environment(variable: environmentVariable)
    }
    let compatible = ModelProviderFieldPolicy(provider: provider).showsEndpointAndDialect
    return ModelProfileInput(
      name: name.trimmingCharacters(in: .whitespacesAndNewlines),
      provider: provider,
      model: model,
      endpoint: compatible ? endpoint : nil,
      dialect: compatible ? dialect : nil,
      credential: credential,
      consentProviderDataSharing: consentsToProviderDataSharing,
      limits: ProfileLimitsInput(
        maxSummariesPerRefresh: summaries,
        maxDailyCostUSD: budget.isEmpty ? nil : budget.dailyBudgetUSD,
        inputCostUSDPerMillion: budget.isEmpty ? nil : budget.inputCostUSDPerMillion,
        outputCostUSDPerMillion: budget.isEmpty ? nil : budget.outputCostUSDPerMillion,
        maxOutputTokens: outputTokens,
        timeoutSeconds: timeout,
        maxRetries: retries
      )
    )
  }

  public mutating func clearSecret() {
    keychainSecret.removeAll(keepingCapacity: false)
  }

  public var debugDescription: String {
    "ModelProfileEditorDraft(name: \(name.debugDescription), provider: \(provider.rawValue), credential: <redacted>)"
  }
}

public struct ModelProfileEditorPresentation: Sendable, Equatable {
  public let validationMessage: String?
  public let canSave: Bool

  public init(
    draft: ModelProfileEditorDraft,
    isSaving: Bool,
    revealsValidation: Bool = false
  ) {
    validationMessage = revealsValidation ? draft.validationMessage : nil
    canSave = !isSaving && draft.validationMessage == nil
  }
}

public enum ModelProfileEditorField: Sendable, Hashable {
  case name
  case provider
  case model
  case endpoint
  case dialect
  case secureCredential
  case environmentVariable
  case requestLimits
  case exactBudgetStrings
  case consentDisclosure
}

public struct ModelProfileEditorRenderPlan: Sendable, Equatable {
  public let fields: Set<ModelProfileEditorField>

  public init(draft: ModelProfileEditorDraft) {
    var fields: Set<ModelProfileEditorField> = [
      .name, .provider, .model, .requestLimits, .exactBudgetStrings, .consentDisclosure,
    ]
    if ModelProviderFieldPolicy(provider: draft.provider).showsEndpointAndDialect {
      fields.formUnion([.endpoint, .dialect])
    }
    switch draft.credentialMode {
    case .keychain: fields.insert(.secureCredential)
    case .environment: fields.insert(.environmentVariable)
    }
    self.fields = fields
  }
}

public struct ModelProfileEditorView: View {
  @Bindable private var model: AppModel
  @State private var draft = ModelProfileEditorDraft()
  @State private var revealsValidation = false

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    let isSaving = model.modelActionState(for: .adding) != nil
    let presentation = ModelProfileEditorPresentation(
      draft: draft,
      isSaving: isSaving,
      revealsValidation: revealsValidation
    )
    let renderPlan = ModelProfileEditorRenderPlan(draft: draft)

    Form {
      Section("Profile") {
        TextField("Name", text: $draft.name, prompt: Text("Daily summaries"))
          .textContentType(.name)
        Picker("Provider", selection: $draft.provider) {
          ForEach(ProviderKind.allCases, id: \.rawValue) { provider in
            Text(provider.settingsTitle).tag(provider)
          }
        }
        TextField("Model identifier", text: $draft.model, prompt: Text("Provider model ID"))
          .accessibilityHint("Preserved exactly and sent to the selected provider")
      }

      if renderPlan.fields.contains(.endpoint) {
        Section {
          TextField(
            "Base endpoint",
            text: $draft.endpoint,
            prompt: Text("https://provider.example/v1")
          )
          .textContentType(.URL)
          Picker("API dialect", selection: $draft.dialect) {
            ForEach(APIDialect.allCases, id: \.rawValue) { dialect in
              Text(dialect.settingsTitle).tag(dialect)
            }
          }
        } header: {
          Text("Compatible connection")
        } footer: {
          Text("Custom endpoint and dialect apply only to OpenAI-compatible providers.")
        }
      }

      Section {
        Picker("Credential source", selection: $draft.credentialMode) {
          ForEach(ModelCredentialMode.allCases, id: \.rawValue) { mode in
            Text(mode.title).tag(mode)
          }
        }
        if renderPlan.fields.contains(.secureCredential) {
          SecureField("API credential", text: $draft.keychainSecret)
            .textContentType(.password)
        } else {
          TextField(
            "Variable name",
            text: $draft.environmentVariable,
            prompt: Text("PROVIDER_API_KEY")
          )
          .accessibilityHint("Enter the variable name, not its credential value")
        }
      } header: {
        Text("Credential")
      } footer: {
        Text(
          draft.credentialMode == .keychain
            ? "The value is stored in Keychain and cleared from this form when saving finishes."
            : "Only the variable name is stored. Its value is resolved when a provider call is made."
        )
      }

      Section("Request limits") {
        TextField("Summaries per refresh", text: $draft.maxSummariesText)
        TextField("Maximum output tokens", text: $draft.maxOutputTokensText)
        TextField("Timeout (seconds)", text: $draft.timeoutSecondsText)
        TextField("Retries", text: $draft.maxRetriesText)
      }

      BudgetFieldsView(draft: $draft.budget)

      Section {
        Toggle(ModelSettingsCopy.consentDisclosure, isOn: $draft.consentsToProviderDataSharing)
      } header: {
        Text("Provider data sharing")
      } footer: {
        Text("Consent is stored with this profile and enables future AI summary requests.")
      }

      if let message = presentation.validationMessage ?? model.modelEditorError {
        Section {
          Label(message, systemImage: "exclamationmark.circle")
            .foregroundStyle(.secondary)
            .accessibilityLabel("Model form error: \(message)")
        }
      }
    }
    .formStyle(.grouped)
    .disabled(isSaving)
    .frame(minWidth: 500, idealWidth: 560, minHeight: 620)
    .navigationTitle("Add Model Profile")
    .safeAreaInset(edge: .bottom) {
      HStack {
        Spacer()
        Button("Cancel") {
          draft.clearSecret()
          model.dismissModelEditor()
        }
        .keyboardShortcut(.cancelAction)
        .disabled(false)
        Button {
          save()
        } label: {
          if isSaving {
            ProgressView()
              .controlSize(.small)
              .accessibilityLabel("Adding model profile")
          } else {
            Text("Add Model")
          }
        }
        .keyboardShortcut(.defaultAction)
        .disabled(!presentation.canSave)
      }
      .padding()
      .background(.bar)
    }
    .onChange(of: draft.validationMessage) { _, _ in
      revealsValidation = true
    }
    .onChange(of: draft.credentialMode) { _, mode in
      if mode == .environment { draft.clearSecret() }
    }
    .onDisappear {
      draft.clearSecret()
    }
  }

  private func save() {
    revealsValidation = true
    guard let input = draft.takeInput() else { return }
    Task {
      let succeeded = await model.addModel(input) {
        draft.clearSecret()
      }
      if succeeded { model.dismissModelEditor() }
    }
  }
}

extension ProviderKind {
  var settingsTitle: String {
    switch self {
    case .openAI: "OpenAI"
    case .anthropic: "Anthropic"
    case .gemini: "Google Gemini"
    case .openAICompatible: "OpenAI-compatible"
    }
  }
}

extension APIDialect {
  fileprivate var settingsTitle: String {
    switch self {
    case .responses: "Responses"
    case .chatCompletions: "Chat Completions"
    }
  }
}
