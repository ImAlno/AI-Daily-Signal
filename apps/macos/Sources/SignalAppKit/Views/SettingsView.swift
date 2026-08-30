import SwiftUI

public struct SettingsView: View {
  private let model: AppModel

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    ModelsSettingsView(model: model)
  }
}
