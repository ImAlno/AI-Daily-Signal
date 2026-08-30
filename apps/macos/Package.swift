// swift-tools-version: 6.2
import Foundation
import PackageDescription

let packageRoot = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let repositoryRoot = packageRoot.deletingLastPathComponent().deletingLastPathComponent()
let rustReleaseDirectory = repositoryRoot.appendingPathComponent("target/release").path
let commandLineToolsFrameworks = "/Library/Developer/CommandLineTools/Library/Developer/Frameworks"
let commandLineToolsLibraries = "/Library/Developer/CommandLineTools/Library/Developer/usr/lib"
let testingSwiftSettings: [SwiftSetting] =
  FileManager.default.fileExists(
    atPath: commandLineToolsFrameworks + "/Testing.framework"
  ) ? [.unsafeFlags(["-F", commandLineToolsFrameworks])] : []
let testingLinkerSettings: [LinkerSetting] =
  FileManager.default.fileExists(
    atPath: commandLineToolsFrameworks + "/Testing.framework"
  )
  ? [
    .unsafeFlags([
      "-F", commandLineToolsFrameworks,
      "-Xlinker", "-rpath",
      "-Xlinker", commandLineToolsFrameworks,
      "-Xlinker", "-rpath",
      "-Xlinker", commandLineToolsLibraries,
    ])
  ] : []

let package = Package(
  name: "AIDailySignalMac",
  platforms: [.macOS(.v26)],
  products: [
    .library(name: "SignalAppKit", targets: ["SignalAppKit"]),
    .executable(name: "SignalMacApp", targets: ["SignalMacApp"]),
  ],
  targets: [
    .systemLibrary(name: "CSignalFFI", path: "Generated/CSignalFFI"),
    .target(
      name: "SignalFFIBindings",
      dependencies: ["CSignalFFI"],
      path: "Generated/Swift",
      linkerSettings: [
        .unsafeFlags([
          "-L", rustReleaseDirectory,
          "-lsignal_ffi",
          "-Xlinker", "-rpath",
          "-Xlinker", "@executable_path/../Frameworks",
        ])
      ]
    ),
    .target(name: "SignalAppKit", dependencies: ["SignalFFIBindings"]),
    .executableTarget(name: "SignalMacApp", dependencies: ["SignalAppKit"]),
    .testTarget(
      name: "SignalAppKitTests",
      dependencies: ["SignalAppKit"],
      swiftSettings: testingSwiftSettings,
      linkerSettings: testingLinkerSettings
    ),
  ]
)
