// swift-tools-version: 6.0
import Foundation
import PackageDescription

// Embed Info.plist into the executable (__TEXT,__info_plist) so the bare helper has a bundle identity and
// the NSMicrophoneUsageDescription macOS requires before microphone access.
let infoPlist = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
    .appendingPathComponent("Sources/bob-native/Info.plist").path

// bob-native: the macOS integration helper (Accessibility, event tap, overlays, mic).
// Logic lives in the BobNativeCore library so it is unit-testable; the executable is a thin shell.
//
// Swift language mode 5 is used on purpose: the Accessibility (C) and AppKit APIs rely on global
// C callbacks and main-thread conventions that Swift 6 strict concurrency cannot yet express without
// pervasive unsafe annotations. Pure logic stays Sendable-clean.
let package = Package(
    name: "bob-native",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "BobNativeCore", targets: ["BobNativeCore"]),
        .executable(name: "bob-native", targets: ["bob-native"]),
    ],
    targets: [
        .target(name: "BobNativeCore"),
        .executableTarget(
            name: "bob-native",
            dependencies: ["BobNativeCore"],
            exclude: ["Info.plist"],
            linkerSettings: [.unsafeFlags(["-Xlinker", "-sectcreate", "-Xlinker", "__TEXT", "-Xlinker", "__info_plist", "-Xlinker", infoPlist])]
        ),
        .testTarget(name: "BobNativeCoreTests", dependencies: ["BobNativeCore"]),
    ],
    swiftLanguageModes: [.v5]
)
