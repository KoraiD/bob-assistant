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

    /// Whether this helper was built with the test hooks.
    public static var enabled: Bool {
        #if BOB_TEST_HOOKS
        return true
        #else
        return false
        #endif
    }
}
