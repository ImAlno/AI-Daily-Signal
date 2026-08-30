# Responsive Codex-Inspired macOS Redesign

## Status

Approved in conversation on 2026-08-30. This document defines the design for the macOS companion redesign. It does not change the Rust core or cross-platform CLI behavior.

## Objective

Rework the macOS companion into a clean, responsive information product that feels native to macOS and borrows the restraint, density, and hierarchy of the Codex desktop app without copying OpenAI branding or agent workflows.

The app's job is to inform. It presents a finite daily briefing, lets the user inspect sources, and keeps configuration accessible without turning the experience into a dashboard, editor, or chat interface.

## Problems to Solve

The current reading window uses a nested `NavigationSplitView` and `HSplitView`. Its internal minimum widths exceed the outer window minimum, so panes clip or compress poorly as the window narrows. The visual treatment also relies too heavily on rounded containers and custom glass wrappers, making controls and settings feel oversized and generic.

The redesign must solve both issues together:

- remove the permanent three-pane email-client layout;
- allow the window to reflow cleanly at wide, medium, and narrow widths;
- establish one consistent app shell across reading, source, model, settings, and onboarding screens;
- use native macOS material only where it communicates navigation or window chrome;
- keep the reading surface quiet, opaque, and content-first;
- remove decorative cards, gradients, AI iconography, and oversized controls.

## Design Principles

### Content before chrome

The daily briefing is one continuous reading surface. Interface elements stay compact and recede when they are not needed.

### Native structure

Use standard SwiftUI navigation, toolbar, list, form, and material behavior. Custom visual wrappers may supplement a genuine platform gap, but they must not restyle every control.

### Codex-inspired restraint

Use monochrome surfaces, compact navigation, subtle separators, a narrow centered content column, small controls, and generous whitespace. Retain AI Daily Signal's own name, icon, source vocabulary, and accent color.

### Information, not conversation

Do not add a chat composer, prompt box, or assistant persona. The product summarizes and explains; it does not ask the user to perform work through a conversation.

### Responsive by construction

No child view may impose a minimum width that conflicts with the window's supported minimum. Layout changes must be defined by an explicit, testable policy rather than accidental compression.

## Information Architecture

The app uses one shared shell with these destinations:

1. Today
2. Latest
3. Saved
4. Sources
5. Models
6. Preferences

Today, Latest, and Saved use the same continuous briefing pattern. Sources, Models, and Preferences replace the main content area inside the shell. They do not open as oversized sheets.

The menu-bar popover remains a compact companion surface and retains its approved fixed-content behavior.

## Shared Window Shell

The shell contains:

- the native macOS title bar and traffic-light controls;
- a compact toolbar with Refresh and context-sensitive actions backed by existing commands;
- a leading navigation sidebar or icon rail;
- one opaque main content surface;
- subtle status text for freshness, active refresh, offline state, or failure.

The sidebar uses system material supplied by the navigation structure. The main content surface uses the system text or window background. Ordinary toolbar buttons use standard SwiftUI button styles and receive material from the system; they do not sit inside a custom glass capsule.

The visual hierarchy uses spacing, typography, and one-pixel separators. Rounded surfaces are reserved for the outer window and native controls that already require them.

## Responsive Layout Policy

Introduce a pure `AppLayoutMode` policy derived from the available content width:

- **Expanded, 820 points and wider:** show the full 228-point sidebar and the centered main content column.
- **Rail, 560 through 819 points:** show a 58-point icon rail and keep the main content readable.
- **Compact, below 560 points:** hide persistent navigation and expose destinations from a toolbar menu.

The supported reading-window minimum is 420 by 520 points. The ideal opening size remains a comfortable desktop window, but no view may require it.

The main reading column has a maximum readable width of 720 points and flexible horizontal padding. It does not grow into a wide dashboard on large displays.

`NavigationSplitView` remains the app-level navigation primitive. The layout policy controls whether its leading content renders as a full sidebar, icon rail, or hidden compact navigation. There is no nested `HSplitView`.

