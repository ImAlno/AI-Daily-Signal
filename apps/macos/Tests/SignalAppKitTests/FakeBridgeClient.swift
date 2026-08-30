import Foundation

@testable import SignalAppKit

final class FakeBridgeClient: BridgeClient, @unchecked Sendable {
  private typealias SnapshotContinuation = CheckedContinuation<AppSnapshot, any Error>
  private typealias RevisionContinuation = CheckedContinuation<StateRevision, any Error>
  private typealias RefreshContinuation = CheckedContinuation<RefreshResult, any Error>
  private typealias StoryMutationContinuation = CheckedContinuation<StoryMutationResult, any Error>
  private typealias GenerationContinuation = CheckedContinuation<GenerationResult, any Error>
  private typealias SourceMutationContinuation = CheckedContinuation<
    SourceMutationResult, any Error
  >
  private typealias ModelMutationContinuation = CheckedContinuation<ModelMutationResult, any Error>
  private typealias ModelTestContinuation = CheckedContinuation<ModelTestResult, any Error>
  private typealias ModelRemovalContinuation = CheckedContinuation<ModelRemovalResult, any Error>

  private let lock = NSLock()
  private var snapshots: [AppSnapshot]
  private var revisions: [StateRevision]
  private var snapshotIndex = 0
  private var revisionIndex = 0
  private var suspendedSnapshotCount = 0
  private var suspendedRevisionCount = 0
  private var suspendedRefreshStartCount = 0
  private var pendingSnapshots: [(SnapshotContinuation, Result<AppSnapshot, any Error>)] = []
  private var pendingRevisions: [(RevisionContinuation, Result<StateRevision, any Error>)] = []
  private var pendingRefreshStarts: [CheckedContinuation<Void, Never>] = []
  private var pendingRefreshes: [String: RefreshContinuation] = [:]
  private var activeRefreshReservationID: String?
  private var refreshReservationIsRunning = false
  private var pendingSavedMutations:
    [(StoryMutationContinuation, Result<StoryMutationResult, any Error>)] = []
  private var pendingReadMutations:
    [(StoryMutationContinuation, Result<StoryMutationResult, any Error>)] = []
  private var pendingSummaryMutations:
    [(StoryMutationContinuation, Result<StoryMutationResult, any Error>)] = []
  private var pendingGenerations: [(GenerationContinuation, Result<GenerationResult, any Error>)] =
    []
  private var pendingAddSources:
    [(SourceMutationContinuation, Result<SourceMutationResult, any Error>)] = []
  private var pendingSourceToggles:
    [(SourceMutationContinuation, Result<SourceMutationResult, any Error>)] = []
  private var pendingSourceRemovals:
    [(SourceMutationContinuation, Result<SourceMutationResult, any Error>)] = []
  private var pendingAddModels:
    [(ModelMutationContinuation, Result<ModelMutationResult, any Error>)] = []
  private var pendingDefaultModels:
    [(ModelMutationContinuation, Result<ModelMutationResult, any Error>)] = []
  private var pendingModelTests: [(ModelTestContinuation, Result<ModelTestResult, any Error>)] = []
  private var pendingModelRemovals:
    [(ModelRemovalContinuation, Result<ModelRemovalResult, any Error>)] = []
  private var cancellationRequests: Set<String> = []
  private var storedRefreshIdentifiers: [String] = []
  private var storedRefreshWorkIdentifiers: [String] = []
  private var storedReservedRefreshIdentifiers: [String] = []
  private var storedReleasedRefreshReservationIdentifiers: [String] = []
  private var storedCancelledIdentifiers: [String] = []
  private var storedSnapshotCalls = 0
  private var storedStateRevisionCalls = 0
  private var storedRefreshEntryCalls = 0
  private var storedModelInputs: [ModelProfileInput] = []
  private var storedDefaultModelRequests: [String] = []
  private var storedTestModelRequests: [String] = []
  private var storedRemoveModelRequests: [String] = []
  private var storedSavedRequests: [(storyID: String, saved: Bool)] = []
  private var storedReadRequests: [(storyID: String, read: Bool)] = []
  private var storedSummaryRequests: [SummaryRequest] = []
  private var storedGenerationRequests: [GenerationRequest] = []
  private var storedSavedResult: StoryMutationResult?
  private var storedReadResult: StoryMutationResult?
  private var storedSummaryResult: StoryMutationResult?
  private var storedGenerationResult: GenerationResult?
  private var storedSavedError: (any Error)?
  private var storedReadError: (any Error)?
  private var storedSummaryError: (any Error)?
  private var storedGenerationError: (any Error)?
  private var storedSourceError: (any Error)?
  private var storedAddSourceResult: SourceMutationResult?
  private var storedSourceToggleResult: SourceMutationResult?
  private var storedRemoveSourceResult: SourceMutationResult?
  private var storedAddModelResult: ModelMutationResult?
  private var storedDefaultModelResult: ModelMutationResult?
  private var storedModelTestResult: ModelTestResult?
  private var storedModelRemovalResult: ModelRemovalResult?
  private var storedAddSourceInputs: [FeedSourceInput] = []
  private var storedSourceToggleRequests: [(id: String, enabled: Bool)] = []
  private var storedSourceRemovalRequests: [String] = []
  private var suspendedSavedMutationCount = 0
  private var suspendedReadMutationCount = 0
  private var suspendedSummaryMutationCount = 0
  private var suspendedGenerationCount = 0
  private var suspendedAddSourceCount = 0
  private var suspendedSourceToggleCount = 0
  private var suspendedSourceRemovalCount = 0
  private var suspendedAddModelCount = 0
  private var suspendedDefaultModelCount = 0
  private var suspendedModelTestCount = 0
  private var suspendedModelRemovalCount = 0
  private var storedSnapshotError: (any Error)?
  private var storedRefreshError: (any Error)?
  private var storedModelError: (any Error)?
  private var activeBridgeCallCount = 0
  private var storedMaximumConcurrentBridgeCalls = 0

