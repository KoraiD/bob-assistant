import Foundation

/// Hooks for the Test Bench. They exist only in a helper built with `-Xswiftc -DBOB_TEST_HOOKS`; release and
/// dev builds (`scripts/build-app.sh`) never read these variables.
public enum TestHooks {
    /// `BOB_TEST_AUDIO_FILE`: dictation plays this audio file instead of the microphone.
    public static var audioFile: URL? {
        #if BOB_TEST_HOOKS
        guard let path = ProcessInfo.processInfo.environment["BOB_TEST_AUDIO_FILE"], !path.isEmpty else {
            return nil
        }
        return URL(fileURLWithPath: path)
        #else
        return nil
        #endif
    }

    /// `BOB_TEST_MAIN_STALL_MS`: block the main thread that long every 2.5 s, to stand in for a slow app
    /// (docs/PERFORMANCE.md, "Typing never waits for BOB"). Compiled out of normal builds (code review SEC-02).
    public static var mainStallMs: Int? {
        #if BOB_TEST_HOOKS
        return stallMs(from: ProcessInfo.processInfo.environment)
        #else
        return nil
        #endif
    }

    /// A positive whole number of milliseconds, at most 10 s; anything else is off.
    static func stallMs(from environment: [String: String]) -> Int? {
        guard let ms = environment["BOB_TEST_MAIN_STALL_MS"].flatMap(Int.init), ms > 0, ms <= 10_000 else { return nil }
        return ms
    }

    /// Whether this helper was built with the test hooks.
    public static var enabled: Bool {
        #if BOB_TEST_HOOKS
        return true
        #else
        return false
        #endif
    }
}
