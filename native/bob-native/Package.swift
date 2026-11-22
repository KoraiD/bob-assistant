// swift-tools-version: 6.0
import PackageDescription

// bob-native: the macOS integration helper (Accessibility, event tap, overlays, mic).
// Logic lives in the BobNativeCore library so it is unit-testable; the executable is a thin shell.
let package = Package(
    name: "bob-native",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "BobNativeCore", targets: ["BobNativeCore"]),
        .executable(name: "bob-native", targets: ["bob-native"]),
    ],
    targets: [
        .target(name: "BobNativeCore"),
        .executableTarget(name: "bob-native", dependencies: ["BobNativeCore"]),
        .testTarget(name: "BobNativeCoreTests", dependencies: ["BobNativeCore"]),
    ]
)