  var snapshotError: (any Error)? {
    get { lock.withLock { storedSnapshotError } }
    set { lock.withLock { storedSnapshotError = newValue } }
  }

  var refreshError: (any Error)? {
    get { lock.withLock { storedRefreshError } }
    set { lock.withLock { storedRefreshError = newValue } }
  }

  var modelError: (any Error)? {
    get { lock.withLock { storedModelError } }
    set { lock.withLock { storedModelError = newValue } }
  }

  var savedResult: StoryMutationResult? {
    get { lock.withLock { storedSavedResult } }
    set { lock.withLock { storedSavedResult = newValue } }
  }

  var readResult: StoryMutationResult? {
    get { lock.withLock { storedReadResult } }
    set { lock.withLock { storedReadResult = newValue } }
  }

  var summaryResult: StoryMutationResult? {
    get { lock.withLock { storedSummaryResult } }
    set { lock.withLock { storedSummaryResult = newValue } }
  }

  var generationResult: GenerationResult? {
    get { lock.withLock { storedGenerationResult } }
    set { lock.withLock { storedGenerationResult = newValue } }
  }

  var generationError: (any Error)? {
    get { lock.withLock { storedGenerationError } }
    set { lock.withLock { storedGenerationError = newValue } }
  }

  var savedError: (any Error)? {
    get { lock.withLock { storedSavedError } }
    set { lock.withLock { storedSavedError = newValue } }
  }

  var readError: (any Error)? {
    get { lock.withLock { storedReadError } }
    set { lock.withLock { storedReadError = newValue } }
  }

  var summaryError: (any Error)? {
    get { lock.withLock { storedSummaryError } }
    set { lock.withLock { storedSummaryError = newValue } }
  }

  var sourceError: (any Error)? {
    get { lock.withLock { storedSourceError } }
    set { lock.withLock { storedSourceError = newValue } }
  }

  var addSourceResult: SourceMutationResult? {
    get { lock.withLock { storedAddSourceResult } }
    set { lock.withLock { storedAddSourceResult = newValue } }
  }

  var sourceToggleResult: SourceMutationResult? {
    get { lock.withLock { storedSourceToggleResult } }
    set { lock.withLock { storedSourceToggleResult = newValue } }
  }

  var removeSourceResult: SourceMutationResult? {
    get { lock.withLock { storedRemoveSourceResult } }
    set { lock.withLock { storedRemoveSourceResult = newValue } }
  }

  var addModelResult: ModelMutationResult? {
    get { lock.withLock { storedAddModelResult } }
    set { lock.withLock { storedAddModelResult = newValue } }
  }

  var defaultModelResult: ModelMutationResult? {
    get { lock.withLock { storedDefaultModelResult } }
    set { lock.withLock { storedDefaultModelResult = newValue } }
  }

  var modelTestResult: ModelTestResult? {
    get { lock.withLock { storedModelTestResult } }
    set { lock.withLock { storedModelTestResult = newValue } }
  }

  var modelRemovalResult: ModelRemovalResult? {
    get { lock.withLock { storedModelRemovalResult } }
    set { lock.withLock { storedModelRemovalResult = newValue } }
  }

  init(snapshot: AppSnapshot, revisions: [StateRevision] = []) {
    snapshots = [snapshot]
    self.revisions = revisions.isEmpty ? [snapshot.revision] : revisions
  }

  var refreshIdentifiers: [String] {
    lock.withLock { storedRefreshIdentifiers }
  }

  var refreshWorkIdentifiers: [String] {
    lock.withLock { storedRefreshWorkIdentifiers }
  }

  var reservedRefreshIdentifiers: [String] {
    lock.withLock { storedReservedRefreshIdentifiers }
  }

  var releasedRefreshReservationIdentifiers: [String] {
    lock.withLock { storedReleasedRefreshReservationIdentifiers }
  }

