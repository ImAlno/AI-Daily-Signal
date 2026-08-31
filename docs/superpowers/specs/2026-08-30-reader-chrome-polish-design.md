# Reader and Chrome Polish

## Status

Approved in conversation on 2026-08-30. This specification refines the shipped responsive macOS redesign at commit `718b7f8`; it is a polish pass, not another application-shell rewrite.

## Product Thesis

AI Daily Signal is a calm desktop briefing for people who want to understand what changed in AI without entering a chat or operating a dashboard. The main window has one job: make a finite set of ranked signals easy to scan, open, understand, and leave.

The visual direction is **Quiet Signal Desk**: native macOS chrome, an opaque editorial reading surface, compact controls, and one recognizable rank line running through the briefing. It borrows Codex's restraint and density without copying its agent interface or branding.

## Goals

- Make collapsed stories visibly interactive without turning them into cards.
- Give the expanded story a deliberate editorial rhythm instead of a stack of default SwiftUI text styles.
- Put summary provenance and model/variant choice where the reader encounters the summary.
- Remove repeated page titles and reduce toolbar noise.
- Give expanded, rail, and compact navigation an unmistakable selected state.
- Restore a restrained sense of macOS material in navigation and title-bar chrome only.
- Make Sources and Models easier to scan by separating identity, status, advanced metadata, and actions.
- Preserve the responsive, accessible, local-first, bridge-confirmed behavior already implemented.

## Non-Goals

- No new Rust, FFI, database, source, model, generation, scheduling, or CLI behavior.
- No chat composer, prompts, dashboard widgets, analytics, or multi-pane detail view.
- No custom web font, gradients, glow, glass content cards, or persistent rounded containers.
- No model editing; creation, testing, default selection, and removal retain the existing contract.
- No redesign of the menu-bar popover or first-briefing flow beyond shared typography tokens.

## Visual System

### Color

Production code uses adaptive macOS semantic colors. The references below describe the intended hierarchy rather than fixed light-only colors:

| Token | macOS source | Light reference | Dark reference | Use |
|---|---|---:|---:|---|
| `contentSurface` | `textBackgroundColor` | `#FFFFFF` | `#1E1E1E` | Reading and settings content |
| `chromeSurface` | native sidebar/bar material | `#F2F2F3` | `#29292B` | Sidebar, rail, title-bar chrome |
| `primaryInk` | `labelColor` | `#1D1D1F` | `#F5F5F7` | Titles and story copy |
| `secondaryInk` | `secondaryLabelColor` | `#6E6E73` | `#A1A1A6` | Metadata and help text |
| `selectionSurface` | native selection/unemphasized selection | `#E8E8EA` | `#3A3A3D` | Active rail item and expanded-story header |
| `signalAccent` | app accent/tint | system accent | system accent | Rank number, active affordances, progress |

Color never carries selection, read state, freshness, or failure by itself.

### Typography

Use San Francisco through SwiftUI system fonts so Dynamic Type and platform substitutions remain native.

- Page title: 30-point semibold system display, compact leading.
- Story title: 21-point semibold when expanded; 15-point semibold/regular when collapsed according to unread/read state.
- Reading body: 15-point regular with approximately 5 points of additional line spacing.
- Section label: 13-point semibold, sentence case, secondary color.
- Metadata: 12-point regular, secondary color.
- Rank: 11-point medium monospaced digits.

Avoid large-title/title3 defaults inside the article body when they create disproportionate jumps. Text remains selectable in expanded stories.

### Spacing and Shape

- Base rhythm: 4, 8, 12, 16, 24, and 32 points.
- Reading width: maximum 680 points, centered.
- Wide/rail/compact side padding: 28/24/18 points, never below the supported 420-point window minimum.
- Permanent content has square/open structure with separators.
- A rounded selection surface appears only behind the active rail item and the hovered/expanded story header. Use a restrained 7-point radius and native selection color; it is an interaction state, not a card system.

### Signature Element

The numbered rank rail becomes the product signature. Ranked Today stories use a monospaced two-digit rank connected to a quiet one-point vertical signal line. Hovering or expanding a story strengthens only that story's rank and line. Latest and Saved remain unnumbered because their order is not a declared rank.

This is the one deliberate aesthetic risk. Everything else stays quiet.

## Window Chrome and Navigation

The `NavigationSplitView` architecture and existing 820/560 breakpoints remain unchanged.

### Expanded sidebar

Use the native sidebar material and native list selection. Keep destination titles and symbols. The window title identifies the product (`AI Daily Signal`), while the content header identifies the destination; do not show `Today` in both the title bar and page body.

### Rail

Each rail destination has:

- a 36-by-36-point interaction target;
- a native rounded selection background when active;
- tint plus `.isSelected` semantics;
- hover feedback and existing help text.

