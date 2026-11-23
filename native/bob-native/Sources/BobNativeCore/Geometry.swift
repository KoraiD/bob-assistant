import Foundation

/// Converts between Accessibility coordinates and AppKit window coordinates.
///
/// Accessibility (and the wire protocol) use global display coordinates with the origin at the
/// **top-left of the primary display**, y growing downward. AppKit uses the **bottom-left of the primary
/// display**, y growing upward. This holds across multiple displays because both are anchored at the
/// primary display (the one with the menu bar).
public enum CoordinateConverter {
    public static func appKitRect(fromAX r: Rect, primaryScreenHeight h: Double) -> Rect {
        Rect(x: r.x, y: h - (r.y + r.height), width: r.width, height: r.height)
    }

    public static func axRect(fromAppKit r: Rect, primaryScreenHeight h: Double) -> Rect {
        Rect(x: r.x, y: h - (r.y + r.height), width: r.width, height: r.height)
    }
}

/// Decides which Accessibility bounds to trust for the caret (requirement GT-4: when none is
/// plausible the chip must not be shown rather than being misplaced).
public enum CaretRectResolver {
    /// A rect is plausible when it is finite, not the all-zero "unknown" value and caret-sized.
    public static func isPlausible(_ r: Rect) -> Bool {
        let values = [r.x, r.y, r.width, r.height]
        guard values.allSatisfy({ $0.isFinite }) else { return false }
        if r.x == 0, r.y == 0, r.width == 0, r.height == 0 { return false }
        guard r.height > 0, r.height <= 400 else { return false }
        guard r.width >= 0, r.width <= 4000 else { return false }
        return true
    }

    public enum Pick: Equatable, Sendable { case previousChar, atCaret, nextChar }

    /// Compose the caret rect from up to three Accessibility answers.
    ///
    /// Observed on real apps (docs/COMPATIBILITY.md): for a zero-length range some apps return the whole line
    /// box (iTerm: 34 pt tall on an empty prompt) or a wide box (Slack, empty field: the placeholder). The
    /// glyph box of the character before the caret is tight, so it wins whenever it sits on the caret's line.
    ///
    /// - Parameters:
    ///   - atCaret: bounds of the zero-length range at the caret (may be a line or field box).
    ///   - previousChar: bounds of the character before the caret.
    ///   - nextChar: bounds of the character after the caret.
    ///   - previousIsNewline: the character before the caret is a line break, so its box is on the line above.
    /// - Returns: a zero-width rect at the caret and which input produced it, or nil when nothing is plausible.
    public static func compose(atCaret: Rect?, previousChar: Rect?, nextChar: Rect?,
                               previousIsNewline: Bool = false) -> (Rect, Pick)? {
        let at = atCaret.flatMap { isPlausible($0) ? $0 : nil }
        let prev = previousIsNewline ? nil : previousChar.flatMap { isPlausible($0) ? $0 : nil }
        if let p = prev {
            // A caret box on another visual line (soft wrap) means the previous glyph is not on our line.
            let onSameLine = at.map { a in
                let mid = p.y + p.height / 2
                return mid >= a.y - 1 && mid <= a.y + a.height + 1
            } ?? true
            if onSameLine {
                return (Rect(x: p.x + p.width, y: p.y, width: 0, height: p.height), .previousChar)
            }
        }
        if let a = at {
            // Left edge: for a collapsed range that is the caret, even when the app returns a wider box.
            let h = prev.map { min($0.height, a.height) } ?? a.height
            return (Rect(x: a.x, y: a.y + a.height - h, width: 0, height: h), .atCaret)
        }
        if let n = nextChar, isPlausible(n) {
            return (Rect(x: n.x, y: n.y, width: 0, height: n.height), .nextChar)
        }
        return nil
    }

    /// Back-compatible wrapper returning only the rect.
    public static func resolve(atCaret: Rect?, previousChar: Rect?, nextChar: Rect?) -> Rect? {
        compose(atCaret: atCaret, previousChar: previousChar, nextChar: nextChar)?.0
    }
}