  var cancelledIdentifiers: [String] {
    lock.withLock { storedCancelledIdentifiers }
  }

  var snapshotCalls: Int {
    lock.withLock { storedSnapshotCalls }
  }

  var stateRevisionCalls: Int {
    lock.withLock { storedStateRevisionCalls }
  }

  var maximumConcurrentBridgeCalls: Int {
    lock.withLock { storedMaximumConcurrentBridgeCalls }
  }

  var refreshEntryCalls: Int {
    lock.withLock { storedRefreshEntryCalls }
  }

  var modelInputs: [ModelProfileInput] {
    lock.withLock { storedModelInputs }
  }

  var addModelRequests: [ModelProfileInput] {
    lock.withLock { storedModelInputs }
  }

  var defaultModelRequests: [String] {
    lock.withLock { storedDefaultModelRequests }
  }

  var testModelRequests: [String] {
    lock.withLock { storedTestModelRequests }
  }

  var removeModelRequests: [String] {
    lock.withLock { storedRemoveModelRequests }
  }

  var savedRequests: [(storyID: String, saved: Bool)] {
    lock.withLock { storedSavedRequests }
  }

  var readRequests: [(storyID: String, read: Bool)] {
    lock.withLock { storedReadRequests }
  }

  var summaryRequests: [SummaryRequest] {
    lock.withLock { storedSummaryRequests }
  }

  var generationRequests: [GenerationRequest] {
    lock.withLock { storedGenerationRequests }
  }

  var addSourceInputs: [FeedSourceInput] {
    lock.withLock { storedAddSourceInputs }
  }

  var sourceToggleRequests: [(id: String, enabled: Bool)] {
    lock.withLock { storedSourceToggleRequests }
  }

  var sourceRemovalRequests: [String] {
    lock.withLock { storedSourceRemovalRequests }
  }

  let savedMutationRevision = StateRevision(
    dataGeneration: 2,
    sourceConfigRevision: "source-a"
  )

  func enqueueSnapshot(_ snapshot: AppSnapshot) {
    lock.withLock { snapshots.append(snapshot) }
  }

  func suspendNextSnapshot() {
    lock.withLock { suspendedSnapshotCount += 1 }
  }

  func suspendNextStateRevision() {
    lock.withLock { suspendedRevisionCount += 1 }
  }

  func suspendNextRefreshBeforeRegistration() {
    lock.withLock { suspendedRefreshStartCount += 1 }
  }

  func suspendNextSavedMutation() {
    lock.withLock { suspendedSavedMutationCount += 1 }
  }

  func suspendNextReadMutation() {
    lock.withLock { suspendedReadMutationCount += 1 }
  }

  func suspendNextSummaryMutation() {
    lock.withLock { suspendedSummaryMutationCount += 1 }
  }

  func suspendNextGeneration() {
    lock.withLock { suspendedGenerationCount += 1 }
  }

  func suspendNextAddSource() {
    lock.withLock { suspendedAddSourceCount += 1 }
  }

  func suspendNextSourceToggle() {
    lock.withLock { suspendedSourceToggleCount += 1 }
  }

  func suspendNextSourceRemoval() {
    lock.withLock { suspendedSourceRemovalCount += 1 }
  }

  func suspendNextAddModel() {
    lock.withLock { suspendedAddModelCount += 1 }
  }

  func suspendNextDefaultModel() {
    lock.withLock { suspendedDefaultModelCount += 1 }
  }

  func suspendNextModelTest() {
    lock.withLock { suspendedModelTestCount += 1 }
  }

  func suspendNextModelRemoval() {
    lock.withLock { suspendedModelRemovalCount += 1 }
  }

  func releaseSnapshots() {
    let pending = lock.withLock {
      let pending = pendingSnapshots
      pendingSnapshots.removeAll()
      for _ in pending { endBridgeCallLocked() }
      return pending
    }
    for (continuation, result) in pending {
      continuation.resume(with: result)
    }
  }

  func releaseStateRevisions() {
    let pending = lock.withLock {
      let pending = pendingRevisions
      pendingRevisions.removeAll()
      for _ in pending { endBridgeCallLocked() }
      return pending
    }
    for (continuation, result) in pending {
      continuation.resume(with: result)
    }
  }

  func releaseRefreshStarts() {
    let pending = lock.withLock {
      let pending = pendingRefreshStarts
      pendingRefreshStarts.removeAll()
      return pending
    }
    for continuation in pending {
      continuation.resume()
    }
  }

  func releaseSavedMutations() {
    let pending = lock.withLock {
      let pending = pendingSavedMutations
      pendingSavedMutations.removeAll()
      for _ in pending { endBridgeCallLocked() }
      return pending
    }
    for (continuation, result) in pending {
      continuation.resume(with: result)
    }
  }

