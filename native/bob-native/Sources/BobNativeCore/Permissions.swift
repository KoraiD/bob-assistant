import ApplicationServices
import AVFoundation
import Foundation

/// macOS permission checks (requirement NB-6).
public enum Permissions {
    /// Whether this process is trusted for Accessibility. With `prompt: true` macOS shows its dialog.
    public static func accessibilityTrusted(prompt: Bool = false) -> Bool {
        let key = kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String
        return AXIsProcessTrustedWithOptions([key: prompt] as CFDictionary)
    }

    public static func microphoneStatus() -> MicStatus {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: return .granted
        case .denied, .restricted: return .denied
        case .notDetermined: return .undetermined
        @unknown default: return .undetermined
        }
    }

    /// Ask for microphone access. macOS terminates a process that requests it without an
    /// `NSMicrophoneUsageDescription`, so this is a no-op unless the helper runs inside a bundle that
    /// declares one (the packaged app does; a bare `swift run` binary does not). Returns whether a
    /// request was actually issued.
    @discardableResult
    public static func requestMicrophoneIfPossible(_ completion: @escaping (Bool) -> Void = { _ in }) -> Bool {
        guard Bundle.main.object(forInfoDictionaryKey: "NSMicrophoneUsageDescription") != nil else {
            return false
        }
        AVCaptureDevice.requestAccess(for: .audio, completionHandler: completion)
        return true
    }

    /// Deep link to the relevant System Settings pane.
    public static func settingsURL(for pane: Pane) -> URL {
        switch pane {
        case .accessibility:
            return URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
        case .microphone:
            return URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")!
        }
    }

    public enum Pane { case accessibility, microphone }

    /// Friendly name of the app hosting this terminal, from `$TERM_PROGRAM`. macOS attributes a
    /// command-line tool's Accessibility trust to that host app, not to the tool itself.
    public static func hostAppName(termProgram: String?) -> String {
        switch termProgram {
        case "iTerm.app": return "iTerm"
        case "Apple_Terminal": return "Terminal"
        case "kiro": return "Kiro"
        case "vscode": return "Visual Studio Code (or Kiro)"
        case "WarpTerminal": return "Warp"
        case "ghostty": return "Ghostty"
        case "WezTerm": return "WezTerm"
        case "Alacritty": return "Alacritty"
        case "Hyper": return "Hyper"
        default: return "the app that hosts this terminal"
        }
    }

    /// Step-by-step help printed when a standalone mode (`--probe`) is not trusted.
    public static func accessibilityHelp(termProgram: String?) -> String {
        let app = hostAppName(termProgram: termProgram)
        return """
        Accessibility is not granted to \(app).
        macOS gives command-line tools the permission of the app that runs them, so grant it to \(app), not to bob-native:
          1. System Settings > Privacy & Security > Accessibility (opening it now)
          2. Click "+", pick \(app) from /Applications, and switch it ON (if it is already listed, remove it with "-" and add it again)
          3. Quit \(app) completely (Cmd+Q) and open it again, then re-run the command
        """
    }
}
