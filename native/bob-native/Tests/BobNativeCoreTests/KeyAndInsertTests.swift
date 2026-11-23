import CoreGraphics
import Testing
@testable import BobNativeCore

@Suite("KeyDecision")
struct KeyDecisionTests {
    @Test func swallowsTabShiftTabAndEscapeOnlyWhileCapturing() {
        #expect(KeyDecision.decide(keyCode: 48, flags: [], capturing: true) == .swallow(.tab))
        #expect(KeyDecision.decide(keyCode: 48, flags: .maskShift, capturing: true) == .swallow(.shiftTab))
        #expect(KeyDecision.decide(keyCode: 53, flags: [], capturing: true) == .swallow(.escape))
        // GT-13: no chip, no capture.
        for code: Int64 in [48, 53] {
            #expect(KeyDecision.decide(keyCode: code, flags: [], capturing: false) == .pass)
        }
    }

    @Test func neverStealsShortcutsOrOtherKeys() {
        for flags: CGEventFlags in [.maskCommand, .maskControl, .maskAlternate, [.maskCommand, .maskShift], .maskSecondaryFn] {
            #expect(KeyDecision.decide(keyCode: 48, flags: flags, capturing: true) == .pass, "Cmd/Ctrl/Option+Tab must pass")
            #expect(KeyDecision.decide(keyCode: 53, flags: flags, capturing: true) == .pass)
        }
        #expect(KeyDecision.decide(keyCode: 53, flags: .maskShift, capturing: true) == .pass)
        #expect(KeyDecision.decide(keyCode: 48, flags: [], capturing: true, synthetic: true) == .pass, "BOB's own events pass")
    }

    /// Randomized property: only keycodes 48 and 53 can ever be swallowed, and never with a blocking modifier.
    @Test func onlyTabAndEscapeCanBeSwallowed() {
        var rng = SeededGenerator(seed: 7)
        let mods: [CGEventFlags] = [.maskShift, .maskCommand, .maskControl, .maskAlternate, .maskSecondaryFn, .maskAlphaShift]
        for _ in 0..<2000 {
            let code = Int64.random(in: 0...127, using: &rng)
            var flags: CGEventFlags = []
            for m in mods where Bool.random(using: &rng) { flags.insert(m) }
            let d = KeyDecision.decide(keyCode: code, flags: flags, capturing: Bool.random(using: &rng))
            if case .swallow = d {
                #expect(code == 48 || code == 53)
                #expect(flags.intersection([.maskCommand, .maskControl, .maskAlternate, .maskSecondaryFn]).isEmpty)
            }
        }
    }
}

@Suite("TextInserter.chunks")
struct TextInserterTests {
    @Test func splitsLongTextIntoBoundedChunks() {
        let c = TextInserter.chunks(String(repeating: "a", count: 40))
        #expect(c.map(\.count) == [16, 16, 8])
    }

    @Test func neverSplitsACharacter() {
        let text = "a😀b👨‍👩‍👧c" + String(repeating: "é", count: 20)
        for limit in 1...20 {
            let chunks = TextInserter.chunks(text, limit: limit)
            let rebuilt = chunks.map { String(decoding: $0, as: UTF16.self) }.joined()
            #expect(rebuilt == text)
            // Each chunk decodes on its own (no broken surrogate pairs).
            for chunk in chunks { #expect(!String(decoding: chunk, as: UTF16.self).contains("\u{FFFD}")) }
        }
    }

    @Test func emptyTextHasNoChunks() {
        #expect(TextInserter.chunks("").isEmpty)
    }
}
