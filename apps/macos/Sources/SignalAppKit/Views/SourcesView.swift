import Foundation
import SwiftUI

public enum SourceSettingsAction: Sendable, Equatable {
  case toggleEnabled
  case remove
}

public struct SourceRowPresentation: Sendable, Equatable {
  public let displayHost: String
  public let originLabel: String
  public let secondaryText: String
  public let tertiaryText: String
  public let directActions: [SourceSettingsAction]
  public let overflowActions: [SourceSettingsAction]
  public let requiresRemovalConfirmation: Bool

  public init(source: Source) {
    displayHost = URLComponents(string: source.feedURL)?.host ?? "Feed address unavailable"
    secondaryText = "\(source.category) · \(displayHost)"
    switch source.origin {
    case .standard:
      originLabel = "Standard source"
      directActions = [.toggleEnabled]
      overflowActions = []
      requiresRemovalConfirmation = false
    case .personal:
      originLabel = "Personal source"
      directActions = [.toggleEnabled]
      overflowActions = [.remove]
      requiresRemovalConfirmation = true
    }
    tertiaryText =
      "Weight \(source.weight.formatted(.number.locale(Locale(identifier: "en_US_POSIX")).precision(.fractionLength(0...2)))) · \(originLabel)"
  }
}

public struct SourceRowTextPresentation: Sendable, Equatable {
  public let nameLineLimit: Int?
  public let metadataLineLimit: Int?

  public init(dynamicTypeSize: DynamicTypeSize) {
    if dynamicTypeSize.isAccessibilitySize {
      nameLineLimit = nil
      metadataLineLimit = nil
    } else {
      nameLineLimit = 2
      metadataLineLimit = 2
    }
  }
}

public struct SourcesView: View {
  @Bindable private var model: AppModel
  @State private var pendingRemoval: Source?
  @Environment(\.dynamicTypeSize) private var dynamicTypeSize
  @Environment(\.appLayoutMode) private var layoutMode

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    let sources = model.snapshot?.sources ?? []
    let standard = sources.filter { $0.origin == .standard }
    let personal = sources.filter { $0.origin == .personal }

