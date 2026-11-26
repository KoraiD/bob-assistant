import Foundation

/// A small seeded generator (SplitMix64), so a run replays exactly from its seed.
public struct SeededRandom: RandomNumberGenerator, Sendable {
    private var state: UInt64

    public init(seed: UInt64) {
        state = seed
    }

    public mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
        return z ^ (z >> 31)
    }

    public mutating func chance(_ p: Double) -> Bool {
        Double.random(in: 0..<1, using: &self) < p
    }
}

/// One thing the typist does.
public enum Action: Equatable, Sendable, CustomStringConvertible {
    case type(Character)
    case backspace
    case accept(AcceptKey)
    case escape
    case pause(ms: Int)

    public var description: String {
        switch self {
        case let .type(c): return c == " " ? "space" : "type \(c)"
        case .backspace: return "backspace"
        case let .accept(k): return k == .tab ? "Tab" : "Shift+Tab"
        case .escape: return "Esc"
        case let .pause(ms): return "pause \(ms) ms"
        }
    }
}

/// How the typist reacts to a suggestion; the defaults follow the owner's report: sometimes take the whole
/// suggestion, sometimes a word, sometimes type part of it, sometimes type something else.
public struct Habits: Sendable {
    public var acceptWord = 0.20
    public var acceptAll = 0.08
    public var dismiss = 0.04
    /// On a new suggestion: plan to type it (type-through) instead of the sentence in mind.
    public var adoptSuggestion = 0.35
    public var typo = 0.03
    /// Thinking pause after a word, long enough for a suggestion to show.
    public var pauseAfterWord = 0.25

    public init() {}
}

/// A human-like typist at an average speed (35 to 65 words per minute, chosen per typist), with typos it
/// fixes, pauses while thinking and the choices above. Pure: it decides from the text and the chip it sees.
public struct Typist: Sendable {
    public private(set) var intent = ""
    private var rng: SeededRandom
    private let corpus: [String]
    private let habits: Habits
    private let msPerChar: Double
    private var pendingFix = false
    private var lastChip: String?

    public init(seed: UInt64, corpus: [String], habits: Habits = Habits()) {
        rng = SeededRandom(seed: seed)
        self.corpus = corpus.isEmpty ? ["The quick brown fox jumps over the lazy dog."] : corpus
        self.habits = habits
        let wpm = Double.random(in: 35...65, using: &rng)
        msPerChar = 60_000 / (wpm * 5)
    }

    /// A piece of a corpus sentence starting at a word, to continue `text` with.
    private mutating func fragment(after text: String) -> String {
        let sentence = corpus.randomElement(using: &rng) ?? ""
        let words = sentence.split(separator: " ", omittingEmptySubsequences: true)
        let start = words.count > 3 && rng.chance(0.4) ? Int.random(in: 1..<(words.count - 2), using: &rng) : 0
        let piece = words[start...].joined(separator: " ")
        let needsSpace = !(text.isEmpty || text.last?.isWhitespace == true)
        return (needsSpace ? " " : "") + piece
    }

    /// The next action for the text in the editor and the chip on screen (nil when none).
    public mutating func next(text: String, chip: String?) -> Action {
        let text = normalized(text)
        if pendingFix {
            pendingFix = false
            return .backspace
        }
        if !intent.hasPrefix(text) || intent.count <= text.count {
            intent = text + fragment(after: text)
        }
        if let chip, !chip.trimmingCharacters(in: .whitespaces).isEmpty {
            let isNew = chip != lastChip
            lastChip = chip
            if isNew, rng.chance(habits.adoptSuggestion) {
                intent = text + chip
                intent += fragment(after: intent)
            }
            let r = Double.random(in: 0..<1, using: &rng)
            if r < habits.acceptWord { return .accept(.tab) }
            if r < habits.acceptWord + habits.acceptAll { return .accept(.shiftTab) }
            if r < habits.acceptWord + habits.acceptAll + habits.dismiss { return .escape }
        } else {
            lastChip = nil
        }
        let nextChar = intent[intent.index(intent.startIndex, offsetBy: text.count)]
        if nextChar == " ", text.last?.isLetter == true, rng.chance(habits.pauseAfterWord) {
            // Pause after finishing a word half the time, before the space the other half.
            if rng.chance(0.5) { return .pause(ms: Int.random(in: 500...1_500, using: &rng)) }
        }
        if nextChar.isLetter, rng.chance(habits.typo) {
            pendingFix = true
            let wrong = "etaoinsrhl".filter { $0 != nextChar }.randomElement(using: &rng) ?? "x"
            return .type(wrong)
        }
        return .type(nextChar)
    }

    /// Milliseconds to wait after `action`, like a person: slower after spaces and sentence ends.
    public mutating func delayMs(after action: Action) -> Int {
        var ms = msPerChar * Double.random(in: 0.55...1.6, using: &rng)
        switch action {
        case let .type(c) where c == " ": ms += 40
        case let .type(c) where ".?!,".contains(c): ms += 250
        case .accept, .escape: ms += 250
        case .backspace: ms += 80
        case .pause: return 0
        default: break
        }
        return max(45, Int(ms))
    }
}
