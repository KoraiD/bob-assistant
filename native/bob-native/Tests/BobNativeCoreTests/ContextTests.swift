import CoreGraphics
import Foundation
import Testing
@testable import BobNativeCore

@Suite("TextWindow.afterCaret")
struct AfterCaretTests {
    @Test func returnsTheTextAfterTheCaretOrSelection() {
        #expect(TextWindow.afterCaret(value: "Hello world", caretUTF16: 5) == " world")
        #expect(TextWindow.afterCaret(value: "Hello world", caretUTF16: 0, selectionUTF16: 6) == "world")
    }

    @Test func clampsAndBounds() {
        #expect(TextWindow.afterCaret(value: "abc", caretUTF16: 99) == "")
        #expect(TextWindow.afterCaret(value: "abc", caretUTF16: -3) == "abc")
        #expect(TextWindow.afterCaret(value: "0123456789", caretUTF16: 2, maxChars: 3) == "234")
        #expect(TextWindow.afterCaret(value: "abc", caretUTF16: 0, maxChars: 0) == "")
    }

    @Test func neverSplitsAGraphemeCluster() {
        let family = "👨‍👩‍👧"
        #expect(TextWindow.afterCaret(value: "x" + family + "y", caretUTF16: 1, maxChars: 1) == family)
    }
}

@Suite("ScreenTextLayout")
struct ScreenTextLayoutTests {
    typealias Line = ScreenTextLayout.Line

    @Test func readsTopToBottomThenLeftToRight() {
        // Vision boxes use a bottom-left origin: larger y is higher on screen.
        let lines = [
            Line(text: "second row right", box: CGRect(x: 0.6, y: 0.50, width: 0.3, height: 0.04)),
            Line(text: "first row", box: CGRect(x: 0.1, y: 0.80, width: 0.3, height: 0.04)),
            Line(text: "second row left", box: CGRect(x: 0.1, y: 0.505, width: 0.3, height: 0.04)),
            Line(text: "   ", box: CGRect(x: 0.1, y: 0.2, width: 0.3, height: 0.04)),
        ]
        #expect(ScreenTextLayout.join(lines) == "first row\nsecond row left\nsecond row right")
    }

    @Test func capsAtALineBoundary() {
        let lines = (0..<10).map { i in
            Line(text: "line \(i)", box: CGRect(x: 0, y: 0.9 - Double(i) * 0.05, width: 0.2, height: 0.02))
        }
        let out = ScreenTextLayout.join(lines, maxChars: 20)
        #expect(out == "line 0\nline 1\nline 2")
        #expect(ScreenTextLayout.join([Line(text: String(repeating: "x", count: 50), box: .zero)], maxChars: 8) == "xxxxxxxx")
    }

    @Test func resultNeverExceedsTheLimitForRandomInput() {
        var rng = SystemRandomNumberGenerator()
        for _ in 0..<200 {
            let n = Int.random(in: 0..<30, using: &rng)
            let lines = (0..<n).map { _ in
                Line(text: String(repeating: "a", count: Int.random(in: 0..<40, using: &rng)),
                     box: CGRect(x: .random(in: 0..<1, using: &rng), y: .random(in: 0..<1, using: &rng), width: 0.1, height: 0.02))
            }
            let limit = Int.random(in: 1..<200, using: &rng)
            #expect(ScreenTextLayout.join(lines, maxChars: limit).count <= limit)
        }
    }
}

@Suite("CapturePolicy")
struct CapturePolicyTests {
    let t0 = Date(timeIntervalSince1970: 1_000_000)

    @Test func accessibilityOnlyNeverCaptures() {
        let p = CapturePolicy()
        #expect(!p.shouldCapture(appBundleId: "com.apple.mail", windowTitle: "Inbox", secure: false, now: t0))
    }

    @Test func capturesOnChangeAndThenThrottles() {
        var p = CapturePolicy()
        p.scope = .activeWindow
        #expect(p.shouldCapture(appBundleId: "com.apple.mail", windowTitle: "Inbox", secure: false, now: t0))
        p.didCapture(appBundleId: "com.apple.mail", windowTitle: "Inbox", at: t0)
        #expect(!p.shouldCapture(appBundleId: "com.apple.mail", windowTitle: "Inbox", secure: false, now: t0 + 1))
        #expect(p.shouldCapture(appBundleId: "com.apple.mail", windowTitle: "Re: Dinner", secure: false, now: t0 + 1))
        #expect(p.shouldCapture(appBundleId: "com.apple.mail", windowTitle: "Inbox", secure: false,
                                now: t0 + CapturePolicy.minInterval))
    }

    @Test func neverCapturesSecureFieldsOrExcludedApps() {
        var p = CapturePolicy()
        p.scope = .allScreens
        p.exclude = ["com.1password.1password"]
        #expect(!p.shouldCapture(appBundleId: "com.apple.mail", windowTitle: nil, secure: true, now: t0))
        #expect(!p.shouldCapture(appBundleId: "com.1password.1password", windowTitle: nil, secure: false, now: t0))
        #expect(!p.shouldCapture(appBundleId: "", windowTitle: nil, secure: false, now: t0))
    }

    @Test func ownAppsAreAlwaysExcluded() {
        let s = ScreenContext()
        s.configure(scope: .allScreens, exclude: [])
        #expect(s.policy.exclude.contains("dev.bob.assistant"))
        #expect(s.policy.exclude.contains("dev.bob.assistant.native"))
    }
}

/// Code review GT-12: an app whose Accessibility was not ready when it came to the front is tried again.
@Suite("AttachRetry")
struct AttachRetryTests {
    @Test func retriesUntilBothTheAppAndAFieldAreObserved() {
        #expect(AttachRetry.needed(appSubscribed: false, elementObserved: true))
        #expect(AttachRetry.needed(appSubscribed: true, elementObserved: false))
        #expect(!AttachRetry.needed(appSubscribed: true, elementObserved: true))
    }
    @Test func delaysGrowAndStopWithinTenSeconds() {
        let d = AttachRetry.delaysMs
        #expect(zip(d, d.dropFirst()).allSatisfy { $0 < $1 })
        #expect(d.reduce(0, +) < 10_000)
    }
    @Test func alreadyRegisteredCountsAsSubscribed() {
        #expect(AttachRetry.subscribed(.success))
        #expect(AttachRetry.subscribed(.notificationAlreadyRegistered))
        #expect(!AttachRetry.subscribed(.cannotComplete))
        #expect(!AttachRetry.subscribed(.apiDisabled))
    }
}