## Reading Experience

### Briefing header

The page begins with:

- the calendar date;
- a short `Today`, `Latest`, or `Saved` title;
- briefing metadata such as signal count, source count, and estimated reading time.

This is text on the reading surface, not a hero card.

### Signal rows

Signals form one vertically scrolling sequence separated by quiet rules.

The highest-priority valid signal starts expanded when a destination first loads. The remaining signals are compact rows. Expanding one signal collapses the previous signal. `selectedStoryID` remains the selection source of truth so keyboard commands, context menus, and bridge operations continue to target the visible story.

An expanded signal shows:

- source name, publication time, and source count;
- title;
- summary;
- a plain `Why this matters` label and explanation;
- text-sized actions for viewing sources and saving the story;
- existing summary variants or regeneration controls only when relevant.

There is no permanent detail pane, summary card, AI badge, or sparkle icon.

### Refresh behavior

The newest stored snapshot appears immediately. A refresh may run in the background using the existing operation and polling model.

Refresh and cancellation remain toolbar commands. Progress and completion appear as understated status text. Recoverable failures appear inline at the top of the reading column with one retry action. Startup storage failure remains blocking and does not offer an inoperative retry.

## Sources

Sources use a divided list grouped into Default and Custom.

Each row shows the source name, domain, category, and compact enable switch. Adding a custom source expands an inline form inside the main pane. Existing sources retain the supported enable, disable, and remove operations; this redesign does not introduce a new Rust source-update contract. The add flow does not present a large modal sheet.

Pending mutations, validation errors, and removal confirmation stay adjacent to the affected source. Existing bridge-confirmed mutation behavior remains authoritative; the UI must not claim success before the Rust core confirms the result.

## Models

Models become a first-class navigation destination rather than being buried inside general settings.

The destination presents configured model profiles as a quiet list. Each row exposes the existing test, default-selection, and removal actions beside provider, model identifier, API-key status, and budget information. Adding a profile reveals the existing creation form inline. Editing an existing profile remains outside the alpha contract because the bridge exposes no profile-update operation.

API keys remain masked and stored through the existing credential boundary. Validation and cleanup warnings appear next to the relevant profile or field. Provider and model choice remain user-controlled.

## Preferences

Preferences uses ordinary macOS settings rows for the app-owned values that already exist, including local storage, optional AI-summary status, the fixed menu-bar launch behavior, and CLI/shared-data compatibility. Values without a persisted preference or bridge operation are presented as status, not as inoperative controls. Adding scheduling, summary-depth, or appearance persistence is outside this UI redesign.

Controls size to their labels and values. The page must not be a grid of settings cards.

## Onboarding

First launch uses the same typography and control language as the main app in a focused single-column welcome flow. It preserves the existing single `Build My First Briefing` action, local-first and network-contact disclosures, and optional-AI contract. Source and model configuration remain available from their first-class destinations after the first briefing is initialized; onboarding does not invent scheduling or bridge behavior.

## State and Data Flow

The redesign preserves the existing architecture:

1. `AppModel` owns observable application, destination, selection, operation, and mutation state.
2. `BridgeClient` and `UniFFIBridgeClient` remain the macOS boundary to Rust.
3. The Rust core remains responsible for collection, storage, model profiles, credentials, summaries, and generation.
4. The CLI and macOS app continue to share compatible configuration and briefing data while the CLI remains independently usable on macOS, Linux, and Windows.

Required presentation changes:

- add `models` to `Destination` and update destination persistence;
- use `selectedStoryID` to drive inline expansion instead of a permanent detail pane;
- replace source and model sheet-presentation booleans with an explicit `InlineEditorRoute?` presentation value whose cases cover adding a source and adding a model profile;
- add the pure layout-mode policy without moving bridge logic into views;
- extract reusable expanded-story content from `StoryDetailView` rather than duplicating summary, source, save, and regeneration behavior.