/// Validates a candidate caret rect against the focused element's frame and fixes its orientation.
///
/// Native AppKit text views return `AXBoundsForRange` in AX coordinates (top-left origin). Chromium's
/// `AXBoundsForTextMarkerRange` builds the rect with `gfx::ScreenRectToNSRect`, i.e. AppKit coordinates
/// (bottom-left origin), so depending on how the system bridges it the value may arrive vertically flipped.
/// Rather than hard-coding per app, accept whichever orientation lies inside the element's frame.
public enum CaretOrientation {
    /// Slack around the element frame, in points (caret rects can touch or slightly exceed the frame).
    public static let tolerance: Double = 8

    public enum Verdict: Equatable, Sendable {
        case asIs(Rect)
        case flipped(Rect)
        /// Both orientations lie inside the element (a tall field around the middle of the screen).
        case ambiguous(asIs: Rect, flipped: Rect)
        case rejected
    }

    static func contains(_ frame: Rect, _ r: Rect, tolerance t: Double) -> Bool {
        r.x >= frame.x - t && r.x <= frame.x + frame.width + t &&
            r.y >= frame.y - t && r.y + r.height <= frame.y + frame.height + t
    }

    /// Classify `candidate` (as returned by the app) against `elementFrame` (AX coordinates).
    public static func classify(_ candidate: Rect, elementFrame: Rect?, primaryScreenHeight: Double) -> Verdict {
        guard CaretRectResolver.isPlausible(candidate) else { return .rejected }
        guard let frame = elementFrame, frame.width > 0, frame.height > 0 else { return .asIs(candidate) }
        let flipped = CoordinateConverter.axRect(fromAppKit: candidate, primaryScreenHeight: primaryScreenHeight)
        let directFits = contains(frame, candidate, tolerance: tolerance)
        let flippedFits = contains(frame, flipped, tolerance: tolerance)
        switch (directFits, flippedFits) {
        case (true, true) where flipped == candidate: return .asIs(candidate)
        case (true, true): return .ambiguous(asIs: candidate, flipped: flipped)
        case (true, false): return .asIs(candidate)
        case (false, true): return .flipped(flipped)
        case (false, false): return .rejected
        }
    }
}

/// Remembers, per app and caret source, which orientation unambiguous samples used, and applies it to
/// ambiguous ones. Without any evidence the AX convention (as returned) is assumed.
public struct OrientationMemory: Sendable {
    private var flippedByKey: [String: Bool] = [:]

    public init() {}

    static func key(_ app: String, _ source: String) -> String { app + "|" + source }

    /// Resolve a verdict to a rect, learning from it. Returns nil when the candidate was rejected.
    public mutating func resolve(_ verdict: CaretOrientation.Verdict, app: String, source: String) -> Rect? {
        let k = Self.key(app, source)
        switch verdict {
        case let .asIs(r):
            flippedByKey[k] = false
            return r
        case let .flipped(r):
            flippedByKey[k] = true
            return r
        case let .ambiguous(asIs, flipped):
            return flippedByKey[k] == true ? flipped : asIs
        case .rejected:
            return nil
        }
    }
}

/// Extracts the text before the caret to send to the core.
public enum TextWindow {
    public static let defaultMaxChars = 2000

    /// Accessibility ranges are measured in UTF-16 code units. Returns the last `maxChars` characters
    /// (grapheme clusters) before `caretUTF16`. Out-of-range offsets are clamped, and a caret inside a
    /// surrogate pair yields a replacement character rather than a crash.
    public static func beforeCaret(value: String, caretUTF16: Int, maxChars: Int = defaultMaxChars) -> String {
        let units = Array(value.utf16)
        let end = min(max(caretUTF16, 0), units.count)
        guard end > 0, maxChars > 0 else { return "" }
        let prefix = String(decoding: units[0..<end], as: UTF16.self)
        return String(prefix.suffix(maxChars))
    }

    /// Context after the caret (PC-3) is kept short: it only tells the brain what follows.
    public static let defaultAfterChars = 400

    /// The first `maxChars` characters after the selection end (`caretUTF16 + selectionUTF16`), clamped
    /// like `beforeCaret`.
    public static func afterCaret(value: String, caretUTF16: Int, selectionUTF16: Int = 0,
                                  maxChars: Int = defaultAfterChars) -> String {
        let units = Array(value.utf16)
        let start = min(max(caretUTF16, 0) + max(selectionUTF16, 0), units.count)
        guard start < units.count, maxChars > 0 else { return "" }
        let suffix = String(decoding: units[start...], as: UTF16.self)
        return String(suffix.prefix(maxChars))
    }
}