  func releaseReadMutations() {
    let pending = lock.withLock {
      let pending = pendingReadMutations
      pendingReadMutations.removeAll()
      for _ in pending { endBridgeCallLocked() }
      return pending
    }
    for (continuation, result) in pending {
      continuation.resume(with: result)
    }
  }

  func releaseSummaryMutations() {
    let pending = lock.withLock {
      let pending = pendingSummaryMutations
      pendingSummaryMutations.removeAll()
      for _ in pending { endBridgeCallLocked() }
      return pending
    }
    for (continuation, result) in pending {
      continuation.resume(with: result)
    }
  }

  func releaseGenerations() {
    let pending = lock.withLock {
      let pending = pendingGenerations
      pendingGenerations.removeAll()
      for _ in pending { endBridgeCallLocked() }
      return pending
    }
    for (continuation, result) in pending {
      continuation.resume(with: result)
    }
  }

  func releaseAddSources() {
    releaseSourceMutations(&pendingAddSources)
  }

  func releaseSourceToggles() {
    releaseSourceMutations(&pendingSourceToggles)
  }

  func releaseSourceRemovals() {
    releaseSourceMutations(&pendingSourceRemovals)
  }

  func releaseAddModels() {
    let pending = lock.withLock {
      let pending = pendingAddModels
      pendingAddModels.removeAll()
      for _ in pending { endBridgeCallLocked() }
      return pending
    }
    for (continuation, result) in pending { continuation.resume(with: result) }
  }

  func releaseDefaultModels() {
    let pending = lock.withLock {
      let pending = pendingDefaultModels
      pendingDefaultModels.removeAll()
      for _ in pending { endBridgeCallLocked() }
      return pending
    }
    for (continuation, result) in pending { continuation.resume(with: result) }
  }

  func releaseModelTests() {
    let pending = lock.withLock {
      let pending = pendingModelTests
      pendingModelTests.removeAll()
      for _ in pending { endBridgeCallLocked() }
      return pending
    }
    for (continuation, result) in pending { continuation.resume(with: result) }
  }

  func releaseModelRemovals() {
    let pending = lock.withLock {
      let pending = pendingModelRemovals
      pendingModelRemovals.removeAll()
      for _ in pending { endBridgeCallLocked() }
      return pending
    }
    for (continuation, result) in pending { continuation.resume(with: result) }
  }

  private func releaseSourceMutations(
    _ storage: inout [(SourceMutationContinuation, Result<SourceMutationResult, any Error>)]
  ) {
    let pending = lock.withLock {
      let pending = storage
      storage.removeAll()
      for _ in pending { endBridgeCallLocked() }
      return pending
    }
    for (continuation, result) in pending {
      continuation.resume(with: result)
    }
  }

  func snapshot() async throws -> AppSnapshot {
    try await withCheckedThrowingContinuation { continuation in
      let immediate = lock.withLock { () -> Result<AppSnapshot, any Error>? in
        beginBridgeCallLocked()
        storedSnapshotCalls += 1
        let result: Result<AppSnapshot, any Error>
        if let storedSnapshotError {
          result = .failure(storedSnapshotError)
        } else {
          result = .success(snapshots[min(snapshotIndex, snapshots.count - 1)])
          snapshotIndex += 1
        }
        if suspendedSnapshotCount > 0 {
          suspendedSnapshotCount -= 1
          pendingSnapshots.append((continuation, result))
          return nil
        }
        endBridgeCallLocked()
        return result
      }
      if let immediate {
        continuation.resume(with: immediate)
      }
    }
  }

  func stateRevision() async throws -> StateRevision {
    try await withCheckedThrowingContinuation { continuation in
      let immediate = lock.withLock { () -> Result<StateRevision, any Error>? in
        beginBridgeCallLocked()
        storedStateRevisionCalls += 1
        let result = Result<StateRevision, any Error>.success(
          revisions[min(revisionIndex, revisions.count - 1)])
        revisionIndex += 1
        if suspendedRevisionCount > 0 {
          suspendedRevisionCount -= 1
          pendingRevisions.append((continuation, result))
          return nil
        }
        endBridgeCallLocked()
        return result
      }
      if let immediate {
        continuation.resume(with: immediate)
      }
    }
  }

  func reserveRefresh(operationID: String) throws {
    let reserved = lock.withLock { () -> Bool in
      guard activeRefreshReservationID == nil else { return false }
      activeRefreshReservationID = operationID
      refreshReservationIsRunning = false
      storedReservedRefreshIdentifiers.append(operationID)
      return true
    }
    guard reserved else { throw BridgeError.refreshAlreadyRunning }
  }

  func releaseRefreshReservation(operationID: String) -> Bool {
    lock.withLock {
      storedReleasedRefreshReservationIdentifiers.append(operationID)
      guard activeRefreshReservationID == operationID, !refreshReservationIsRunning else {
        return false
      }
      activeRefreshReservationID = nil
      cancellationRequests.remove(operationID)
      return true
    }
  }

