import Foundation

/// An editor profile: which of the editor's own substitutions are on.
public enum EditorProfile: String, Codable, CaseIterable, Sendable {
    /// Plain text, no substitutions (TextEdit in plain-text mode).
    case plain
    /// Smart quotes and dashes on (TextEdit's defaults for rich text).
    case smart
}

/// What the runner needs from the world: the real app posts key events and reads the chip through
/// Accessibility; tests use a fake.
public protocol BenchIO: AnyObject {
    /// Milliseconds since the run started.
    func nowMs() -> Double
    func sleep(ms: Int)
    /// The editor's text (the caret is always at the end).
    func text() -> String
    /// The chip's text when one is on screen.
    func chip() -> String?
    /// Post one key. Returns a reason when a guard refused (the run stops).
    func post(_ action: Action) -> String?
    /// Empty the editor and apply `profile`.
    func reset(profile: EditorProfile)
    /// A reason to stop now (focus left the editor, someone touched the mouse or keyboard, time is up).
    func stopReason() -> String?
}

public enum FindingKind: String, Codable, Sendable {
    /// The editor's text differs from what the typist typed and accepted.
    case textMismatch = "text_mismatch"
    /// Tab or Shift+Tab inserted something other than the chip's next word or rest.
    case acceptMismatch = "accept_mismatch"
    /// The chip stayed after the text stopped continuing it.
    case staleChip = "stale_chip"
    /// The chip did not shrink while the typist typed its next characters.
    case chipNotFollowing = "chip_not_following"
    /// Right after an accept, the chip offered the accepted words again.
    case chipRepeatsAccepted = "chip_repeats_accepted"
    /// The chip stayed after Esc.
    case chipAfterEscape = "chip_after_escape"
}

public struct Finding: Codable, Sendable, Equatable {
    public var kind: FindingKind
    public var profile: EditorProfile
    public var episode: Int
    public var step: Int
    public var atMs: Int
    public var expected: String
    public var actual: String
    public var chip: String?
    /// True when the chip changed within 60 ms before the key, so BOB may have seen a newer chip.
    public var racy: Bool
    public var recent: [String]
}

public struct Summary: Codable, Sendable {
    public var seed: UInt64
    public var episodes = 0
    public var keys = 0
    public var suggestions = 0
    public var accepts = 0
    public var acceptsOK = 0
    public var findings: [String: Int] = [:]
    public var stoppedBecause: String?
    public var durationMs = 0

    public var defectsPer1000Keys: Double {
        keys == 0 ? 0 : Double(findings.values.reduce(0, +)) * 1000 / Double(keys)
    }
}

/// Runs typist sessions against a BOB and checks every step. One instance per run.
public final class Runner {
    private let io: BenchIO
    private let corpus: [String]
    public private(set) var summary: Summary
    public private(set) var findings: [Finding] = []
    private var tracker = ChipTracker()
    private var expected = ""
    private var recent: [String] = []
    private var profile = EditorProfile.plain
    private var episode = 0
    private var step = 0
    private var lastAccepted: (text: String, atMs: Double)?
    private var lastClaim: String?
    /// Called with each finding as it is made (the app writes it to disk at once).
    public var onFinding: ((Finding) -> Void)?

    public init(io: BenchIO, seed: UInt64, corpus: [String]) {
        self.io = io
        self.corpus = corpus
        summary = Summary(seed: seed)
    }

    private func note(_ s: String) {
        recent.append(s)
        if recent.count > 14 { recent.removeFirst(recent.count - 14) }
    }

    private func report(_ kind: FindingKind, expected: String, actual: String, chip: String? = nil, racy: Bool = false) {
        let f = Finding(kind: kind, profile: profile, episode: episode, step: step, atMs: Int(io.nowMs()),
                        expected: expected, actual: actual, chip: chip, racy: racy, recent: recent)
        findings.append(f)
        summary.findings[kind.rawValue, default: 0] += 1
        onFinding?(f)
    }

    /// Read the editor and the chip once and run the continuous checks.
    @discardableResult
    private func observe() -> (text: String, chip: String?) {
        let text = io.text()
        let chip = io.chip()
        let now = io.nowMs()
        if let issue = tracker.observe(text: text, chip: chip, atMs: now) {
            switch issue {
            case let .stale(chip, shownText):
                report(.staleChip, expected: "no chip", actual: shownText, chip: chip)
            case let .notFollowing(chip, want):
                report(.chipNotFollowing, expected: want, actual: chip, chip: chip)
            }
        }
        if let claim = tracker.claim, claim != lastClaim {
            lastClaim = claim
            summary.suggestions += 1
            note("chip \"\(tracker.shown ?? "")\"")
            if let acc = lastAccepted, now - acc.atMs < 1_000, let shown = tracker.shown {
                let word = acc.text.trimmingCharacters(in: .whitespaces)
                let offered = shown.trimmingCharacters(in: .whitespaces)
                if word.count >= 3, normalized(text).hasSuffix(acc.text),
                   offered == word || offered.hasPrefix(word + " ") {
                    report(.chipRepeatsAccepted, expected: "a continuation after \"\(word)\"", actual: shown, chip: shown)
                }
            }
        }
        return (text, chip)
    }

