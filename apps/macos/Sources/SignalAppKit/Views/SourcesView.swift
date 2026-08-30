import Foundation
import SwiftUI

public struct SourceRowPresentation: Sendable, Equatable {
  public let displayHost: String
  public let originLabel: String
  public let canRemove: Bool
  public let requiresRemovalConfirmation: Bool

  public init(source: Source) {
    displayHost = URLComponents(string: source.feedURL)?.host ?? "Feed address unavailable"
    switch source.origin {
    case .standard:
      originLabel = "Standard source"
      canRemove = false
      requiresRemovalConfirmation = false
    case .personal:
      originLabel = "Personal source"
      canRemove = true
      requiresRemovalConfirmation = true
    }
  }
}

public struct SourcesView: View {
  @Bindable private var model: AppModel
  @State private var pendingRemoval: Source?

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    let sources = model.snapshot?.sources ?? []
    let standard = sources.filter { $0.origin == .standard }
    let personal = sources.filter { $0.origin == .personal }

    List {
      Section("Standard Sources") {
        if standard.isEmpty {
          sourceEmptyRow(
            "No standard sources",
            detail: "Build a briefing to initialize the standard source set."
          )
        } else {
          ForEach(standard) { source in
            sourceRow(source)
          }
        }
      }

      Section("Personal Sources") {
        if personal.isEmpty {
          sourceEmptyRow(
            "No personal sources",
            detail: "Add an RSS or Atom feed to include it in future briefings."
          )
        } else {
          ForEach(personal) { source in
            sourceRow(source)
          }
        }
      }
    }
    .listStyle(.inset)
    .toolbar {
      ToolbarItem(placement: .primaryAction) {
        Button("Add Source", systemImage: "plus") {
          model.isSourceEditorPresented = true
        }
        .keyboardShortcut("n", modifiers: [.command, .shift])
        .help("Add personal source (⇧⌘N)")
      }
    }
    .sheet(isPresented: $model.isSourceEditorPresented) {
      SourceEditorView(model: model)
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
    .padding(.vertical, 5)
    .accessibilityElement(children: .contain)
  }

  private func sourceInformation(
    _ source: Source,
    presentation: SourceRowPresentation
  ) -> some View {
    VStack(alignment: .leading, spacing: 4) {
      Text(source.name)
        .font(.headline)
        .lineLimit(2)
      Text("\(source.category) · \(presentation.displayHost)")
        .font(.subheadline)
        .foregroundStyle(.secondary)
        .lineLimit(2)
      Text(
        "Weight \(source.weight.formatted(.number.precision(.fractionLength(0...2)))) · \(presentation.originLabel)"
      )
      .font(.caption)
      .foregroundStyle(.secondary)
      .lineLimit(2)
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

      if presentation.canRemove {
        Button("Remove \(source.name)", systemImage: "trash", role: .destructive) {
          if presentation.requiresRemovalConfirmation {
            pendingRemoval = source
          }
        }
        .labelStyle(.iconOnly)
        .disabled(isBusy)
        .help("Remove personal source")
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
    .padding(.vertical, 6)
  }
}
