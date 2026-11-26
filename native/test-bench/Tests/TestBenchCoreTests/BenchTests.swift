import Foundation
import Testing
@testable import TestBenchCore

let corpus = [
    "Thanks for your email, I will get back to you by the end of the week.",
    "Could we move the review meeting to Thursday afternoon at three?",
    "I have attached the latest report with the numbers from last quarter.",
]

@Suite("Oracle")
struct OracleTests {
    @Test func tabTakesTheNextWordAndTheRestWhenOnlySpaceRemains() {
        #expect(acceptWord(" back to you") == (" back", " to you"))
        #expect(expectedInsert(claim: "I will get back to", before: "I will get", key: .tab) == " back")
        #expect(expectedInsert(claim: "I will get back ", before: "I will get", key: .tab) == " back ")
        #expect(expectedInsert(claim: "I will get back to", before: "I will get", key: .shiftTab) == " back to")
        #expect(expectedInsert(claim: "I will get back", before: "I will go", key: .tab) == nil)
        #expect(expectedInsert(claim: "I will ", before: "I will", key: .tab) == nil)
    }

    @Test func smartQuotesDoNotCountAsDifferences() {
        #expect(normalized("it\u{2019}s \u{201C}ok\u{201D}") == "it's \"ok\"")
    }

    @Test func aChipThatShrinksWithTypingIsFine() {
        var t = ChipTracker()
        #expect(t.observe(text: "I will get", chip: " back to", atMs: 0) == nil)
        #expect(t.observe(text: "I will get ", chip: " back to", atMs: 10) == nil)
        #expect(t.observe(text: "I will get ", chip: "back to", atMs: 40) == nil)
        #expect(t.observe(text: "I will get b", chip: "ack to", atMs: 400) == nil)
        #expect(t.claim == "I will get back to")
    }

    @Test func aChipThatStaysAfterTheTextMovesOnIsStale() {
        var t = ChipTracker()
        _ = t.observe(text: "I will get", chip: " back", atMs: 0)
        #expect(t.observe(text: "I will gex", chip: " back", atMs: 100) == nil)
        #expect(t.observe(text: "I will gex", chip: " back", atMs: 799) == nil)
        #expect(t.observe(text: "I will gex", chip: " back", atMs: 800) == .stale(chip: " back", text: "I will gex"))
        #expect(t.observe(text: "I will gex", chip: " back", atMs: 900) == nil, "reported once")
    }

    @Test func aChipThatDoesNotShrinkIsNotFollowing() {
        var t = ChipTracker()
        _ = t.observe(text: "I will get", chip: " back", atMs: 0)
        _ = t.observe(text: "I will get b", chip: " back", atMs: 10)
        #expect(t.observe(text: "I will get b", chip: " back", atMs: 710) == .notFollowing(chip: " back", expected: "ack"))
    }

    @Test func theGuardStopsForAnyoneElse() {
        var s = GuardState(frontmostPID: 7, ownPID: 7, windowIsKey: true, editorHasFocus: true,
                           foreignInput: nil, elapsedMs: 0, limitMs: 1_000)
        #expect(stopReason(s) == nil)
        s.frontmostPID = 8
        #expect(stopReason(s)?.contains("another app") == true)
        s.frontmostPID = nil
        #expect(stopReason(s) != nil)
        s.frontmostPID = 7
        s.editorHasFocus = false
        #expect(stopReason(s)?.contains("lost focus") == true)
        s.editorHasFocus = true
        s.foreignInput = "mouse"
        #expect(stopReason(s)?.contains("mouse") == true)
        s.foreignInput = nil
        s.elapsedMs = 1_000
        #expect(stopReason(s) == "time is up")
        #expect(isOwnEvent(userData: benchEventMarker) && isOwnEvent(userData: bobEventMarker))
        #expect(!isOwnEvent(userData: 0))
    }
}

@Suite("Typist")
struct TypistTests {
    @Test func theSameSeedTypesTheSameSession() {
        func session(_ seed: UInt64) -> [Action] {
            var t = Typist(seed: seed, corpus: corpus)
            var text = ""
            var out: [Action] = []
            for _ in 0..<200 {
                let a = t.next(text: text, chip: nil)
                out.append(a)
                if case let .type(c) = a { text.append(c) }
                if case .backspace = a, !text.isEmpty { text.removeLast() }
            }
            return out
        }
        #expect(session(3) == session(3))
        #expect(session(3) != session(4))
    }

    @Test func itTypesCorpusTextAndFixesItsTypos() {
        var t = Typist(seed: 11, corpus: corpus)
        var text = ""
        for _ in 0..<400 {
            switch t.next(text: text, chip: nil) {
            case let .type(c): text.append(c)
            case .backspace: if !text.isEmpty { text.removeLast() }
            default: break
            }
        }
        let words = Set(corpus.joined(separator: " ").split(separator: " ").map(String.init))
        let typed = text.split(separator: " ").dropLast().map(String.init)
        #expect(!typed.isEmpty)
        #expect(typed.allSatisfy { words.contains($0) }, "\(text)")
    }

