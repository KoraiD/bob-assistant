// swift-tools-version: 6.0
import PackageDescription

// BOB Test Bench: a text editor that drives a test copy of BOB with real key events, only into itself.
// A development tool; never bundled into BOB.app. Logic lives in TestBenchCore so it is unit-testable.
// Swift language mode 5 for the same reason as bob-native (AppKit and C callbacks).
let package = Package(
    name: "test-bench",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "bob-test-bench", targets: ["bob-test-bench"]),
    ],
    targets: [
        .target(name: "TestBenchCore"),
        .executableTarget(name: "bob-test-bench", dependencies: ["TestBenchCore"]),
        .testTarget(name: "TestBenchCoreTests", dependencies: ["TestBenchCore"]),
    ],
    swiftLanguageModes: [.v5]
)
