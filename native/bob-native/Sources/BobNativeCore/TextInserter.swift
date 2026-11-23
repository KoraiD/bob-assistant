import CoreGraphics
import Foundation

/// Types text at the caret of the focused app with synthetic Unicode key events (GT-14). This works in
/// nearly every app and leaves the clipboard untouched. Fallbacks (AX replacement, paste) come with
/// Selection Actions (Task 7).
public enum TextInserter {
    /// `CGEventKeyboardSetUnicodeString` handles short strings only; 16 UTF-16 units per event is safe.
    public static let maxUnitsPerEvent = 16

    /// Split `text` into chunks of at most `limit` UTF-16 units without splitting a character
    /// (surrogate pairs, emoji, combining sequences stay whole).
    public static func chunks(_ text: String, limit: Int = maxUnitsPerEvent) -> [[UniChar]] {
        var out: [[UniChar]] = []
        var current: [UniChar] = []
        for ch in text {
            let units = Array(String(ch).utf16)
            if !current.isEmpty, current.count + units.count > limit {
                out.append(current)
                current = []
            }
            current.append(contentsOf: units)
        }
        if !current.isEmpty { out.append(current) }
        return out
    }

    /// Post the text as keyboard events marked as BOB's own (so the key tap never captures them).
    public static func insert(_ text: String) {
        guard let source = CGEventSource(stateID: .hidSystemState) else { return }
        source.userData = bobSyntheticEventMarker
        for chunk in chunks(text) {
            for keyDown in [true, false] {
                guard let ev = CGEvent(keyboardEventSource: source, virtualKey: 0, keyDown: keyDown) else { continue }
                // No modifiers may leak into the typed text (e.g. a still-held Shift from Shift+Tab).
                ev.flags = []
                chunk.withUnsafeBufferPointer { buf in
                    ev.keyboardSetUnicodeString(stringLength: buf.count, unicodeString: buf.baseAddress)
                }
                ev.setIntegerValueField(.eventSourceUserData, value: bobSyntheticEventMarker)
                ev.post(tap: .cghidEventTap)
            }
        }
    }
}