Inactive rail buttons have no visible container.

### Compact

Keep the native destination picker in the toolbar with its visible current destination and accessibility value. It remains the only navigation control; the default split-view sidebar toggle stays removed.

### Toolbar

Keep the title bar visually quiet:

- Refresh is the only persistent direct action.
- Open Source, Save/Unsave, and Preferences live in one standard overflow menu in every width mode.
- Story actions remain available beside the expanded story, so moving them out of the wide toolbar does not hide capability.
- Keyboard shortcuts remain Command-R, Command-O, Command-S, and Command-comma.
- Refresh progress may replace the Refresh symbol with native progress/cancel treatment; completion uses the existing transient status surface.

No custom glass capsule wraps toolbar controls. Material comes from the native title bar and sidebar.

## Reading Experience

### Page header

The reading surface begins with one content header:

- calendar date as metadata;
- destination title;
- signal/source metadata.

Reduce its bottom gap so the first signal is visually connected to the briefing. Estimated reading time remains out of scope unless it can be derived locally without a new persisted contract.

### Collapsed signal

A collapsed signal keeps the open, separator-based list but gains clear interaction feedback:

- the title, metadata, provenance, saved state, and optional rank remain visible;
- a small trailing disclosure chevron communicates that the row opens inline;
- pointer hover applies a temporary native selection surface to the story header area only;
- keyboard focus uses the system focus ring;
- the hit target spans the full row;
- title wrapping remains capped at three lines outside accessibility sizes and becomes unrestricted at accessibility sizes.

The row must not gain a persistent border, shadow, or card background.

### Expanded signal

Expansion keeps the story's header visible so opening a row feels continuous rather than replacing it with a different component. The header contains metadata, title, provenance, and a collapse chevron.

Immediately below provenance, show the existing Raw/Smart/AI summary selection as a compact menu or segmented control that adapts to available width. Its accessible label states `Summary version`; the selected value states the actual provenance/model. Do not wait until the bottom of the article to reveal which summary is being read.

Body order:

1. Original excerpt, when selected or required by existing presentation logic.
2. What happened.
3. Why it matters.
4. Caveat, when available.
5. Collapsed `Why this ranked here` disclosure.
6. Compact source/save/read/regenerate actions.

Use smaller sentence-case section labels, consistent body typography, and 20–24 points between semantic sections. Remove the duplicate summary picker from the bottom.

### Motion

Expansion/collapse and hover changes use one restrained 160–180 ms ease-out transition. Do not animate the entire scroll position or stagger individual text blocks. When Reduce Motion is enabled, state changes are immediate.

## Sources

Sources remain a native divided list with Standard and Personal sections.

Each row prioritizes:

1. Source name and enabled state.
2. Domain and category.
3. Origin and weight as tertiary metadata.

The enable switch remains directly visible. Personal-source removal moves into a small standard overflow menu unless the platform can show it as a trailing destructive action without crowding at 420 points. Busy progress and errors remain adjacent to the affected source.

The inline add form keeps a single scrolling owner, adds a compact page heading and one sentence of guidance, and sizes fields/actions to their content. It does not gain a card or sheet.

## Models

Model rows stop presenting provider, model, credential source, consent, endpoint, summary budget, token budget, and three actions at equal visual weight.

Each row shows by default:

- profile name plus a native `Default` status label when applicable;
- provider and model identifier;
- credential/consent readiness in plain language;
- one primary action: `Use as Default` when available;
- a standard overflow menu containing Test and Remove.

Endpoint and budget information move into a native disclosure section below the row. Testing retains its paid-network confirmation. Removal retains its history and Keychain disclosures. The inline add form follows the same heading, spacing, and one-scroll-owner rules as Sources.

## Preferences

Preferences remains status-oriented because most displayed values are not persisted user controls. Add a compact page heading and explanatory sentence, then use grouped native rows for Briefing and Companion. Do not make status values look editable.

If the page remains visually sparse, use width and vertical rhythm rather than cards or placeholder controls. No new preferences are invented for appearance, scheduling, or summary depth.

## Welcome

Keep the existing one-action welcome flow. Apply only the shared type scale, 680-point reading rhythm where appropriate, and title-bar/content-surface behavior. Do not add a mock dashboard, feature carousel, source wizard, or model setup step.

## State and Component Boundaries

- `AppModel` remains the source of truth for destination, selected story, summary selection, operations, and mutations.
- Hover state is view-local and never enters `AppModel`.
- Extract a shared story-header presentation/view only if it prevents the collapsed and expanded states from drifting; do not duplicate story action logic.
- Extend presentation values for visual state, labels, and action ordering only. Do not create marker booleans whose sole purpose is to make styling tests pass.
- Preserve bridge-confirmed source/model/story mutations and existing credential redaction.
- Preserve all responsive thresholds, minimum window size, command bindings, and menu-bar behavior.