## Error and Empty States

All states follow the same compact content language:

- Loading shows a small progress indicator and direct status copy in the content column.
- No briefing explains that Refresh will build one from enabled sources.
- No saved items explains how saving works without displaying a large illustration.
- Offline and refresh failures retain cached content when available and show an inline retry action.
- Source and model validation errors remain next to their inputs.
- Blocking local-data failure remains a focused full-content state.

Errors must identify what happened and the next valid action. Color is never the only status signal.

## Accessibility

The redesign must preserve and extend the existing accessibility policy:

- honor Reduce Transparency, Increase Contrast, Reduce Motion, light mode, and dark mode;
- retain distinct symbols and VoiceOver labels for status states;
- provide accessible labels and help for icon-only toolbar controls;
- preserve native tab order and keyboard focus;
- preserve Command-R, Command-O, Command-S, and Command-comma behavior;
- keep text readable at system text sizes without clipping;
- use standard controls so system accessibility substitutions work automatically.

When transparency is reduced, the sidebar becomes an opaque system surface with the same hierarchy and separators.

## Implementation Boundaries

Primary Swift files expected to change:

- `Views/ReadingWindowView.swift`
- `Views/TodayView.swift`
- `Views/StoryListView.swift`
- `Views/StoryRowView.swift`
- `Views/StoryDetailView.swift`
- `Views/SourcesView.swift`
- `Views/SourceEditorView.swift`
- `Views/ModelsSettingsView.swift`
- `Views/ModelProfileEditorView.swift`
- `Views/SettingsView.swift`
- `Views/WelcomeView.swift`
- `State/AppModel.swift`
- `Design/SignalGlass.swift`
- a new small adaptive-layout or shared-shell design file if it keeps responsibilities focused.

Rust crates, database migrations, FFI behavior, packaging behavior, and the menu-bar popover are outside the redesign. If the new Swift destination or presentation types require a compile-time adapter change at the existing FFI boundary, that adapter may change without altering the Rust contract or behavior.

## Testing Strategy

### Presentation and unit tests

Add or update tests for:

- layout modes at 559, 560, 819, and 820 points;
- the complete destination set including Models;
- first valid signal selection and one-at-a-time inline expansion;
- selection persistence and invalid-selection fallback;
- inline source and model editor presentation;
- refresh, retry, save, open-source, and settings commands;
- accessibility labels for compact icon navigation and toolbar controls;
- preservation of bridge-confirmed mutations and cached-content error behavior.

### Existing suites

Run the existing Swift presentation, model, reading-flow, source, model, accessibility, acceptance, and packaging tests. Run the Rust workspace tests because the macOS package links against the shared core, even though Rust behavior is not intended to change.

### Visual verification

Render and inspect the main destinations at these representative sizes:

- 1100 by 720, expanded sidebar;
- 760 by 640, icon rail;
- 480 by 620, compact navigation.

Repeat the checks in light and dark appearances and with Reduce Transparency and Increase Contrast enabled. Verify that no text, toolbar action, editor field, list row, or status message clips or overlaps.

## Acceptance Criteria

The redesign is complete when:

- the reading window can shrink to 420 by 520 points without horizontal clipping;
- no reading destination contains a nested split view or permanent detail pane;
- the same shell and visual language cover onboarding, reading, sources, models, and preferences;
- glass or material is limited to navigation and window chrome;
- ordinary content and controls are not wrapped in decorative cards;
- toolbar and row controls size to their content and remain keyboard accessible;
- only one signal is expanded at a time and all existing story actions still work;
- source and model creation works inline, while existing toggle, test, default-selection, and removal actions retain bridge-confirmed behavior;
- cached, loading, empty, offline, failure, and startup-failure states remain correct;
- the macOS test suite, Rust workspace tests, packaging checks, and visual-size verification pass;
- the cross-platform CLI remains unchanged and independently usable.
