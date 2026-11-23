import AppKit
import CoreGraphics
import Foundation
import ScreenCaptureKit
import Vision

/// Orders and joins recognized text lines (pure, unit-tested).
public enum ScreenTextLayout {
    public static let defaultMaxChars = 4000

    /// A recognized line with its Vision bounding box (normalized, **bottom-left** origin).
    public struct Line: Equatable, Sendable {
        public var text: String
        public var box: CGRect
        public init(text: String, box: CGRect) {
            self.text = text
            self.box = box
        }
    }

    /// Reading order: top to bottom, then left to right among lines whose vertical centers are within
    /// half a line height of each other (same visual row). Blank lines are dropped and the result is
    /// capped at `maxChars` characters, cut at a line boundary when possible.
    public static func join(_ lines: [Line], maxChars: Int = defaultMaxChars) -> String {
        let kept = lines.filter { !$0.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
        let sorted = kept.sorted { a, b in
            let tolerance = max(min(a.box.height, b.box.height) / 2, 0.002)
            if abs(a.box.midY - b.box.midY) > tolerance { return a.box.midY > b.box.midY }
            return a.box.minX < b.box.minX
        }
        var out = ""
        for line in sorted {
            let piece = (out.isEmpty ? "" : "\n") + line.text
            if out.count + piece.count > maxChars {
                if out.isEmpty { out = String(line.text.prefix(maxChars)) }
                break
            }
            out += piece
        }
        return out
    }
}

/// When to capture (pure, unit-tested): never in Accessibility-only mode or for excluded apps; at once
/// when the app or window changes; otherwise at most every `minInterval` seconds while the user types.
public struct CapturePolicy: Sendable {
    public static let minInterval: TimeInterval = 8

    public var scope: ContextScope = .accessibility
    public var exclude: Set<String> = []
    public var lastKey: String?
    public var lastAt: Date = .distantPast

    public init() {}

    public func shouldCapture(appBundleId: String, windowTitle: String?, secure: Bool, now: Date) -> Bool {
        guard scope != .accessibility, !secure, !appBundleId.isEmpty, !exclude.contains(appBundleId) else { return false }
        let key = "\(appBundleId)\u{1}\(windowTitle ?? "")"
        return key != lastKey || now.timeIntervalSince(lastAt) >= Self.minInterval
    }

    public mutating func didCapture(appBundleId: String, windowTitle: String?, at now: Date) {
        lastKey = "\(appBundleId)\u{1}\(windowTitle ?? "")"
        lastAt = now
    }
}

/// Opt-in screen context (PC-4): one ScreenCaptureKit frame of the frontmost window or of every display,
/// then on-device Vision OCR. Needs the Screen Recording permission. Images never leave this process
/// and are never written to disk; only the recognized text is sent to the core.
/// State is only touched on the main thread (the capture task hops back with `MainActor.run`), hence
/// `@unchecked Sendable`.
public final class ScreenContext: @unchecked Sendable {
    /// BOB's own processes are never captured.
    static let ownBundleIds: Set<String> = ["dev.bob.assistant", "dev.bob.assistant.native"]
    /// Longest side of a captured image, in pixels: enough for OCR, bounded cost.
    static let maxPixels = 2800

    public private(set) var policy = CapturePolicy()
    private var inFlight = false
    /// Called on the main thread with (scope, app bundle id, text, milliseconds).
    public var onText: ((ContextScope, String, String, Int) -> Void)?

    public init() {}

    public func configure(scope: ContextScope, exclude: [String]) {
        policy.scope = scope
        policy.exclude = Set(exclude).union(Self.ownBundleIds)
        policy.lastKey = nil  // capture fresh at the next event
    }

    public static var permitted: Bool { CGPreflightScreenCaptureAccess() }

    /// Shows the system prompt (once per app; afterwards macOS only offers System Settings).
    @discardableResult
    public static func requestPermission() -> Bool { CGRequestScreenCaptureAccess() }

    /// Main thread. Starts a capture when the policy allows and none is running.
    public func maybeCapture(appBundleId: String, pid: pid_t, windowTitle: String?, secure: Bool) {
        let now = Date()
        guard !inFlight, policy.shouldCapture(appBundleId: appBundleId, windowTitle: windowTitle, secure: secure, now: now),
              Self.permitted else { return }
        policy.didCapture(appBundleId: appBundleId, windowTitle: windowTitle, at: now)
        inFlight = true
        let scope = policy.scope
        let exclude = policy.exclude
        Task.detached(priority: .utility) { [self] in  // lives as long as the helper
            let started = Date()
            let text = (try? await Self.capture(scope: scope, pid: pid, exclude: exclude)) ?? ""
            let ms = Int(Date().timeIntervalSince(started) * 1000)
            await MainActor.run {
                self.inFlight = false
                // Drop results for a scope the user has turned off meanwhile.
                guard self.policy.scope == scope else { return }
                self.onText?(scope, appBundleId, text, ms)
            }
        }
    }

    // MARK: capture + OCR (background)

    static func capture(scope: ContextScope, pid: pid_t, exclude: Set<String>) async throws -> String {
        let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
        var images: [CGImage] = []
        switch scope {
        case .accessibility:
            return ""
        case .activeWindow:
            guard let windowID = frontWindowID(pid: pid),
                  let window = content.windows.first(where: { $0.windowID == windowID }) else { return "" }
            let filter = SCContentFilter(desktopIndependentWindow: window)
            images.append(try await shot(filter))
        case .allScreens:
            let excluded = content.applications.filter { exclude.contains($0.bundleIdentifier) }
            for display in content.displays {
                let filter = SCContentFilter(display: display, excludingApplications: excluded, exceptingWindows: [])
                images.append(try await shot(filter))
            }
        }
        return images.map(recognize).filter { !$0.isEmpty }.joined(separator: "\n\n")
            .prefix(ScreenTextLayout.defaultMaxChars).description
    }

    /// The frontmost normal-layer window of `pid` (CGWindowList is ordered front to back).
    static func frontWindowID(pid: pid_t) -> CGWindowID? {
        guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
            as? [[String: Any]] else { return nil }
        let match = list.first {
            ($0[kCGWindowOwnerPID as String] as? Int).map(pid_t.init) == pid && ($0[kCGWindowLayer as String] as? Int) == 0
        }
        return (match?[kCGWindowNumber as String] as? Int).map(CGWindowID.init)
    }

    static func shot(_ filter: SCContentFilter) async throws -> CGImage {
        let config = SCStreamConfiguration()
        let scale = CGFloat(filter.pointPixelScale)
        var width = filter.contentRect.width * scale
        var height = filter.contentRect.height * scale
        let longest = max(width, height)
        if longest > CGFloat(maxPixels) {
            let k = CGFloat(maxPixels) / longest
            width *= k
            height *= k
        }
        config.width = max(Int(width), 1)
        config.height = max(Int(height), 1)
        config.showsCursor = false
        return try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config)
    }

    static func recognize(_ image: CGImage) -> String {
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = true
        request.automaticallyDetectsLanguage = true
        do {
            try VNImageRequestHandler(cgImage: image).perform([request])
        } catch {
            return ""
        }
        let lines = (request.results ?? []).compactMap { obs -> ScreenTextLayout.Line? in
            guard let top = obs.topCandidates(1).first else { return nil }
            return ScreenTextLayout.Line(text: top.string, box: obs.boundingBox)
        }
        return ScreenTextLayout.join(lines)
    }
}