  func refresh(operationID: String, ai: Bool) async throws -> RefreshResult {
    let shouldSuspend = lock.withLock {
      storedRefreshEntryCalls += 1
      guard suspendedRefreshStartCount > 0 else { return false }
      suspendedRefreshStartCount -= 1
      return true
    }
    if shouldSuspend {
      await withCheckedContinuation { continuation in
        lock.withLock { pendingRefreshStarts.append(continuation) }
      }
    }
    return try await withCheckedThrowingContinuation { continuation in
      let immediateError = lock.withLock { () -> (any Error)? in
        beginBridgeCallLocked()
        storedRefreshIdentifiers.append(operationID)
        guard activeRefreshReservationID == operationID else {
          endBridgeCallLocked()
          return BridgeError.refreshAlreadyRunning
        }
        refreshReservationIsRunning = true
        if let storedRefreshError {
          activeRefreshReservationID = nil
          refreshReservationIsRunning = false
          endBridgeCallLocked()
          return storedRefreshError
        }
        if cancellationRequests.contains(operationID) {
          activeRefreshReservationID = nil
          refreshReservationIsRunning = false
          cancellationRequests.remove(operationID)
          endBridgeCallLocked()
          return BridgeError.cancelled
        }
        storedRefreshWorkIdentifiers.append(operationID)
        pendingRefreshes[operationID] = continuation
        return nil
      }
      if let immediateError {
        continuation.resume(throwing: immediateError)
      }
    }
  }

  func finishRefresh(with result: RefreshResult) {
    let continuations = lock.withLock { () -> [RefreshContinuation] in
      let continuations = Array(pendingRefreshes.values)
      pendingRefreshes.removeAll()
      activeRefreshReservationID = nil
      refreshReservationIsRunning = false
      for _ in continuations { endBridgeCallLocked() }
      return continuations
    }
    for continuation in continuations {
      continuation.resume(returning: result)
    }
  }

  func cancelOperation(id: String) -> Bool {
    let (matched, continuation) = lock.withLock {
      () -> (Bool, RefreshContinuation?) in
      storedCancelledIdentifiers.append(id)
      let matched = activeRefreshReservationID == id
      if matched {
        cancellationRequests.insert(id)
      }
      let continuation = pendingRefreshes.removeValue(forKey: id)
      if continuation != nil {
        activeRefreshReservationID = nil
        refreshReservationIsRunning = false
        cancellationRequests.remove(id)
        endBridgeCallLocked()
      }
      return (matched, continuation)
    }
    if let continuation {
      continuation.resume(throwing: BridgeError.cancelled)
    }
    return matched
  }

  func setSaved(storyID: String, saved: Bool) async throws -> StoryMutationResult {
    try await withCheckedThrowingContinuation { continuation in
      let immediate = lock.withLock { () -> Result<StoryMutationResult, any Error>? in
        beginBridgeCallLocked()
        storedSavedRequests.append((storyID, saved))
        let result: Result<StoryMutationResult, any Error>
        if let storedSavedError {
          result = .failure(storedSavedError)
        } else if let storedSavedResult {
          result = .success(storedSavedResult)
        } else {
          let story = Story.fixture
          result = .success(
            StoryMutationResult(
              story: Story(
                id: story.id,
                title: story.title,
                canonicalURL: story.canonicalURL,
                excerpt: story.excerpt,
                category: story.category,
                publishedAt: story.publishedAt,
                sourceIDs: story.sourceIDs,
                score: story.score,
                smartSummary: story.smartSummary,
                isRead: story.isRead,
                isSaved: saved,
                selectedSummary: story.selectedSummary,
                summaryVariants: story.summaryVariants
              ),
              revision: savedMutationRevision
            )
          )
        }
        enqueueStorySnapshotIfNeededLocked(result)
        if suspendedSavedMutationCount > 0 {
          suspendedSavedMutationCount -= 1
          pendingSavedMutations.append((continuation, result))
          return nil
        }
        endBridgeCallLocked()
        return result
      }
      if let immediate { continuation.resume(with: immediate) }
    }
  }

  func setRead(storyID: String, read: Bool) async throws -> StoryMutationResult {
    try await withCheckedThrowingContinuation { continuation in
      let immediate = lock.withLock { () -> Result<StoryMutationResult, any Error>? in
        beginBridgeCallLocked()
        storedReadRequests.append((storyID, read))
        let result: Result<StoryMutationResult, any Error>
        if let storedReadError {
          result = .failure(storedReadError)
        } else {
          result = .success(
            storedReadResult
              ?? StoryMutationResult(story: .fixture, revision: savedMutationRevision)
          )
        }
        enqueueStorySnapshotIfNeededLocked(result)
        if suspendedReadMutationCount > 0 {
          suspendedReadMutationCount -= 1
          pendingReadMutations.append((continuation, result))
          return nil
        }
        endBridgeCallLocked()
        return result
      }
      if let immediate { continuation.resume(with: immediate) }
    }
  }

