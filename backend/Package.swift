// swift-tools-version: 6.0
import Foundation
import PackageDescription

let repository = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
  .deletingLastPathComponent().path
let rustProfile = ProcessInfo.processInfo.environment["MUSIC_BUILD_PROFILE"] ?? "debug"

let package = Package(
  name: "MusicBackend",
  platforms: [.macOS(.v14)],
  products: [
    .library(name: "MusicContract", targets: ["MusicContract"]),
    .executable(name: "muse", targets: ["MusicHost"]),
  ],
  targets: [
    .target(name: "MusicContract"),
    .target(name: "Backend", dependencies: ["MusicContract"]),
    .testTarget(name: "BackendTests", dependencies: ["Backend"]),
    .target(name: "HostTransport", dependencies: ["MusicContract"]),
    .systemLibrary(name: "CMuse"),
    .executableTarget(
      name: "MusicHost", dependencies: ["Backend", "MusicContract", "HostTransport", "CMuse"],
      linkerSettings: [
        .linkedLibrary("music_frontend"), .linkedLibrary("iconv"),
        .unsafeFlags([
          "-L", repository + "/frontend/target/" + rustProfile,
          "-Xlinker", "-sectcreate", "-Xlinker", "__TEXT", "-Xlinker", "__info_plist",
          "-Xlinker", repository + "/backend/.build/Info.plist",
        ]),
      ]),
    .testTarget(name: "MusicHostTests", dependencies: ["MusicHost"]),
    .testTarget(name: "HostTransportTests", dependencies: ["HostTransport"]),
    .testTarget(name: "MusicContractTests", dependencies: ["MusicContract"]),
  ]
)