    /// Sleep in small slices, observing between them. Returns a stop reason if one came up.
    private func wait(ms: Int, until done: (() -> Bool)? = nil) -> String? {
        let end = io.nowMs() + Double(ms)
        while io.nowMs() < end {
            if let r = io.stopReason() { return r }
            io.sleep(ms: 15)
            observe()
            if let done, done() { return nil }
        }
        return nil
    }

    private func checkText(_ text: String) {
        guard normalized(text) != normalized(expected) else { return }
        // Give a slow app a moment before calling it a mismatch.
        _ = wait(ms: 250)
        let now = io.text()
        guard normalized(now) != normalized(expected) else { return }
        report(.textMismatch, expected: tail(expected), actual: tail(now))
        expected = now
    }

    /// Run episodes until `episodes` are done or a guard stops the run.
    public func run(episodes: Int, stepsPerEpisode: Int, profiles: [EditorProfile], seed: UInt64) {
        let start = io.nowMs()
        defer { summary.durationMs = Int(io.nowMs() - start) }
        var picker = SeededRandom(seed: seed ^ 0xBEEF)
        for e in 0..<episodes {
            episode = e
            profile = profiles.randomElement(using: &picker) ?? .plain
            io.reset(profile: profile)
            expected = ""
            tracker = ChipTracker()
            lastAccepted = nil
            lastClaim = nil
            recent = []
            if let r = wait(ms: 300) { summary.stoppedBecause = r; return }
            var typist = Typist(seed: seed &+ UInt64(e) &* 7919, corpus: corpus)
            summary.episodes += 1
            for s in 0..<stepsPerEpisode {
                step = s
                if let r = runStep(&typist) { summary.stoppedBecause = r; return }
            }
            if let r = wait(ms: 1_200) { summary.stoppedBecause = r; return }
            checkText(io.text())
        }
    }

    private func runStep(_ typist: inout Typist) -> String? {
        if let r = io.stopReason() { return r }
        let (text, chip) = observe()
        checkText(text)
        let current = io.text()
        let action = typist.next(text: current, chip: tracker.shown)
        switch action {
        case let .pause(ms):
            note(action.description)
            return wait(ms: ms)
        case let .accept(key):
            return accept(key, before: current, chip: chip)
        case .escape:
            if let r = io.post(.escape) { return r }
            summary.keys += 1
            note("Esc")
            if let r = wait(ms: 400) { return r }
            if let shown = io.chip() { report(.chipAfterEscape, expected: "no chip", actual: shown, chip: shown) }
            return nil
        case let .type(c):
            if let r = io.post(action) { return r }
            summary.keys += 1
            expected.append(c)
            note(action.description)
        case .backspace:
            if let r = io.post(action) { return r }
            summary.keys += 1
            if !expected.isEmpty { expected.removeLast() }
            note(action.description)
        }
        return wait(ms: typist.delayMs(after: action))
    }

    private func accept(_ key: AcceptKey, before: String, chip: String?) -> String? {
        let racy = io.nowMs() - tracker.changedAtMs < 60
        let claim = tracker.claim
        let insert = claim.flatMap { expectedInsert(claim: $0, before: normalized(before), key: key) }
        if let r = io.post(.accept(key)) { return r }
        summary.keys += 1
        summary.accepts += 1
        note(Action.accept(key).description)
        if let r = wait(ms: 700, until: { normalized(self.io.text()) != normalized(before) }) { return r }
        if let r = wait(ms: 150) { return r }
        let after = io.text()
        if let insert {
            let want = normalized(before) + insert
            if normalized(after) == want {
                summary.acceptsOK += 1
                lastAccepted = (insert, io.nowMs())
            } else if !(racy && after == before + "\t") {
                report(.acceptMismatch, expected: tail(want), actual: tail(after), chip: chip, racy: racy)
            }
        } else if key == .tab, after != before, after != before + "\t" {
            // No suggestion to take: the editor gets the Tab (a tab character), BOB must not insert text.
            report(.acceptMismatch, expected: tail(before + "\t"), actual: tail(after), chip: chip, racy: racy)
        }
        expected = after
        return nil
    }
}
