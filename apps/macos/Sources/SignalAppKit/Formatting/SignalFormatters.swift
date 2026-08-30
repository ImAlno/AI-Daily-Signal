import Foundation

public enum SignalFormatters {
  public static func bridgeDate(_ value: String?) -> Date? {
    guard let value else { return nil }
    if let fractional = try? Date(
      value,
      strategy: Date.ISO8601FormatStyle(includingFractionalSeconds: true)
    ) {
      return fractional
    }
    return try? Date(value, strategy: Date.ISO8601FormatStyle())
  }

  public static func relativeDate(_ date: Date?, relativeTo reference: Date = .now) -> String {
    guard let date else { return "Unknown date" }
    let formatter = RelativeDateTimeFormatter()
    formatter.unitsStyle = .short
    return formatter.localizedString(for: date, relativeTo: reference)
  }
}
