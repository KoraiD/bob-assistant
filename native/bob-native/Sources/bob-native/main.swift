import BobNativeCore
import Foundation

// bob-native entry point.
//   bob-native                  connect to the core at $BOB_SOCKET (started by the app)
//   bob-native --version        print the protocol version
//   bob-native --probe [secs]   compatibility spike: observe the focused field, print findings
//   bob-native --chip-demo      show a ghost chip at the caret in any app

let args = Array(CommandLine.arguments.dropFirst())

if args.contains("--version") {
    print(BobNativeInfo.banner())
    exit(0)
}

if let i = args.firstIndex(of: "--probe") {
    let seconds = args.indices.contains(i + 1) ? Double(args[i + 1]) ?? 15 : 15
    HelperRuntime(mode: .probe(seconds: seconds)).run()
}

if args.contains("--chip-demo") {
    HelperRuntime(mode: .chipDemo).run()
}

guard let socketPath = ProcessInfo.processInfo.environment["BOB_SOCKET"] else {
    print("bob-native: BOB_SOCKET is not set. Run the BOB app, or use --probe / --chip-demo / --version.")
    exit(64)
}
HelperRuntime(mode: .connect(socketPath: socketPath)).run()
