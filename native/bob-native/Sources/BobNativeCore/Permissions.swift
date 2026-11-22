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
}
