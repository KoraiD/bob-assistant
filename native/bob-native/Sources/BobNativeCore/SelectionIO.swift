import AppKit
import ApplicationServices
import Carbon.HIToolbox
import Foundation

/// Everything on a pasteboard, so BOB can borrow the clipboard for Cmd+C / Cmd+V and put it back (SA-2, SA-6).
public struct PasteboardSnapshot: Equatable {
    /// Per item: (type, data) pairs, in order.
    public var items: [[(NSPasteboard.PasteboardType, Data)]]

    public static func == (a: Self, b: Self) -> Bool {
        a.items.count == b.items.count && zip(a.items, b.items).allSatisfy { x, y in
            x.count == y.count && zip(x, y).allSatisfy { $0.0 == $1.0 && $0.1 == $1.1 }
        }
    }

    public static func capture(_ pb: NSPasteboard) -> PasteboardSnapshot {
        let items = (pb.pasteboardItems ?? []).map { item in
            item.types.compactMap { type in item.data(forType: type).map { (type, $0) } }
        }
        return PasteboardSnapshot(items: items)
    }

    public func restore(to pb: NSPasteboard) {
        pb.clearContents()
        guard !items.isEmpty else { return }
        let restored = items.map { pairs -> NSPasteboardItem in
            let item = NSPasteboardItem()
            for (type, data) in pairs { item.setData(data, forType: type) }
            return item
        }
        pb.writeObjects(restored)
    }
}

/// What the hotkey found (SA-1, SA-2, SA-10).
public struct SelectionCapture {
    public var appBundleId: String
    public var pid: pid_t
    public var text: String
    public var secure: Bool
    public var source: SelectionSource?
    /// Selection bounds in AX coordinates, when the app reports them.
    public var anchor: Rect?
}

/// Reads and replaces the selection in the frontmost app. Main thread only.
public enum SelectionIO {
    /// Longest selection BOB reads (characters); longer ones are refused by the core with a message.
    public static let maxChars = 20_000
    static let copyTimeout: TimeInterval = 0.4

    // MARK: read

    public static func read(completion: @escaping (SelectionCapture) -> Void) {
        let front = NSWorkspace.shared.frontmostApplication
        let pid = front?.processIdentifier ?? 0
        var capture = SelectionCapture(appBundleId: front?.bundleIdentifier ?? "", pid: pid, text: "",
                                       secure: false, source: nil, anchor: nil)
        if let element = AXReader.focusedElement() {
            let subrole = AXReader.string(element, kAXSubroleAttribute)
            if subrole == (kAXSecureTextFieldSubrole as String) {
                capture.secure = true  // never read (SA-10)
                completion(capture)
                return
            }
            capture.anchor = selectionBounds(element)
            if let text = AXReader.string(element, kAXSelectedTextAttribute), !text.isEmpty {
                capture.text = String(text.prefix(maxChars + 1))
                capture.source = .accessibility
                completion(capture)
                return
            }
        }
        // Fallback: Cmd+C with the clipboard saved and restored.
        copySelection { text in
            if let text, !text.isEmpty {
                capture.text = String(text.prefix(maxChars + 1))
                capture.source = .copy
            }
            completion(capture)
        }
    }

    /// Bounds of the selected range (range API first, then text markers), AX coordinates.
    static func selectionBounds(_ element: AXUIElement) -> Rect? {
        if let range = AXReader.range(element, kAXSelectedTextRangeAttribute), range.length > 0,
           let r = AXReader.bounds(element, for: range), r.width > 0 || r.height > 0, r.x.isFinite, r.y.isFinite {
            return r
        }
        if let selected = AXReader.attribute(element, kAXSelectedTextMarkerRangeAttribute),
           AXReader.isTextMarkerRange(selected), let r = AXReader.markerBounds(element, selected),
           r.width > 0 || r.height > 0 {
            return r
        }
        return nil
    }