  func selectSummary(storyID: String, variantID: String) async throws -> StoryMutationResult {
    try await withCheckedThrowingContinuation { continuation in
      let immediate = lock.withLock { () -> Result<StoryMutationResult, any Error>? in
        beginBridgeCallLocked()
        storedSummaryRequests.append(SummaryRequest(storyID: storyID, variantID: variantID))
        let result: Result<StoryMutationResult, any Error>
        if let storedSummaryError {
          result = .failure(storedSummaryError)
        } else {
          result = .success(
            storedSummaryResult
              ?? StoryMutationResult(story: .fixture, revision: savedMutationRevision)
          )
        }
        enqueueStorySnapshotIfNeededLocked(result)
        if suspendedSummaryMutationCount > 0 {
          suspendedSummaryMutationCount -= 1
          pendingSummaryMutations.append((continuation, result))
          return nil
        }
        endBridgeCallLocked()
        return result
      }
      if let immediate { continuation.resume(with: immediate) }
    }
  }

  func regenerate(storyID: String, profile: String?, force: Bool) async throws -> GenerationResult {
    try await withCheckedThrowingContinuation { continuation in
      let immediate = lock.withLock { () -> Result<GenerationResult, any Error>? in
        beginBridgeCallLocked()
        storedGenerationRequests.append(
          GenerationRequest(storyID: storyID, profileID: profile, force: force)
        )
        let result: Result<GenerationResult, any Error>
        if let storedGenerationError {
          result = .failure(storedGenerationError)
        } else {
          result = .success(
            storedGenerationResult
              ?? GenerationResult(story: .fixture, selectedSummary: .fixture, revision: .fixture)
          )
        }
        if case .success(let generation) = result {
          enqueueStorySnapshotIfNeededLocked(
            .success(
              StoryMutationResult(story: generation.story, revision: generation.revision)
            )
          )
        }
        if suspendedGenerationCount > 0 {
          suspendedGenerationCount -= 1
          pendingGenerations.append((continuation, result))
          return nil
        }
        endBridgeCallLocked()
        return result
      }
      if let immediate { continuation.resume(with: immediate) }
    }
  }
  func addSource(_ input: FeedSourceInput) async throws -> SourceMutationResult {
    try await withCheckedThrowingContinuation { continuation in
      let immediate = lock.withLock { () -> Result<SourceMutationResult, any Error>? in
        beginBridgeCallLocked()
        storedAddSourceInputs.append(input)
        let result = sourceMutationResult(storedAddSourceResult)
        enqueueSourceSnapshotIfNeededLocked(result, operation: .upsert)
        if suspendedAddSourceCount > 0 {
          suspendedAddSourceCount -= 1
          pendingAddSources.append((continuation, result))
          return nil
        }
        endBridgeCallLocked()
        return result
      }
      if let immediate { continuation.resume(with: immediate) }
    }
  }

  func setSourceEnabled(id: String, enabled: Bool) async throws -> SourceMutationResult {
    try await withCheckedThrowingContinuation { continuation in
      let immediate = lock.withLock { () -> Result<SourceMutationResult, any Error>? in
        beginBridgeCallLocked()
        storedSourceToggleRequests.append((id, enabled))
        let result = sourceMutationResult(storedSourceToggleResult)
        enqueueSourceSnapshotIfNeededLocked(result, operation: .upsert)
        if suspendedSourceToggleCount > 0 {
          suspendedSourceToggleCount -= 1
          pendingSourceToggles.append((continuation, result))
          return nil
        }
        endBridgeCallLocked()
        return result
      }
      if let immediate { continuation.resume(with: immediate) }
    }
  }

  func removeSource(id: String) async throws -> SourceMutationResult {
    try await withCheckedThrowingContinuation { continuation in
      let immediate = lock.withLock { () -> Result<SourceMutationResult, any Error>? in
        beginBridgeCallLocked()
        storedSourceRemovalRequests.append(id)
        let result = sourceMutationResult(storedRemoveSourceResult)
        enqueueSourceSnapshotIfNeededLocked(result, operation: .remove)
        if suspendedSourceRemovalCount > 0 {
          suspendedSourceRemovalCount -= 1
          pendingSourceRemovals.append((continuation, result))
          return nil
        }
        endBridgeCallLocked()
        return result
      }
      if let immediate { continuation.resume(with: immediate) }
    }
  }

  func addModel(_ input: ModelProfileInput) async throws -> ModelMutationResult {
    try await withCheckedThrowingContinuation { continuation in
      let immediate = lock.withLock { () -> Result<ModelMutationResult, any Error>? in
        beginBridgeCallLocked()
        storedModelInputs.append(input)
        let result: Result<ModelMutationResult, any Error> =
          storedModelError.map(Result.failure)
          ?? .success(
            storedAddModelResult ?? ModelMutationResult(profile: .fixture, revision: .fixture)
          )
        if suspendedAddModelCount > 0 {
          suspendedAddModelCount -= 1
          pendingAddModels.append((continuation, result))
          return nil
        }
        endBridgeCallLocked()
        return result
      }
      if let immediate { continuation.resume(with: immediate) }
    }
  }

