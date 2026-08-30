import Foundation
import Testing

@testable import SignalAppKit

@Suite(.serialized)
struct ModelSettingsTests {
  @Test
  func newProfileUsesTheExactBoundedDefaults() {
    // Break caught: silently expanding the number, size, duration, or retries of paid requests.
    let draft = ModelProfileEditorDraft()

    #expect(draft.maxSummariesText == "5")
    #expect(draft.maxOutputTokensText == "384")
    #expect(draft.timeoutSecondsText == "30")
    #expect(draft.maxRetriesText == "2")
  }

  @Test(arguments: ProviderKind.allCases)
  func compatibleConnectionFieldsAppearOnlyForTheCompatibleProvider(provider: ProviderKind) {
    // Break caught: accepting endpoint/dialect controls for official providers or hiding them for
    // the one provider family that requires them.
    let policy = ModelProviderFieldPolicy(provider: provider)

    #expect(
      policy.showsEndpointAndDialect == (provider == .openAICompatible)
    )
  }

  @Test
  func editorPreservesOpaqueModelAndExactMoneyStringsAcrossSubmission() {
    // Break caught: trimming an opaque model identifier or parsing USD through floating point.
    var draft = validModelDraft()
    draft.model = "  opaque/model:beta?literal#value  "
    draft.budget = BudgetFieldsDraft(
      dailyBudgetUSD: "1.234567",
      inputCostUSDPerMillion: "0.000001",
      outputCostUSDPerMillion: "999999.999999"
    )

    let input = draft.takeInput()

    #expect(input?.model == "  opaque/model:beta?literal#value  ")
    #expect(input?.limits.maxDailyCostUSD == "1.234567")
    #expect(input?.limits.inputCostUSDPerMillion == "0.000001")
    #expect(input?.limits.outputCostUSDPerMillion == "999999.999999")
  }

  @Test
  func budgetStringsAreEitherAllAbsentOrAllPresent() {
    // Break caught: presenting a cap that Rust cannot enforce because one exact rate is absent.
    var draft = validModelDraft()
    draft.budget = BudgetFieldsDraft(
      dailyBudgetUSD: "1",
      inputCostUSDPerMillion: "0.25",
      outputCostUSDPerMillion: ""
    )

    #expect(draft.validationMessage == BudgetFieldsPresentation.allTogetherMessage)
    #expect(draft.takeInput() == nil)

    draft.budget = BudgetFieldsDraft()
    #expect(draft.validationMessage == nil)
    #expect(draft.takeInput()?.limits.maxDailyCostUSD == nil)
  }

  @Test
  func saveRequiresExplicitStoryContentConsent() {
    // Break caught: creating an enabled provider profile before deliberate data-sharing consent.
    var draft = validModelDraft()
    draft.consentsToProviderDataSharing = false

    let presentation = ModelProfileEditorPresentation(
      draft: draft,
      isSaving: false,
      revealsValidation: true
    )

    #expect(!presentation.canSave)
    #expect(presentation.validationMessage == ModelSettingsCopy.consentRequired)
    #expect(ModelSettingsCopy.consentDisclosure.contains("approved story content"))
  }

  @Test
  func keychainSecretIsMovedOutOfTheViewDraftAndRedactedFromDebugCopy() {
    // Break caught: duplicating a submitted credential in editor state or diagnostic output.
    let sentinel = "swift-model-secret-SENTINEL"
    var draft = validModelDraft()
    draft.credentialMode = .keychain
    draft.environmentVariable = ""
    draft.keychainSecret = sentinel

    #expect(!String(reflecting: draft).contains(sentinel))
    let input = draft.takeInput()

    #expect(draft.keychainSecret.isEmpty)
    #expect(input?.credential == .systemStore(secret: sentinel))
    #expect(!String(reflecting: draft).contains(sentinel))
    #expect(!String(describing: draft).contains(sentinel))
    #expect(!String(reflecting: input).contains(sentinel))
    #expect(!String(describing: input).contains(sentinel))
    let credentialDebug = String(reflecting: input!.credential)
    let credentialDescription = String(describing: input!.credential)
    #expect(!credentialDebug.contains(sentinel))
    #expect(!credentialDescription.contains(sentinel))
  }

  @Test
  func rowPolicyAllowsDefaultOnlyForEnabledConsentedProfiles() {
    // Break caught: offering automatic paid generation for a disabled or unconsented profile.
    let usable = ModelProfileRowPresentation(profile: modelProfile(), isDefault: false)
    let disabled = ModelProfileRowPresentation(
      profile: modelProfile(enabled: false),
      isDefault: false
    )
    let unconsented = ModelProfileRowPresentation(
      profile: modelProfile(consented: false),
      isDefault: false
    )

    #expect(usable.canSetDefault)
    #expect(!disabled.canSetDefault)
    #expect(!unconsented.canSetDefault)
  }

  @Test(arguments: [
    CredentialDeletionStatus.deleted,
    CredentialDeletionStatus.notApplicable,
    CredentialDeletionStatus.deleteFailed,
  ])
  func removalShowsOnlyTheFiniteCredentialCleanupWarning(status: CredentialDeletionStatus) {
    // Break caught: interpolating Keychain diagnostics or warning when cleanup was successful.
    let presentation = ModelRemovalPresentation(credentialDeletion: status)

    if status == .deleteFailed {
      #expect(presentation.cleanupWarning == ModelSettingsCopy.credentialCleanupWarning)
    } else {
      #expect(presentation.cleanupWarning == nil)
    }
    #expect(
      ModelSettingsCopy.removalHistoryDisclosure.contains("Historical summaries are retained"))
  }

  @Test
  func paidTestPolicyRequiresTheExactDisclosureBeforeDispatch() {
    // Break caught: turning profile creation or selection into an automatic paid connectivity test.
    #expect(ModelPaidTestPresentation.requiresConfirmation)
    #expect(
      ModelPaidTestPresentation.disclosure
        == "This test sends synthetic text and may incur provider cost."
    )
  }

  @Test
  func editorRenderPolicyUsesSecureAndProviderAwareFields() {
    // Break caught: wiring a visible secret field or compatible endpoint controls into every form.
    var keychain = validModelDraft()
    keychain.credentialMode = .keychain
    keychain.keychainSecret = "not-rendered"
    keychain.environmentVariable = ""
    let official = ModelProfileEditorRenderPlan(draft: keychain)

    #expect(official.fields.contains(.secureCredential))
    #expect(!official.fields.contains(.environmentVariable))
    #expect(!official.fields.contains(.endpoint))
    #expect(!official.fields.contains(.dialect))
    #expect(official.fields.contains(.consentDisclosure))

    keychain.provider = .openAICompatible
    keychain.credentialMode = .environment
    keychain.environmentVariable = "COMPATIBLE_API_KEY"
    let compatible = ModelProfileEditorRenderPlan(draft: keychain)

    #expect(!compatible.fields.contains(.secureCredential))
    #expect(compatible.fields.contains(.environmentVariable))
    #expect(compatible.fields.contains(.endpoint))
    #expect(compatible.fields.contains(.dialect))
  }

  @Test
  func settingsRenderPolicyHasNoEditingOrAutomaticTestAction() {
    // Break caught: expanding alpha scope into in-place editing or a test-on-save side effect.
    #expect(
      ModelsSettingsRenderPlan.profileActions == [.test, .setDefault, .remove]
    )
    #expect(ModelsSettingsRenderPlan.creationAction == .add)
    #expect(!ModelsSettingsRenderPlan.testsAutomatically)
    #expect(ModelsSettingsRenderPlan.testRequiresConfirmation)
    #expect(ModelsSettingsRenderPlan.removeRequiresConfirmation)
  }

  @Test @MainActor
  func addMovesSecretOnceClearsEveryPathAndReconcilesTheWholeSnapshot() async {
    // Break caught: retaining a secret, issuing duplicate calls, or publishing a partial model state.
    let sentinel = "app-model-secret-SENTINEL"
    let initial = modelSnapshot(profiles: [], defaultID: nil, generation: 1)
    let added = modelProfile(id: "profile-added", name: "Added")
    let revision = StateRevision(dataGeneration: 2, sourceConfigRevision: "source-a")
    let authoritative = modelSnapshot(
      profiles: [added],
      defaultID: nil,
      generation: 2,
      latestTitle: "Authoritative after add"
    )
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.addModelResult = ModelMutationResult(profile: added, revision: revision)
    bridge.enqueueSnapshot(authoritative)
    bridge.suspendNextAddModel()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()
    var viewSecret = sentinel
    var draft = validModelDraft()
    draft.credentialMode = .keychain
    draft.environmentVariable = ""
    draft.keychainSecret = viewSecret
    let input = draft.takeInput()!

    let task = Task { await model.addModel(input) { viewSecret = "" } }
    await eventually { bridge.addModelRequests.count == 1 }

    #expect(model.modelActionState(for: .adding) == .inFlight)
    #expect(model.snapshot == initial)
    #expect(bridge.addModelRequests.count == 1)
    #expect(bridge.addModelRequests.first?.credential == .systemStore(secret: sentinel))
    #expect(draft.keychainSecret.isEmpty)

    bridge.releaseAddModels()
    #expect(await task.value)
    #expect(viewSecret.isEmpty)
    #expect(model.snapshot == authoritative)
    #expect(model.snapshot?.latest.first?.title == "Authoritative after add")
    #expect(!String(reflecting: model).contains(sentinel))

    let failureBridge = FakeBridgeClient(snapshot: initial)
    failureBridge.modelError = DetailedFakeError(description: "backend \(sentinel)")
    let failureModel = AppModel(
      bridge: failureBridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await failureModel.start()
    viewSecret = sentinel
    #expect(!(await failureModel.addModel(input) { viewSecret = "" }))
    #expect(viewSecret.isEmpty)
    #expect(!failureModel.modelEditorError.orEmpty.contains(sentinel))
  }

  @Test @MainActor
  func defaultTestAndRemovalRequirePolicyConfirmationAndPublishOnlyReconciledSnapshots() async {
    // Break caught: bypassing eligibility/confirmation or patching one profile over mismatched state.
    let usable = modelProfile()
    let disabled = modelProfile(id: "disabled", enabled: false)
    let initial = modelSnapshot(profiles: [usable, disabled], defaultID: nil, generation: 4)
    let bridge = FakeBridgeClient(snapshot: initial)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()

    #expect(!(await model.setDefaultModel(id: disabled.id)))
    #expect(bridge.defaultModelRequests.isEmpty)
    #expect(!(await model.testModel(id: usable.id, confirmedCost: false)))
    #expect(bridge.testModelRequests.isEmpty)
    #expect(!(await model.removeModel(id: usable.id, confirmed: false)))
    #expect(bridge.removeModelRequests.isEmpty)

    let defaultRevision = StateRevision(dataGeneration: 5, sourceConfigRevision: "source-a")
    let defaultSnapshot = modelSnapshot(
      profiles: [usable, disabled], defaultID: usable.id, generation: 5)
    bridge.defaultModelResult = ModelMutationResult(profile: usable, revision: defaultRevision)
    bridge.enqueueSnapshot(defaultSnapshot)
    #expect(await model.setDefaultModel(id: usable.id))
    #expect(model.snapshot == defaultSnapshot)

    let testRevision = StateRevision(dataGeneration: 6, sourceConfigRevision: "source-a")
    let testSnapshot = modelSnapshot(
      profiles: [usable, disabled], defaultID: usable.id, generation: 6)
    bridge.modelTestResult = ModelTestResult(
      profile: usable,
      costMayApply: true,
      revision: testRevision
    )
    bridge.enqueueSnapshot(testSnapshot)
    #expect(await model.testModel(id: usable.id, confirmedCost: true))
    #expect(bridge.testModelRequests == [usable.id])
    #expect(model.snapshot == testSnapshot)

    let removalRevision = StateRevision(dataGeneration: 7, sourceConfigRevision: "source-a")
    let removalSnapshot = modelSnapshot(profiles: [disabled], defaultID: nil, generation: 7)
    bridge.modelRemovalResult = ModelRemovalResult(
      profile: usable,
      credentialDeletion: .deleteFailed,
      revision: removalRevision
    )
    bridge.enqueueSnapshot(removalSnapshot)
    #expect(await model.removeModel(id: usable.id, confirmed: true))
    #expect(bridge.removeModelRequests == [usable.id])
    #expect(model.snapshot == removalSnapshot)
    #expect(model.credentialCleanupWarning == ModelSettingsCopy.credentialCleanupWarning)
  }

  @Test @MainActor
  func suspendedDefaultTestAndRemoveKeepConfirmedStateUntilEachFullReconciliation() async {
    // Break caught: optimistic profile/default removal or re-enabling controls before Rust returns.
    let profile = modelProfile()
    let initial = modelSnapshot(profiles: [profile], defaultID: nil, generation: 10)
    let bridge = FakeBridgeClient(snapshot: initial)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()

    let defaultSnapshot = modelSnapshot(profiles: [profile], defaultID: profile.id, generation: 11)
    bridge.defaultModelResult = ModelMutationResult(
      profile: profile,
      revision: defaultSnapshot.revision
    )
    bridge.enqueueSnapshot(defaultSnapshot)
    bridge.suspendNextDefaultModel()
    let defaultTask = Task { await model.setDefaultModel(id: profile.id) }
    await eventually { bridge.defaultModelRequests.count == 1 }
    #expect(model.modelActionState(for: .settingDefault(profileID: profile.id)) == .inFlight)
    #expect(model.snapshot == initial)
    bridge.releaseDefaultModels()
    #expect(await defaultTask.value)
    #expect(model.snapshot == defaultSnapshot)

    let testedSnapshot = modelSnapshot(profiles: [profile], defaultID: profile.id, generation: 12)
    bridge.modelTestResult = ModelTestResult(
      profile: profile,
      costMayApply: true,
      revision: testedSnapshot.revision
    )
    bridge.enqueueSnapshot(testedSnapshot)
    bridge.suspendNextModelTest()
    let testTask = Task { await model.testModel(id: profile.id, confirmedCost: true) }
    await eventually { bridge.testModelRequests.count == 1 }
    #expect(model.modelActionState(for: .testing(profileID: profile.id)) == .inFlight)
    #expect(model.snapshot == defaultSnapshot)
    bridge.releaseModelTests()
    #expect(await testTask.value)
    #expect(model.snapshot == testedSnapshot)

    let removedSnapshot = modelSnapshot(profiles: [], defaultID: nil, generation: 13)
    bridge.modelRemovalResult = ModelRemovalResult(
      profile: profile,
      credentialDeletion: .deleted,
      revision: removedSnapshot.revision
    )
    bridge.enqueueSnapshot(removedSnapshot)
    bridge.suspendNextModelRemoval()
    let removeTask = Task { await model.removeModel(id: profile.id, confirmed: true) }
    await eventually { bridge.removeModelRequests.count == 1 }
    #expect(model.modelActionState(for: .removing(profileID: profile.id)) == .inFlight)
    #expect(model.snapshot == testedSnapshot)
    bridge.releaseModelRemovals()
    #expect(await removeTask.value)
    #expect(model.snapshot == removedSnapshot)
  }

  @Test @MainActor
  func modelMutationWaitsForSuspendedPollingWithoutBridgeOverlap() async {
    // Break caught: a local profile mutation racing the revision read or its snapshot reconciliation.
    let profile = modelProfile()
    let initial = modelSnapshot(profiles: [profile], defaultID: nil, generation: 20)
    let defaultSnapshot = modelSnapshot(profiles: [profile], defaultID: profile.id, generation: 21)
    let bridge = FakeBridgeClient(snapshot: initial, revisions: [initial.revision])
    bridge.defaultModelResult = ModelMutationResult(
      profile: profile,
      revision: defaultSnapshot.revision
    )
    bridge.enqueueSnapshot(defaultSnapshot)
    bridge.suspendNextStateRevision()
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true),
      pollInterval: .milliseconds(1)
    )
    await model.start()
    model.setActive(true)
    await eventually { bridge.stateRevisionCalls == 1 }

    let mutation = Task { await model.setDefaultModel(id: profile.id) }
    await Task.yield()
    #expect(bridge.defaultModelRequests.isEmpty)
    #expect(model.modelActionState(for: .settingDefault(profileID: profile.id)) == .queued)
    #expect(bridge.maximumConcurrentBridgeCalls == 1)

    bridge.releaseStateRevisions()
    await eventually { bridge.defaultModelRequests.count == 1 }
    model.stopPolling()
    #expect(await mutation.value)
    #expect(bridge.maximumConcurrentBridgeCalls == 1)
    #expect(model.snapshot == defaultSnapshot)
  }

  @Test @MainActor
  func modelMutationRejectsAStaleReconciliationSnapshotWithSafeCopy() async {
    // Break caught: claiming success when the full snapshot predates the model mutation revision.
    let profile = modelProfile()
    let initial = modelSnapshot(profiles: [profile], defaultID: nil, generation: 30)
    let bridge = FakeBridgeClient(snapshot: initial)
    bridge.defaultModelResult = ModelMutationResult(
      profile: profile,
      revision: StateRevision(dataGeneration: 31, sourceConfigRevision: "source-a")
    )
    bridge.enqueueSnapshot(initial)
    let model = AppModel(
      bridge: bridge,
      preferences: MemoryAppPreferences(welcomeCompleted: true)
    )
    await model.start()

    #expect(!(await model.setDefaultModel(id: profile.id)))
    #expect(model.snapshot == initial)
    #expect(
      model.modelActionError(for: profile.id)
        == "The model state changed before it could be confirmed. Reload and try again."
    )
  }
}

