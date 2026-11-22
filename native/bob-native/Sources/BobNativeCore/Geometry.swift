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

    /// - Parameters:
    ///   - atCaret: bounds of the zero-length range at the caret.
    ///   - previousChar: bounds of the character before the caret.
    ///   - nextChar: bounds of the character after the caret.
    /// - Returns: a zero-width rect at the caret position, or nil when nothing is plausible.
    public static func resolve(atCaret: Rect?, previousChar: Rect?, nextChar: Rect?) -> Rect? {
        if let a = atCaret, isPlausible(a) {
            return Rect(x: a.x, y: a.y, width: 0, height: a.height)
        }
        if let p = previousChar, isPlausible(p) {
            return Rect(x: p.x + p.width, y: p.y, width: 0, height: p.height)
        }
        if let n = nextChar, isPlausible(n) {
            return Rect(x: n.x, y: n.y, width: 0, height: n.height)
        }
        return nil
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
}