  func setDefaultModel(_ selector: String) async throws -> ModelMutationResult {
    try await withCheckedThrowingContinuation { continuation in
      let immediate = lock.withLock { () -> Result<ModelMutationResult, any Error>? in
        beginBridgeCallLocked()
        storedDefaultModelRequests.append(selector)
        let result: Result<ModelMutationResult, any Error> =
          storedModelError.map(Result.failure)
          ?? .success(
            storedDefaultModelResult ?? ModelMutationResult(profile: .fixture, revision: .fixture)
          )
        if suspendedDefaultModelCount > 0 {
          suspendedDefaultModelCount -= 1
          pendingDefaultModels.append((continuation, result))
          return nil
        }
        endBridgeCallLocked()
        return result
      }
      if let immediate { continuation.resume(with: immediate) }
    }
  }

  func testModel(_ selector: String) async throws -> ModelTestResult {
    try await withCheckedThrowingContinuation { continuation in
      let immediate = lock.withLock { () -> Result<ModelTestResult, any Error>? in
        beginBridgeCallLocked()
        storedTestModelRequests.append(selector)
        let result: Result<ModelTestResult, any Error> =
          storedModelError.map(Result.failure)
          ?? .success(
            storedModelTestResult
              ?? ModelTestResult(profile: .fixture, costMayApply: true, revision: .fixture)
          )
        if suspendedModelTestCount > 0 {
          suspendedModelTestCount -= 1
          pendingModelTests.append((continuation, result))
          return nil
        }
        endBridgeCallLocked()
        return result
      }
      if let immediate { continuation.resume(with: immediate) }
    }
  }

  func removeModel(_ selector: String) async throws -> ModelRemovalResult {
    try await withCheckedThrowingContinuation { continuation in
      let immediate = lock.withLock { () -> Result<ModelRemovalResult, any Error>? in
        beginBridgeCallLocked()
        storedRemoveModelRequests.append(selector)
        let result: Result<ModelRemovalResult, any Error> =
          storedModelError.map(Result.failure)
          ?? .success(
            storedModelRemovalResult
              ?? ModelRemovalResult(
                profile: .fixture,
                credentialDeletion: .deleted,
                revision: .fixture
              )
          )
        if suspendedModelRemovalCount > 0 {
          suspendedModelRemovalCount -= 1
          pendingModelRemovals.append((continuation, result))
          return nil
        }
        endBridgeCallLocked()
        return result
      }
      if let immediate { continuation.resume(with: immediate) }
    }
  }

  private func beginBridgeCallLocked() {
    activeBridgeCallCount += 1
    storedMaximumConcurrentBridgeCalls = max(
      storedMaximumConcurrentBridgeCalls,
      activeBridgeCallCount
    )
  }

  private func sourceMutationResult(
    _ configured: SourceMutationResult?
  ) -> Result<SourceMutationResult, any Error> {
    if let storedSourceError {
      return .failure(storedSourceError)
    }
    return .success(
      configured ?? SourceMutationResult(source: .fixture, revision: .fixture)
    )
  }

  private enum SourceSnapshotOperation {
    case upsert
    case remove
  }

  /// Mirrors the real bridge's durable mutation side effect when a test has not
  /// supplied a more authoritative post-mutation snapshot explicitly.
  private func enqueueStorySnapshotIfNeededLocked(
    _ result: Result<StoryMutationResult, any Error>
  ) {
    guard snapshots.count <= snapshotIndex, case .success(let mutation) = result,
      let current = snapshots.last
    else { return }

    func replacing(_ stories: [Story]) -> [Story] {
      stories.map { $0.id == mutation.story.id ? mutation.story : $0 }
    }

    let today = current.today.map { briefing in
      Briefing(
        date: briefing.date,
        generatedAt: briefing.generatedAt,
        isStale: briefing.isStale,
        items: briefing.items.map { item in
          guard item.story.id == mutation.story.id else { return item }
          return BriefingItem(
            position: item.position,
            section: item.section,
            isStale: item.isStale,
            story: mutation.story,
            selectedSummary: mutation.story.selectedSummary,
            summaryVariants: mutation.story.summaryVariants
          )
        }
      )
    }
    var saved = replacing(current.saved)
    if mutation.story.isSaved {
      if !saved.contains(where: { $0.id == mutation.story.id }) {
        saved.append(mutation.story)
      }
    } else {
      saved.removeAll(where: { $0.id == mutation.story.id })
    }
    snapshots.append(
      AppSnapshot(
        revision: mutation.revision,
        status: current.status,
        today: today,
        latest: replacing(current.latest),
        saved: saved,
        sources: current.sources,
        modelProfiles: current.modelProfiles,
        defaultModelProfileID: current.defaultModelProfileID,
        hasUsableAIProfile: current.hasUsableAIProfile
      )
    )
  }

