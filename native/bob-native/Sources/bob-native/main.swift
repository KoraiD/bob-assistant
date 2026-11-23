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

//   bob-native --transcribe <audio> --model <ggml.bin> [--lang en] [--audio-ctx N] [--runs N]
// Measurement tool for dictation models (docs/PERFORMANCE.md). Prints JSON lines with timings and text.
if let i = args.firstIndex(of: "--transcribe"), args.indices.contains(i + 1) {
    func value(_ flag: String) -> String? {
        args.firstIndex(of: flag).flatMap { args.indices.contains($0 + 1) ? args[$0 + 1] : nil }
    }
    guard let model = value("--model") else { print("--model is required"); exit(64) }
    let samples = try DictationAudio.load(url: URL(fileURLWithPath: args[i + 1]))
    let transcriber = Transcriber()
    let loadStart = Date()
    let done = DispatchSemaphore(value: 0)
    transcriber.setModel(path: model) { failure in
        if let failure { print("load failed: \(failure)"); exit(1) }
        done.signal()
    }
    done.wait()
    let loadMs = Int(Date().timeIntervalSince(loadStart) * 1000)
    let ctx = Int32(value("--audio-ctx") ?? "0") ?? 0
    let runs = Int(value("--runs") ?? "3") ?? 3
    for run in 0..<runs {
        let finished = DispatchSemaphore(value: 0)
        transcriber.transcribe(samples, language: value("--lang"), audioCtx: ctx) { result in
            switch result {
            case let .success(r):
                let json: [String: Any] = ["run": run, "audio_ms": samples.count / 16, "load_ms": loadMs,
                                           "transcribe_ms": r.ms, "language": r.language,
                                           "no_speech": Double(r.noSpeechProb), "speech": SpeechGate.hasSpeech(samples),
                                           "text": r.text]
                let data = try! JSONSerialization.data(withJSONObject: json, options: [.sortedKeys])
                print(String(data: data, encoding: .utf8)!)
            case let .failure(f): print("failed: \(f)")
            }
            finished.signal()
        }
        finished.wait()
    }
    transcriber.shutdown()  // free Metal buffers before exit (ggml asserts otherwise)
    exit(0)
}

guard let socketPath = ProcessInfo.processInfo.environment["BOB_SOCKET"] else {
    print("bob-native: BOB_SOCKET is not set. Run the BOB app, or use --probe / --chip-demo / --version.")
    exit(64)
}
HelperRuntime(mode: .connect(socketPath: socketPath)).run()
