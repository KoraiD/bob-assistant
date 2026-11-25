import AppKit

/// Finds a misspelled word the user just finished, using the macOS spell checker (on this Mac, with the
/// languages and learned words of System Settings > Keyboard). Only the last word before the caret is
/// checked, and only once it is complete (followed by a space or punctuation).
public enum TypoFinder {
    public struct Typo: Equatable, Sendable {
        public let word: String
        public let fix: String
        /// What follows the word before the caret (a space, punctuation).
        public let trailing: String
    }

    /// The finished last word and what follows it, or nil (pure, tested). Skips words that are probably
    /// names (capitalized mid-sentence), codes (digits, `@`, `_`, `/`), very short or long words, and
    /// anything in an address or path.
    public static func lastWord(_ before: String) -> (word: String, trailing: String, sentenceStart: Bool)? {
        let trailing = String(before.reversed().prefix { " .,;:!?)\"'”’".contains($0) }.reversed())
        guard !trailing.isEmpty, trailing.count <= 3 else { return nil }
        let body = before.dropLast(trailing.count)
        let word = String(body.reversed().prefix { $0.isLetter || $0 == "'" || $0 == "’" }.reversed())
        guard (3...24).contains(word.count) else { return nil }
        let rest = body.dropLast(word.count)
        // Part of something bigger: an email, a path, a hashtag, a number.
        if let c = rest.last, !c.isWhitespace, !"(\"'“‘".contains(c) { return nil }
        let start = rest.trimmingCharacters(in: .whitespaces).last.map { ".!?\n".contains($0) } ?? true
        guard let first = word.first else { return nil }
        if first.isUppercase && !start { return nil }  // probably a name
        if word.dropFirst().contains(where: \.isUppercase) { return nil }  // camelCase, acronyms
        return (word, trailing, start)
    }

    /// Ask the spell checker about the last finished word in `before`. Main thread only.
    @MainActor
    public static func find(in before: String) -> Typo? {
        guard let (word, trailing, _) = lastWord(before) else { return nil }
        let checker = NSSpellChecker.shared
        checker.automaticallyIdentifiesLanguages = true
        // The sentence around the word helps the checker pick the language.
        let context = String(before.suffix(200))
        let ns = context as NSString
        let range = ns.range(of: word, options: .backwards)
        guard range.location != NSNotFound else { return nil }
        // macOS's autocorrection knows common slips ("teh") that the spelling check accepts.
        var fix = checker.correction(forWordRange: range, in: context, language: checker.language(),
                                     inSpellDocumentWithTag: 0)
        if fix == nil {
            let miss = checker.checkSpelling(of: context, startingAt: range.location)
            guard miss.location == range.location, miss.length == range.length else { return nil }
            fix = checker.guesses(forWordRange: range, in: context, language: nil, inSpellDocumentWithTag: 0)?.first
        }
        guard let fix, fix != word, distance(fix.lowercased(), word.lowercased()) <= 2 else { return nil }
        return Typo(word: word, fix: matchCase(fix, like: word), trailing: trailing)
    }

    /// "Teh" -> "The": keep a capital first letter.
    static func matchCase(_ fix: String, like word: String) -> String {
        guard let w = word.first, w.isUppercase, let f = fix.first, f.isLowercase else { return fix }
        return f.uppercased() + fix.dropFirst()
    }

    /// Levenshtein distance (small words only).
    static func distance(_ a: String, _ b: String) -> Int {
        let a = Array(a), b = Array(b)
        var row = Array(0...b.count)
        for i in 1...max(a.count, 1) where !a.isEmpty {
            var prev = row[0]
            row[0] = i
            for j in 1...max(b.count, 1) where !b.isEmpty {
                let old = row[j]
                row[j] = min(row[j] + 1, row[j - 1] + 1, prev + (a[i - 1] == b[j - 1] ? 0 : 1))
                prev = old
            }
        }
        return a.isEmpty ? b.count : row[b.count]
    }
}