## Accessibility

- The complete story header is one understandable accessibility element in collapsed mode.
- Expanded mode exposes title, provenance, summary selector, sections, and actions in reading order.
- Rail selection has both a visible selection surface and `.isSelected` semantics.
- Hover is never required to discover or operate an action.
- Disclosure controls state expanded/collapsed and use meaningful labels/hints.
- Dynamic Type may increase row height and remove title/metadata line limits.
- Reduce Motion disables polish animations.
- Reduce Transparency replaces material with opaque semantic chrome without flattening selection hierarchy.
- Increase Contrast retains visible separators, selection states, and focus rings.

## Implementation Boundaries

Expected Swift files include:

- `Views/ReadingWindowView.swift`
- `Views/AppNavigationView.swift`
- `Views/BriefingHeaderView.swift`
- `Views/SignalDisclosureView.swift`
- `Views/StoryRowView.swift`
- `Views/ExpandedStoryView.swift`
- `Views/SourcesView.swift`
- `Views/SourceEditorView.swift`
- `Views/ModelsSettingsView.swift`
- `Views/ModelProfileEditorView.swift`
- `Views/SettingsView.swift`
- `Views/WelcomeView.swift` only if needed for shared token alignment
- a focused design/presentation file if shared reader tokens would otherwise be duplicated.

Rust crates, UniFFI contracts, CLI behavior, packaging scripts, and menu-bar views are excluded.

## Testing

Use behavior-first tests for:

- one content destination title rather than duplicate shell/page titles;
- toolbar command placement and keyboard shortcuts;
- rail selection surface/semantics and compact selection value;
- collapsed/expanded header continuity and disclosure state;
- summary selector placement and selected provenance;
- Dynamic Type line-limit behavior;
- reduced-motion transition policy;
- Sources and Models action hierarchy;
- single-scroll-owner inline editors at 420 by 520;
- preservation of story, source, and model operations.

Run:

- focused Swift presentation and hosted-view tests;
- `scripts/test-swift-testing.sh`;
- `scripts/test-swift-package-modes.sh`;
- `cargo test --workspace --all-features` as the shared-core regression gate;
- macOS app build, exact-bundle verification, isolated smoke, and packaging adversarial checks;
- structural scans for forbidden cards, gradients, nested splits, editor sheets, and duplicate summary pickers.

Native visual inspection must cover 1100 by 720, 760 by 640, 480 by 620, and 420 by 520 in light/dark appearances, plus accessibility text, Reduce Transparency, Increase Contrast, keyboard focus, and Reduce Motion. If the noninteractive environment still cannot capture valid native frames, report that honestly and require a human/native inspection rather than accepting placeholders.

## Acceptance Criteria

- The shell uses native material only for navigation/title-bar chrome; reading and settings content remain opaque.
- The title bar and content do not repeat the active destination title.
- Refresh is the sole direct toolbar action; existing story/settings commands remain in overflow and retain shortcuts.
- Rail selection is visible without relying on tint alone.
- Collapsed stories visibly afford inline expansion without becoming permanent cards.
- Opening a story preserves header continuity, shows summary provenance/selection before the body, and animates only when Reduce Motion is off.
- Article typography follows one compact editorial hierarchy within a 680-point maximum column.
- Ranked Today stories use the signal-line signature; Latest and Saved do not imply ranks.
- Sources and Models present identity/status before advanced metadata and reduce row-level action noise.
- Inline forms retain one scrolling owner and fit the 420-by-520 minimum.
- Preferences remains honest status UI rather than fake controls.
- Existing accessibility, bridge-confirmed mutations, keyboard commands, responsive modes, standalone app packaging, and cross-platform CLI behavior remain intact.

## Implementation Verification — 2026-08-31

Acceptance coverage was audited before the release matrix. No additional acceptance-only test was needed:

