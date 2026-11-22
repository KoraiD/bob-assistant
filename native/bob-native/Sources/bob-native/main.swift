import BobNativeCore

// Task 1: placeholder entry point. Task 2 replaces this with the IPC run loop.
let args = CommandLine.arguments.dropFirst()
if args.contains("--version") {
    print(BobNativeInfo.banner())
} else {
    print("\(BobNativeInfo.name): native helper is not wired up yet (see .kiro/specs/ghost-text).")
}