private func validModelDraft() -> ModelProfileEditorDraft {
  ModelProfileEditorDraft(
    name: "Daily",
    provider: .openAI,
    model: "gpt-test",
    credentialMode: .environment,
    environmentVariable: "OPENAI_API_KEY",
    consentsToProviderDataSharing: true
  )
}

private func modelProfile(
  id: String = "profile-1",
  name: String = "Example",
  enabled: Bool = true,
  consented: Bool = true
) -> ModelProfile {
  let fixture = ModelProfile.fixture
  return ModelProfile(
    id: id,
    name: name,
    provider: fixture.provider,
    model: fixture.model,
    endpoint: fixture.endpoint,
    dialect: fixture.dialect,
    credentialSource: fixture.credentialSource,
    consentedAt: consented ? fixture.consentedAt : nil,
    enabled: enabled,
    limits: fixture.limits,
    createdAt: fixture.createdAt,
    updatedAt: fixture.updatedAt
  )
}

private func modelSnapshot(
  profiles: [ModelProfile],
  defaultID: String?,
  generation: UInt64,
  latestTitle: String = "A signal"
) -> AppSnapshot {
  let fixture = AppSnapshot.fixture
  let story = Story.fixture
  let latest = Story(
    id: story.id,
    title: latestTitle,
    canonicalURL: story.canonicalURL,
    excerpt: story.excerpt,
    category: story.category,
    publishedAt: story.publishedAt,
    sourceIDs: story.sourceIDs,
    score: story.score,
    smartSummary: story.smartSummary,
    isRead: story.isRead,
    isSaved: story.isSaved,
    selectedSummary: story.selectedSummary,
    summaryVariants: story.summaryVariants
  )
  return AppSnapshot(
    revision: StateRevision(
      dataGeneration: generation,
      sourceConfigRevision: fixture.revision.sourceConfigRevision
    ),
    status: fixture.status,
    today: fixture.today,
    latest: [latest],
    saved: fixture.saved,
    sources: fixture.sources,
    modelProfiles: profiles,
    defaultModelProfileID: defaultID,
    hasUsableAIProfile: defaultID != nil
  )
}

@MainActor
private func eventually(
  timeout: Duration = .seconds(1),
  condition: @MainActor () -> Bool
) async {
  let clock = ContinuousClock()
  let deadline = clock.now.advanced(by: timeout)
  while !condition(), clock.now < deadline {
    await Task.yield()
  }
  #expect(condition())
}

extension Optional where Wrapped == String {
  fileprivate var orEmpty: String { self ?? "" }
}
