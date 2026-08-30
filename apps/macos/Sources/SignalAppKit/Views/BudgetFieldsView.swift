import SwiftUI

public struct BudgetFieldsDraft: Sendable, Equatable {
  public var dailyBudgetUSD: String
  public var inputCostUSDPerMillion: String
  public var outputCostUSDPerMillion: String

  public init(
    dailyBudgetUSD: String = "",
    inputCostUSDPerMillion: String = "",
    outputCostUSDPerMillion: String = ""
  ) {
    self.dailyBudgetUSD = dailyBudgetUSD
    self.inputCostUSDPerMillion = inputCostUSDPerMillion
    self.outputCostUSDPerMillion = outputCostUSDPerMillion
  }

  public var isEmpty: Bool {
    dailyBudgetUSD.isEmpty
      && inputCostUSDPerMillion.isEmpty
      && outputCostUSDPerMillion.isEmpty
  }

  public var isComplete: Bool {
    !dailyBudgetUSD.isEmpty
      && !inputCostUSDPerMillion.isEmpty
      && !outputCostUSDPerMillion.isEmpty
  }
}

public enum BudgetFieldsPresentation {
  public static let allTogetherMessage =
    "Enter the daily budget and both per-million rates, or leave all three blank."
  public static let exactParsingExplanation =
    "All three fields are optional together. Rust parses these USD values exactly; the app never uses floating-point cost math."
  public static let conservativeCapExplanation =
    "Conservative estimates can stop a provider call before the displayed daily cap is reached."
}

public struct BudgetFieldsView: View {
  @Binding private var draft: BudgetFieldsDraft

  public init(draft: Binding<BudgetFieldsDraft>) {
    _draft = draft
  }

  public var body: some View {
    Section {
      TextField("Daily budget (USD)", text: $draft.dailyBudgetUSD, prompt: Text("Optional"))
      TextField(
        "Input cost per million tokens (USD)",
        text: $draft.inputCostUSDPerMillion,
        prompt: Text("Optional")
      )
      TextField(
        "Output cost per million tokens (USD)",
        text: $draft.outputCostUSDPerMillion,
        prompt: Text("Optional")
      )
    } header: {
      Text("Budget")
    } footer: {
      VStack(alignment: .leading, spacing: 4) {
        Text(BudgetFieldsPresentation.exactParsingExplanation)
        Text(BudgetFieldsPresentation.conservativeCapExplanation)
      }
    }
  }
}
