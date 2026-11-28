import CoreGraphics
import Foundation
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

/// Code review SEC-09: the last check before typing fails closed.
@Suite("InsertGuard")
struct InsertGuardTests {
    @Test func typesOnlyIntoAReadableNonSecureFieldWithSecureInputOff() {
        #expect(InsertGuard.decide(secureEventInput: false, field: .readable(secure: false)) == nil)
        #expect(InsertGuard.decide(secureEventInput: false, field: .readable(secure: true)) == .secureField)
        #expect(InsertGuard.decide(secureEventInput: false, field: .unreadable) == .unreadable)
        for field: FocusedField in [.readable(secure: false), .readable(secure: true), .unreadable] {
            #expect(InsertGuard.decide(secureEventInput: true, field: field) == .secureInput)
        }
    }
    @Test func refusalsUseTheProtocolReasons() throws {
        let reasons = [InsertRefusal.secureInput, .secureField, .unreadable].map(\.rawValue)
        #expect(reasons == ["secure_input", "secure_field", "unreadable"])
        let line = String(decoding: try JSONEncoder().encode(NativeEvent.insertRefused(reason: "unreadable")), as: UTF8.self)
        #expect(line.contains("\"type\":\"insert_refused\""))
    }
}

/// Code review GT-01: what a key that reaches the app does, for the core's expected text.
@Suite("EditDecision")
struct EditDecisionTests {
    @Test func typedTextBackspaceAndCaretKeys() {
        #expect(EditDecision.classify(keyCode: 16, flags: [], text: "y")! == (.type, "y"))
        #expect(EditDecision.classify(keyCode: 16, flags: .maskShift, text: "Y")! == (.type, "Y"))
        #expect(EditDecision.classify(keyCode: 51, flags: [], text: "\u{8}")! == (.backspace, nil))
        #expect(EditDecision.classify(keyCode: 51, flags: .maskAlternate, text: "")! == (.other, nil))
        for code: Int64 in [123, 124, 125, 126, 115, 119, 117, 36] {
            #expect(EditDecision.classify(keyCode: code, flags: [], text: "")! == (.other, nil), "\(code)")
        }
        #expect(EditDecision.classify(keyCode: 9, flags: .maskCommand, text: "v")! == (.other, nil), "Cmd+V")
        #expect(EditDecision.classify(keyCode: 32, flags: [], text: "") == nil, "a dead key types nothing yet")
        #expect(EditDecision.classify(keyCode: 122, flags: [], text: "\u{F704}") == nil, "F1")
    }
    @Test func editEventsAndPostKeyRoundTrip() throws {
        let ev = NativeEvent.edit(kind: .type, text: "y")
        let back = try JSONDecoder().decode(NativeEvent.self, from: JSONEncoder().encode(ev))
        #expect(back == ev)
        let cmd = try JSONDecoder().decode(CoreCommand.self, from: Data(#"{"type":"post_key","key":"tab"}"#.utf8))
        #expect(cmd == .postKey(.tab))
    }
}

/// Code review GT-09: a typo fix never runs after the user typed on.
@Suite("TypoFixGuard")
struct TypoFixGuardTests {
    @Test func aKeyAfterTheTabCancelsTheFix() {
        #expect(TypoFixGuard.unchanged(keysAtTab: 41, keysNow: 41))
        #expect(!TypoFixGuard.unchanged(keysAtTab: 41, keysNow: 42))
    }
}