    @Test func itTypesAtAHumanSpeed() {
        var t = Typist(seed: 5, corpus: corpus)
        let delays = (0..<500).map { _ in t.delayMs(after: .type("a")) }
        let mean = Double(delays.reduce(0, +)) / Double(delays.count)
        #expect(mean > 150 && mean < 420, "\(mean) ms per key")
        #expect(delays.allSatisfy { $0 >= 45 })
    }
}

/// A fake world: an editor plus a BOB that behaves as specified (optionally with one bug), on a virtual clock.
final class FakeWorld: BenchIO {
    enum Bug { case none, insertTwice, staleAfterTyping }
    var clock = 0.0
    var editor = ""
    var chipText: String?
    var lastKeyMs = 0.0
    var dismissed = false
    let bug: Bug
    let source = corpus.joined(separator: " ")

    init(bug: Bug) { self.bug = bug }

    func nowMs() -> Double { clock }
    func sleep(ms: Int) { clock += Double(ms); tick() }
    func text() -> String { editor }
    func chip() -> String? { chipText }
    func reset(profile: EditorProfile) { editor = ""; chipText = nil }
    var stopAtMs: Double?
    func stopReason() -> String? { stopAtMs.flatMap { clock > $0 ? "time is up" : nil } }

    /// BOB suggests the corpus continuation 150 ms after the last key.
    func tick() {
        guard chipText == nil, !dismissed, clock - lastKeyMs >= 150, editor.count >= 6 else { return }
        let needle = String(editor.suffix(40))
        guard let r = source.range(of: needle) else { return }
        let rest = source[r.upperBound...]
        let words = rest.split(separator: " ", maxSplits: 3, omittingEmptySubsequences: false).prefix(3)
        let s = (rest.hasPrefix(" ") ? "" : "") + words.joined(separator: " ")
        chipText = s.isEmpty ? nil : s
    }

    func post(_ action: Action) -> String? {
        lastKeyMs = clock
        if case .escape = action { dismissed = true } else { dismissed = false }
        switch action {
        case let .type(c):
            editor.append(c)
            if let chip = chipText {
                if chip.first == c {
                    chipText = bug == .staleAfterTyping ? chip : String(chip.dropFirst())
                    if chipText?.isEmpty == true { chipText = nil }
                } else if bug != .staleAfterTyping {
                    chipText = nil
                }
            }
        case .backspace:
            if !editor.isEmpty { editor.removeLast() }
            chipText = nil
        case let .accept(key):
            guard let chip = chipText else {
                if key == .tab { editor.append("\t") }
                return nil
            }
            let insert = expectedInsert(claim: editor + chip, before: editor, key: key) ?? ""
            editor += insert
            if bug == .insertTwice { editor += insert }
            let rest = String(chip.dropFirst(insert.count))
            chipText = rest.trimmingCharacters(in: .whitespaces).isEmpty ? nil : rest
        case .escape:
            chipText = nil
        case .pause:
            break
        }
        return nil
    }
}

@Suite("Runner")
struct RunnerTests {
    @Test func aCorrectBOBProducesNoFindings() {
        let world = FakeWorld(bug: .none)
        let runner = Runner(io: world, seed: 1, corpus: corpus)
        runner.run(episodes: 12, stepsPerEpisode: 150, profiles: [.plain], seed: 1)
        #expect(runner.findings.isEmpty, "\(runner.findings.prefix(3))")
        #expect(runner.summary.suggestions > 20)
        #expect(runner.summary.acceptsOK > 5)
        #expect(runner.summary.keys > 1_000)
    }

    @Test func aStaleChipIsFound() {
        let world = FakeWorld(bug: .staleAfterTyping)
        let runner = Runner(io: world, seed: 2, corpus: corpus)
        runner.run(episodes: 6, stepsPerEpisode: 150, profiles: [.plain], seed: 2)
        let kinds = Set(runner.findings.map(\.kind))
        #expect(kinds.contains(.staleChip) || kinds.contains(.chipNotFollowing), "\(kinds)")
    }

    @Test func aDoubledAcceptIsFound() {
        let world = FakeWorld(bug: .insertTwice)
        let runner = Runner(io: world, seed: 4, corpus: corpus)
        runner.run(episodes: 6, stepsPerEpisode: 150, profiles: [.plain], seed: 4)
        let mismatch = runner.findings.first { $0.kind == .acceptMismatch }
        #expect(mismatch != nil)
        #expect(runner.findings.filter { $0.kind == .textMismatch }.isEmpty, "the accept is reported, not the text after it")
    }

    @Test func aStopReasonEndsTheRun() {
        let world = FakeWorld(bug: .none)
        world.stopAtMs = 2_000
        let runner = Runner(io: world, seed: 3, corpus: corpus)
        runner.run(episodes: 50, stepsPerEpisode: 500, profiles: [.plain], seed: 3)
        #expect(runner.summary.stoppedBecause == "time is up")
        #expect(world.clock < 3_000)
    }
}