  private func enqueueSourceSnapshotIfNeededLocked(
    _ result: Result<SourceMutationResult, any Error>,
    operation: SourceSnapshotOperation
  ) {
    guard snapshots.count <= snapshotIndex, case .success(let mutation) = result,
      let current = snapshots.last
    else { return }

    var sources = current.sources
    sources.removeAll(where: { $0.id == mutation.source.id })
    if case .upsert = operation {
      sources.append(mutation.source)
    }
    snapshots.append(
      AppSnapshot(
        revision: mutation.revision,
        status: current.status,
        today: current.today,
        latest: current.latest,
        saved: current.saved,
        sources: sources,
        modelProfiles: current.modelProfiles,
        defaultModelProfileID: current.defaultModelProfileID,
        hasUsableAIProfile: current.hasUsableAIProfile
      )
    )
  }

  private func endBridgeCallLocked() {
    activeBridgeCallCount -= 1
  }
}

struct SummaryRequest: Sendable, Equatable {
  let storyID: String
  let variantID: String
}

struct GenerationRequest: Sendable, Equatable {
  let storyID: String
  let profileID: String?
  let force: Bool
}

struct DetailedFakeError: Error, CustomStringConvertible, Sendable {
  let description: String
}

extension StateRevision {
  static let fixture = StateRevision(dataGeneration: 1, sourceConfigRevision: "source-a")
}

extension SummaryVariant {
  static let fixture = SummaryVariant(
    id: "variant-1",
    storyID: "story-1",
    profileID: "profile-1",
    provider: .openAI,
    model: "model-1",
    dialect: .responses,
    fields: SummaryFields(whatHappened: "What", whyItMatters: "Why", caveat: nil),
    generatedAt: Date(timeIntervalSince1970: 1_700_000_000)
  )
}

extension Story {
  static let fixture = Story(
    id: "story-1",
    title: "A signal",
    canonicalURL: "https://example.test/story",
    excerpt: "Original text",
    category: "research",
    publishedAt: Date(timeIntervalSince1970: 1_700_000_000),
    sourceIDs: ["source-1"],
    score: Score(recency: 1, sourceWeight: 0.8, corroboration: 0.5, total: 2.3),
    smartSummary: "Smart summary",
    isRead: false,
    isSaved: false,
    selectedSummary: .fixture,
    summaryVariants: [.fixture]
  )
}

extension Source {
  static let fixture = Source(
    id: "source-1",
    name: "Example",
    category: "research",
    enabled: true,
    weight: 0.8,
    feedURL: "https://example.test/feed.xml",
    origin: .standard
  )
}

extension ModelProfile {
  static let fixture = ModelProfile(
    id: "profile-1",
    name: "Example model",
    provider: .openAI,
    model: "model-1",
    endpoint: nil,
    dialect: .responses,
    credentialSource: .systemStore,
    consentedAt: Date(timeIntervalSince1970: 1_700_000_000),
    enabled: true,
    limits: ProfileLimits(
      maxSummariesPerRefresh: 5,
      maxDailyCostMicrousd: nil,
      inputCostMicrousdPerMillion: nil,
      outputCostMicrousdPerMillion: nil,
      maxOutputTokens: 384,
      timeoutSeconds: 30,
      maxRetries: 2
    ),
    createdAt: Date(timeIntervalSince1970: 1_700_000_000),
    updatedAt: Date(timeIntervalSince1970: 1_700_000_000)
  )
}

extension AppSnapshot {
  static let fixture = AppSnapshot(
    revision: .fixture,
    status: CollectionStatus(
      state: .ready,
      refresh: RefreshMetadata(
        lastRefreshAt: Date(timeIntervalSince1970: 1_700_000_000),
        storyCount: 1
      )
    ),
    today: Briefing(
      date: "2026-08-30",
      generatedAt: Date(timeIntervalSince1970: 1_700_000_000),
      isStale: false,
      items: [
        BriefingItem(
          position: 1,
          section: "Top Signals",
          isStale: false,
          story: .fixture,
          selectedSummary: .fixture,
          summaryVariants: [.fixture]
        )
      ]
    ),
    latest: [.fixture],
    saved: [],
    sources: [.fixture],
    modelProfiles: [.fixture],
    defaultModelProfileID: "profile-1",
    hasUsableAIProfile: true
  )
}

extension RefreshResult {
  static let fixture = RefreshResult(
    briefing: AppSnapshot.fixture.today!,
    successfulSources: 1,
    failedSources: 0,
    generation: GenerationReport(
      eligible: 1,
      generated: 1,
      cacheHits: 0,
      skippedCap: 0,
      skippedBudget: 0,
      missingCredentials: 0,
      providerFailures: 0,
      malformedOutputs: 0,
      smartFallbacks: 0
    ),
    revision: .fixture
  )
}
