import Foundation
import Testing
@testable import BobNativeCore

@Suite("Stall")
struct StallTests {
    @Test func reportsOnlyOverTheThreshold() {
        let kind = "test-threshold-\(UUID().uuidString)"
        #expect(!Stall.shouldReport(kind, ms: 29, threshold: 30, now: Date()))
        #expect(Stall.shouldReport(kind, ms: 30, threshold: 30, now: Date()))
    }

    @Test func throttlesEachKindSeparately() {
        let a = "test-a-\(UUID().uuidString)", b = "test-b-\(UUID().uuidString)"
        let t0 = Date()
        #expect(Stall.shouldReport(a, ms: 100, threshold: 30, now: t0))
        #expect(!Stall.shouldReport(a, ms: 100, threshold: 30, now: t0.addingTimeInterval(1)))
        #expect(Stall.shouldReport(b, ms: 100, threshold: 30, now: t0.addingTimeInterval(1)))
        #expect(Stall.shouldReport(a, ms: 100, threshold: 30, now: t0.addingTimeInterval(Stall.interval)))
    }

    @Test func measureReturnsTheBodyResult() {
        #expect(Stall.measure("test-measure") { 42 } == 42)
    }

    /// A hung app must cost well under a second per Accessibility call (macOS default: about 6 s).
    @Test func accessibilityTimeoutIsShort() {
        #expect(AXReader.messagingTimeout > 0 && AXReader.messagingTimeout <= 0.5)
    }
}