    VStack(spacing: 0) {
      if model.inlineEditorRoute == .addSource {
        ScrollView {
          SourceEditorView(model: model)
            .padding(.horizontal, SettingsGridMetrics.horizontalPadding(for: layoutMode))
            .padding(.vertical, SettingsGridMetrics.verticalPadding)
            .frame(maxWidth: .infinity, alignment: .top)
        }
      } else {
        ScrollView {
          LazyVStack(alignment: .leading, spacing: 0) {
            SettingsPageHeaderView(
              title: "Sources",
              message: "Choose which feeds contribute to future briefings."
            )
            .padding(.bottom, SettingsGridMetrics.headerBottomSpacing)

            settingsSectionLabel("Standard Sources")
            if standard.isEmpty {
              sourceEmptyRow(
                "No standard sources",
                detail: "Build a briefing to initialize the standard source set."
              )
            } else {
              ForEach(standard) { source in
                sourceRow(source)
                Divider()
              }
            }

            settingsSectionLabel("Personal Sources")
            if personal.isEmpty {
              sourceEmptyRow(
                "No personal sources",
                detail: "Add an RSS or Atom feed to include it in future briefings."
              )
            } else {
              ForEach(personal) { source in
                sourceRow(source)
                Divider()
              }
            }
          }
          .frame(maxWidth: SettingsGridMetrics.maximumWidth, alignment: .leading)
          .padding(.horizontal, SettingsGridMetrics.horizontalPadding(for: layoutMode))
          .padding(.vertical, SettingsGridMetrics.verticalPadding)
          .frame(maxWidth: .infinity, alignment: .center)
        }
      }
    }
    .background(Color(nsColor: .textBackgroundColor))
    .toolbar {
      ToolbarItem(placement: .primaryAction) {
        Button("Add Source", systemImage: "plus") {
          model.presentSourceEditor()
        }
        .keyboardShortcut("n", modifiers: [.command, .shift])
        .disabled(
          model.inlineEditorRoute == .addSource
            || model.sourceActionState(for: .adding) != nil
        )
        .help("Add personal source (⇧⌘N)")
      }
    }
    .confirmationDialog(
      pendingRemoval.map { "Remove \($0.name)?" } ?? "Remove personal source?",
      isPresented: removalConfirmation,
      titleVisibility: .visible
    ) {
      if let source = pendingRemoval {
        Button("Remove Source", role: .destructive) {
          pendingRemoval = nil
          Task { await model.removePersonalSource(id: source.id) }
        }
      }
      Button("Cancel", role: .cancel) {
        pendingRemoval = nil
      }
    } message: {
      Text("This feed will no longer be included in future briefings.")
    }
  }

  private var removalConfirmation: Binding<Bool> {
    Binding(
      get: { pendingRemoval != nil },
      set: { presented in
        if !presented { pendingRemoval = nil }
      }
    )
  }

  @ViewBuilder
  private func sourceRow(_ source: Source) -> some View {
    let presentation = SourceRowPresentation(source: source)
    let isBusy = sourceIsBusy(source.id)

    ViewThatFits(in: .horizontal) {
      HStack(alignment: .center, spacing: 16) {
        sourceInformation(source, presentation: presentation)
        Spacer(minLength: 8)
        sourceControls(source, presentation: presentation, isBusy: isBusy)
      }
      VStack(alignment: .leading, spacing: 10) {
        sourceInformation(source, presentation: presentation)
        sourceControls(source, presentation: presentation, isBusy: isBusy)
          .frame(maxWidth: .infinity, alignment: .trailing)
      }
    }
    .padding(.vertical, SettingsGridMetrics.rowVerticalPadding)
    .accessibilityElement(children: .contain)
  }

  private func settingsSectionLabel(_ title: String) -> some View {
    Text(title)
      .font(.caption.weight(.medium))
      .foregroundStyle(.secondary)
      .padding(.top, SettingsGridMetrics.sectionSpacing)
      .padding(.bottom, 5)
      .accessibilityAddTraits(.isHeader)
  }

  private func sourceInformation(
    _ source: Source,
    presentation: SourceRowPresentation
  ) -> some View {
    let textPresentation = SourceRowTextPresentation(dynamicTypeSize: dynamicTypeSize)
    return VStack(alignment: .leading, spacing: 4) {
      Text(source.name)
        .font(.headline)
        .lineLimit(textPresentation.nameLineLimit)
      Text(presentation.secondaryText)
        .font(.subheadline)
        .foregroundStyle(.secondary)
        .lineLimit(textPresentation.metadataLineLimit)
      Text(presentation.tertiaryText)
        .font(.caption)
        .foregroundStyle(.secondary)
        .lineLimit(textPresentation.metadataLineLimit)
      if let error = model.sourceActionError(for: source.id) {
        Label(error, systemImage: "exclamationmark.circle")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }

  private func sourceControls(
    _ source: Source,
    presentation: SourceRowPresentation,
    isBusy: Bool
  ) -> some View {
    HStack(spacing: 12) {
      if isBusy {
        ProgressView()
          .controlSize(.small)
          .accessibilityLabel("Updating \(source.name)")
      }

      if presentation.directActions.contains(.toggleEnabled) {
        Toggle(
          source.enabled ? "Disable \(source.name)" : "Enable \(source.name)",
          isOn: Binding(
            get: { source.enabled },
            set: { enabled in
              Task { await model.setSourceEnabled(id: source.id, enabled: enabled) }
            }
          )
        )
        .labelsHidden()
        .disabled(isBusy)
        .help(source.enabled ? "Disable this source" : "Enable this source")
      }

      if !presentation.overflowActions.isEmpty {
        Menu {
          if presentation.overflowActions.contains(.remove) {
            Button("Remove Source", role: .destructive) {
              if presentation.requiresRemovalConfirmation {
                pendingRemoval = source
              }
            }
            .accessibilityLabel("Remove \(source.name)")
            .accessibilityHint(IconControlDescriptor.removeSource.help)
          }
        } label: {
          Image(systemName: "ellipsis.circle")
        }
        .disabled(isBusy)
        .frame(
          minWidth: VisualPolicy().minimumControlDimension,
          minHeight: VisualPolicy().minimumControlDimension
        )
        .accessibilityLabel("More actions for \(source.name)")
        .accessibilityHint("Manage this source")
        .help(IconControlDescriptor.removeSource.help)
      }
    }
  }

  private func sourceIsBusy(_ sourceID: String) -> Bool {
    model.sourceActionState(for: .toggling(sourceID: sourceID)) != nil
      || model.sourceActionState(for: .removing(sourceID: sourceID)) != nil
  }

  private func sourceEmptyRow(_ title: String, detail: String) -> some View {
    Label {
      VStack(alignment: .leading, spacing: 3) {
        Text(title)
        Text(detail)
          .font(.caption)
          .foregroundStyle(.secondary)
      }
    } icon: {
      Image(systemName: "dot.radiowaves.left.and.right")
        .foregroundStyle(.secondary)
    }
    .padding(.vertical, SettingsGridMetrics.rowVerticalPadding)
  }
}
