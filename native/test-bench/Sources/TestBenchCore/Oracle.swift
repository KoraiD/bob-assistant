import Foundation

/// The keys that accept a suggestion.
public enum AcceptKey: String, Codable, Sendable {
    case tab, shiftTab
}

/// Split off the next word of a chip, with its leading whitespace (mirrors `ghost::chip::accept_word`).
/// Returns `(inserted, remainder)` with `inserted + remainder == chip`.
public func acceptWord(_ chip: String) -> (String, String) {
    let lead = chip.prefix(while: { $0.isWhitespace })
    let rest = chip.dropFirst(lead.count)
    let word = rest.prefix(while: { !$0.isWhitespace })
    let inserted = String(lead) + String(word)
    return (inserted, String(chip.dropFirst(inserted.count)))
}

/// What Tab or Shift+Tab should insert, given the chip's promise `claim` (the text the chip completes) and the
/// text before the caret at the key press. Nil when the chip no longer continues the text. Tab takes the next
/// word, or the whole rest when only whitespace would remain (mirrors `sim::world::expected_insert`).
public func expectedInsert(claim: String, before: String, key: AcceptKey) -> String? {
    guard claim.hasPrefix(before) else { return nil }
    let rest = String(claim.dropFirst(before.count))
    guard !rest.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
    switch key {
    case .shiftTab:
        return rest
    case .tab:
        let (word, remainder) = acceptWord(rest)
        return remainder.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? rest : word
    }
}

/// Text compared without the editor's own substitutions (smart quotes), so only BOB's changes count.
public func normalized(_ s: String) -> String {
    var out = s
    for (curly, straight) in [("\u{2019}", "'"), ("\u{2018}", "'"), ("\u{201C}", "\""), ("\u{201D}", "\"")] {
        out = out.replacingOccurrences(of: curly, with: straight)
    }
    return out
}

/// The last `n` characters, for reports.
public func tail(_ s: String, _ n: Int = 60) -> String {
    String(s.suffix(n))
}

/// A chip problem the tracker found.
public enum ChipIssue: Equatable, Sendable {
    /// The text no longer continues the chip's promise, but the chip stayed on screen.
    case stale(chip: String, text: String)
    /// The text continues the promise, but the chip did not shrink to match.
    case notFollowing(chip: String, expected: String)
}

/// Follows what the chip promises and reports a chip that stays out of step with the text for longer than
/// `limitMs` (the simulator's `STALE_LIMIT`). A chip that changes in any other way than shrinking with the
/// typed text is a new suggestion and starts a new promise.
public struct ChipTracker: Sendable {
    public private(set) var claim: String?
    public private(set) var shown: String?
    public private(set) var changedAtMs: Double = -1_000_000
    private var badSinceMs: Double?
    private var reported = false
    public let limitMs: Double

    public init(limitMs: Double = 700) {
        self.limitMs = limitMs
    }

    /// Feed one observation. Returns an issue once per out-of-step stretch.
    public mutating func observe(text rawText: String, chip: String?, atMs now: Double) -> ChipIssue? {
        let text = normalized(rawText)
        guard let chip = chip.map(normalized), !chip.isEmpty else {
            if shown != nil { changedAtMs = now }
            claim = nil
            shown = nil
            badSinceMs = nil
            reported = false
            return nil
        }
        if chip != shown {
            changedAtMs = now
            let follows = claim.map { $0.hasPrefix(text) && String($0.dropFirst(text.count)) == chip } ?? false
            if !follows { claim = text + chip }
            shown = chip
        }
        guard let claim else { return nil }
        let consistent = claim.hasPrefix(text) && String(claim.dropFirst(text.count)) == chip
        if consistent {
            badSinceMs = nil
            reported = false
            return nil
        }
        let since = badSinceMs ?? now
        badSinceMs = since
        guard !reported, now - since >= limitMs else { return nil }
        reported = true
        if claim.hasPrefix(text) {
            return .notFollowing(chip: chip, expected: String(claim.dropFirst(text.count)))
        }
        return .stale(chip: chip, text: tail(text, 40))
    }
}
