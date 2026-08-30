import Foundation
import Testing

@testable import SignalAppKit

@Suite(.serialized)
struct PreviewFixtureTests {
  @Test
  func requiredVisualStatesHaveUniqueStableIdentifiers() {
    // Break caught: a named preview disappearing or two deterministic states sharing cache identity.
    let required = [
      PreviewFixtures.welcome,
      PreviewFixtures.empty,
      PreviewFixtures.populated,
      PreviewFixtures.selectedAI,
      PreviewFixtures.smartFallback,
      PreviewFixtures.stalePartialRefresh,
      PreviewFixtures.offlineCachedBriefing,
      PreviewFixtures.providerFailure,
      PreviewFixtures.darkAppearance,
      PreviewFixtures.reducedTransparency,
      PreviewFixtures.increasedContrast,
    ]

    #expect(
      required.map(\.id) == [
        "welcome", "empty", "populated", "selected-ai", "smart-fallback",
        "stale-partial-refresh", "offline-cached-briefing", "provider-failure",
        "dark-appearance", "reduced-transparency", "increased-contrast",
      ])
    #expect(Set(required.map(\.id)).count == required.count)
  }

  @Test
  func everyApplicationPhaseHasADeterministicFixture() {
    // Break caught: introducing or dropping an app phase without a reproducible non-Xcode state.
    #expect(Set(PreviewFixtures.all.map(\.phaseKind)) == Set(AppPhaseKind.allCases))
  }

  @Test
  func fixtureStoriesAndSourcesUseValidWebURLsAndFixedTimestamps() {
    // Break caught: previews silently depending on current time or carrying a malformed link.
    let snapshots = PreviewFixtures.all.compactMap(\.snapshot)
    let stories = snapshots.flatMap { snapshot in
      let today = snapshot.today?.items.map(\.story) ?? []
      return today + snapshot.latest + snapshot.saved
    }
    let sources = snapshots.flatMap(\.sources)
    var optionalDates: [Date?] = []
    for snapshot in snapshots {
      optionalDates.append(snapshot.status.refresh?.lastRefreshAt)
      optionalDates.append(snapshot.today?.generatedAt)
      for story in snapshot.latest {
        optionalDates.append(story.publishedAt)
        optionalDates.append(contentsOf: story.summaryVariants.map(\.generatedAt))
      }
      for profile in snapshot.modelProfiles {
        optionalDates.append(contentsOf: [
          profile.consentedAt, profile.createdAt, profile.updatedAt,
        ])
      }
    }
    let dates = optionalDates.compactMap { $0 }

    #expect(!stories.isEmpty)
    #expect(stories.allSatisfy { StorySourceURL.validated($0.canonicalURL) != nil })
    #expect(sources.allSatisfy { StorySourceURL.validated($0.feedURL) != nil })
    #expect(!dates.isEmpty)
    #expect(dates.allSatisfy { $0 <= PreviewFixtures.referenceDate })
  }

  @Test
  func selectedAIAndSmartFallbackExerciseDifferentProvenanceBranches() {
    // Break caught: the two summary previews accidentally rendering the same provenance path.
    let ai = PreviewFixtures.selectedAI.selectedStory
    let smart = PreviewFixtures.smartFallback.selectedStory

    #expect(ai?.selectedSummary != nil)
    #expect(ai?.summaryVariants.isEmpty == false)
    #expect(smart?.selectedSummary == nil)
    #expect(smart?.summaryVariants.isEmpty == true)
    #expect(smart?.smartSummary.isEmpty == false)
  }

  @Test
  func populatedAndSelectedAIFixturesExerciseDifferentSelectionStates() {
    // Break caught: two named visual fixtures rendering the same selected detail state.
    #expect(PreviewFixtures.populated.snapshot == PreviewFixtures.selectedAI.snapshot)
    #expect(PreviewFixtures.populated.selectedStoryID == nil)
    #expect(PreviewFixtures.populated.selectedStory == nil)
    #expect(PreviewFixtures.selectedAI.selectedStoryID == "preview-story-ai")
    #expect(PreviewFixtures.selectedAI.selectedStory?.selectedSummary != nil)
  }

  @Test
  func failureFixturesPreserveCachedContentAndExplainTheFallback() {
    // Break caught: failure previews turning cached content into a misleading empty state.
    let fixture = PreviewFixtures.stalePartialRefresh
    let briefing = fixture.snapshot?.today
    let rowFlags = briefing?.items.map(\.isStale)
    let presentation = TodayPresentation(
      briefing: briefing,
      sources: fixture.snapshot?.sources ?? [],
      selectionForStory: { storyID in
        fixture.snapshot?.today?.items.first(where: { $0.story.id == storyID })?.story
          .selectedSummary.map { .ai(variantID: $0.id) } ?? .smart
      },
      relativeTo: PreviewFixtures.referenceDate
    )

    #expect(fixture.phase == .stale)
    #expect(SignalStatus(phase: fixture.phase).title == "Partially stale")
    #expect(briefing?.isStale == false)
    #expect(rowFlags == [true, false])
    #expect(presentation.sections.flatMap(\.rows).map(\.isStale) == [true, false])
    #expect(PreviewFixtures.offlineCachedBriefing.snapshot?.today?.items.isEmpty == false)
    #expect(PreviewFixtures.providerFailure.snapshot?.today?.items.isEmpty == false)
    #expect(PreviewFixtures.providerFailure.message?.contains("Smart") == true)
  }

  @Test
  func appearanceAndAccessibilityFixturesChangeOnlyTheirNamedEnvironment() {
    // Break caught: a visual variant quietly mutating the underlying story data.
    let baseline = PreviewFixtures.populated

    #expect(PreviewFixtures.darkAppearance.appearance == .dark)
    #expect(PreviewFixtures.reducedTransparency.reduceTransparency)
    #expect(PreviewFixtures.increasedContrast.increaseContrast)
    #expect(PreviewFixtures.darkAppearance.snapshot == baseline.snapshot)
    #expect(PreviewFixtures.reducedTransparency.snapshot == baseline.snapshot)
    #expect(PreviewFixtures.increasedContrast.snapshot == baseline.snapshot)
  }

  @Test
  func fixturesContainNeitherSecretFieldsNorRecognizableSecretValues() {
    // Break caught: preview data acquiring a credential-bearing field or a realistic API-key value.
    let findings = PreviewFixtureSecurityAudit.findings(in: PreviewFixtures.all)

    #expect(findings.isEmpty)
    #expect(
      PreviewFixtureSecurityAudit.findings(
        in: ["sk-preview-should-be-detected", "Authorization: Bearer preview-secret"]
      ).count == 2
    )
  }
}