    /// Post Cmd+C, wait for the pasteboard to change, read its string, restore the previous contents.
    static func copySelection(completion: @escaping (String?) -> Void) {
        let pb = NSPasteboard.general
        let saved = PasteboardSnapshot.capture(pb)
        let before = pb.changeCount
        postShortcut(keyCode: CGKeyCode(kVK_ANSI_C))
        let deadline = Date().addingTimeInterval(copyTimeout)
        func poll() {
            if pb.changeCount != before {
                let text = pb.string(forType: .string)
                saved.restore(to: pb)
                completion(text)
            } else if Date() >= deadline {
                completion(nil)  // nothing copied: the clipboard was not touched
            } else {
                DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(15), execute: poll)
            }
        }
        poll()
    }

    /// Cmd+<key>, marked as BOB's own event so the key tap ignores it.
    static func postShortcut(keyCode: CGKeyCode) {
        guard let source = CGEventSource(stateID: .hidSystemState) else { return }
        source.userData = bobSyntheticEventMarker
        for down in [true, false] {
            guard let ev = CGEvent(keyboardEventSource: source, virtualKey: keyCode, keyDown: down) else { continue }
            ev.flags = .maskCommand
            ev.setIntegerValueField(.eventSourceUserData, value: bobSyntheticEventMarker)
            ev.post(tap: .cghidEventTap)
        }
    }

    // MARK: replace

    /// Replace the selection with `text` if it still equals `expected` in the same app (SA-6, SA-7).
    /// Accessibility first (one undoable edit in native text views), paste with clipboard restore otherwise.
    public static func replace(expected: String, text: String, pid: pid_t,
                               completion: @escaping (_ ok: Bool, _ method: String?, _ error: String?) -> Void) {
        guard NSWorkspace.shared.frontmostApplication?.processIdentifier == pid else {
            completion(false, nil, "you switched apps, so nothing was replaced")
            return
        }
        let element = AXReader.focusedElement()
        if let element, AXReader.pid(of: element) == pid,
           let current = AXReader.string(element, kAXSelectedTextAttribute), !current.isEmpty {
            guard SelectionGuard.matches(current: current, expected: expected) else {
                completion(false, nil, "the selection changed, so nothing was replaced")
                return
            }
            let valueBefore = AXReader.string(element, kAXValueAttribute)
            let status = AXUIElementSetAttributeValue(element, kAXSelectedTextAttribute as CFString, text as CFString)
            let valueAfter = AXReader.string(element, kAXValueAttribute)
            // Some apps report success and ignore the write; a readable, unchanged value means "ignored".
            if status == .success, valueBefore == nil || valueAfter != valueBefore {
                completion(true, "accessibility", nil)
                return
            }
            paste(text, completion: completion)
            return
        }
        // No AX selection: verify with a fresh Cmd+C, then paste.
        copySelection { current in
            guard let current, SelectionGuard.matches(current: current, expected: expected) else {
                completion(false, nil, "the selection changed, so nothing was replaced")
                return
            }
            paste(text, completion: completion)
        }
    }

    /// Put `text` on the clipboard, Cmd+V, then restore the clipboard once the app has read it.
    static func paste(_ text: String, completion: @escaping (Bool, String?, String?) -> Void) {
        let pb = NSPasteboard.general
        let saved = PasteboardSnapshot.capture(pb)
        pb.clearContents()
        pb.setString(text, forType: .string)
        postShortcut(keyCode: CGKeyCode(kVK_ANSI_V))
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(450)) {
            saved.restore(to: pb)
            completion(true, "paste", nil)
        }
    }
}

/// SA-7 / P2: replace only when the current selection is the one that was transformed. Apps differ in
/// line endings and may trim a trailing newline from a selection, so those differences are tolerated;
/// any other difference blocks the replacement.
public enum SelectionGuard {
    public static func normalize(_ s: String) -> String {
        s.replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n")
            .trimmingCharacters(in: .newlines)
    }

    public static func matches(current: String, expected: String) -> Bool {
        !expected.isEmpty && normalize(current) == normalize(expected)
    }
}