| Criterion | Existing behavior or hosted-view evidence |
|---|---|
| 420-by-520 shown-window minimum | `shownWindowKeepsApprovedMinimumAfterSwiftUILayoutSettles` |
| 820/560 responsive transitions; 680-point reading maximum; 28/24/18 padding | `exactLayoutBreakpointsAreStable`, `navigationAndReadingMetricsMatchTheApprovedEditorialColumn` |
| Refresh-only direct toolbar; overflow commands and shortcuts | `toolbarKeepsOnlyRefreshDirectAndPlacesContextInOverflow`, `approvedKeyboardCommandsHaveTruthfulDescriptors` |
| Rail visible selection and selected semantics | hosted `railNavigationMakesTheCurrentDestinationVisiblySelected` |
| Compact current destination and absent sidebar toggle | `compactNavigationExposesTheCurrentDestinationAsItsValue`, hosted `compactShellHasNoSidebarToggleAfterLayoutSettles` |
| One selected story and continuous header expansion | `signalDisclosureExpansionIsDerivedOnlyFromSelectedStoryID`, `storyHeaderKeepsIdentityAndDisclosesItsState`, `firstValidStoryExpandsAndValidSelectionPersists` |
| Summary selector/provenance before the body | `pickerOrdersRawSmartThenImmutableAINewestFirst`, `expandedBodyExcludesIdentityAndKeepsSemanticOrder`, plus the live-flow structural check below |
| Accessibility title wrapping and Reduce Motion | `accessibilityTextLeavesStoryTitlesUnrestricted`, `sourceRowTextLimitsRelaxForAccessibilitySizes`, `reduceMotionDisablesReaderPolishAnimation` |
| Exactly one outer source/model editor scroll owner around each `.columns` `Form` | hosted `inlineEditorsHaveOneVerticalScrollingOwnerAtMinimumWindowSize` at exactly 420 by 520 |
| Source/model action and confirmation behavior | `sourceRowsPrioritizeIdentityStatusThenTertiaryMetadata`, `personalRemovalUsesConfirmationPolicyAndStandardRemovalNeverCallsBridge`, `modelRowsKeepIdentityAndReadinessVisibleWhileDeferringAdvancedMetadata`, `defaultTestAndRemovalRequirePolicyConfirmationAndPublishOnlyReconciledSnapshots` |
| Preferences status-only content | `preferencesExplainThatDisplayedValuesAreCurrentStatus`, hosted `settingsPagesUseContinuousRowStacksInsteadOfInsetOrGroupedContainers` |

The complete release matrix was run as separate commands from the polished worktree. Every command exited zero:

- `git diff --check`: no output.
- `scripts/test-swift-testing.sh`: 157 tests in 10 suites passed, including `AlphaAcceptanceTests` and the hosted acceptance regressions.
- `scripts/test-swift-package-modes.sh`: reported `Swift package modes are isolated`.
- `cargo test --workspace --all-features`: 264 tests passed; the one real OS credential-store contract remained ignored with its explicit unlocked-ephemeral-store requirement; there were no failures.
- `scripts/build-macos-app.sh`: production Swift build completed and assembled `target/macos/AI Daily Signal.app` with its bundled `libsignal_ffi.dylib`.
- `scripts/verify-macos-app.sh`: verified the exact standalone bundle.
- `scripts/smoke-test-macos-app.sh`: verified direct bundle launch under isolated `HOME`/`PATH`, absent overrides, created configuration and SQLite state, integrity, and binary-safe scans; the owned process was stopped by the script.
- `scripts/test-macos-packaging-hardening.sh`, `scripts/test-macos-smoke-ownership.sh`, and `scripts/test-macos-verifier-adversarial.sh`: all adversarial checks passed.

The required forbidden-pattern scan returned no matches in macOS sources or tests. Focused structural inspection found one `SummaryVariantPicker` in the live `TodayView`/`StoryListView` → `SignalDisclosureView` expanded flow, only Refresh in `directCommands`, Open Source/Save/Preferences rendered from `overflowCommands`, and exactly one outer `ScrollView` owning vertical scrolling around each source/model editor `.columns` `Form`. The branch diff contains no Rust crate, generated binding, bridge, CLI, menu-bar view, package manifest, or packaging-script change.

Safe deterministic native inspection produced complete component renders from the approved fixture at 1100 by 720 expanded, 760 by 640 rail, and 480 by 620 compact in both light and dark appearances, plus Today at 420 by 520. Those inspected renders showed adaptive opaque reading surfaces, responsive persistent-navigation changes, a visible narrow rail selection indicator, the numbered signal line, a continuous selected header, the summary selector immediately before article sections, compact wrapping, and the wide/rail article actions. Separate 420-by-520 full-window frames for the Sources editor and Preferences rendered complete enough to inspect their wrapped guidance, compact grid content, and opaque surfaces.

Native visual acceptance is not complete. Full-window off-screen captures of Today at 1100 by 720, 760 by 640, 480 by 620, and 420 by 520 produced black/partial SwiftUI frames even though the same production reader components rendered correctly without the shell. The Models editor frame clipped its leading title content and was rejected. Injected accessibility-size content rendered byte-identically to the standard-size frame, while keyboard-focus, Reduce Motion, Reduce Transparency, and Increase Contrast shell frames were also black/partial; none were accepted as evidence. Therefore toolbar/title-bar duplication and crowding, hover feel, live editor scrolling and validation wrapping, Dynamic Type, keyboard focus ring, Reduce Motion transitions, Reduce Transparency, Increase Contrast, and the complete Models editor still require human inspection in the native app. No global appearance or accessibility setting was changed, and no capture permission was requested.
