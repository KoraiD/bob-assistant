import AppKit
import ApplicationServices

/// What the helper learned about the focused UI element. Contains typed text, so it is never logged.
public struct AXSnapshot {
    public var appBundleId: String
    public var appName: String
    public var role: String?
    public var subrole: String?
    public var secure: Bool
    public var editable: Bool
    public var valueLength: Int?
    public var beforeCaret: String?
    public var selectionLen: Int?
    /// Caret rect in **AX coordinates** (top-left origin) or nil when no plausible bounds exist.
    public var caret: Rect?
}

/// Thin wrappers around the Accessibility C API. No force unwraps on AX values (macos-native steering).
public enum AXReader {
    static let editableRoles: Set<String> = ["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"]

    public static func focusedElement() -> AXUIElement? {
        var value: CFTypeRef?
        let system = AXUIElementCreateSystemWide()
        guard AXUIElementCopyAttributeValue(system, kAXFocusedUIElementAttribute as CFString, &value) == .success,
              let value, CFGetTypeID(value) == AXUIElementGetTypeID() else { return nil }
        return (value as! AXUIElement)  // type id checked above
    }

    static func string(_ element: AXUIElement, _ attribute: String) -> String? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, attribute as CFString, &value) == .success else { return nil }
        return value as? String
    }

    static func range(_ element: AXUIElement, _ attribute: String) -> CFRange? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, attribute as CFString, &value) == .success,
              let value, CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
        var range = CFRange()
        guard AXValueGetValue(value as! AXValue, .cfRange, &range) else { return nil }
        return range
    }

    /// `kAXBoundsForRangeParameterizedAttribute` for `range`, in AX screen coordinates.
    static func bounds(_ element: AXUIElement, for range: CFRange) -> Rect? {
        var r = range
        guard let param = AXValueCreate(.cfRange, &r) else { return nil }
        var out: CFTypeRef?
        guard AXUIElementCopyParameterizedAttributeValue(
            element, kAXBoundsForRangeParameterizedAttribute as CFString, param, &out) == .success,
            let out, CFGetTypeID(out) == AXValueGetTypeID() else { return nil }
        var rect = CGRect.zero
        guard AXValueGetValue(out as! AXValue, .cgRect, &rect) else { return nil }
        return Rect(x: rect.origin.x, y: rect.origin.y, width: rect.size.width, height: rect.size.height)
    }

    public static func pid(of element: AXUIElement) -> pid_t? {
        var pid: pid_t = 0
        return AXUIElementGetPid(element, &pid) == .success ? pid : nil
    }

    /// Build a snapshot. For secure fields the text is never read (requirement GT-3).
    public static func snapshot(of element: AXUIElement) -> AXSnapshot {
        var bundle = ""
        var name = ""
        if let p = pid(of: element), let app = NSRunningApplication(processIdentifier: p) {
            bundle = app.bundleIdentifier ?? ""
            name = app.localizedName ?? ""
        }
        let role = string(element, kAXRoleAttribute)
        let subrole = string(element, kAXSubroleAttribute)
        let secure = subrole == (kAXSecureTextFieldSubrole as String)
        let editable = role.map { editableRoles.contains($0) } ?? false
        var snap = AXSnapshot(appBundleId: bundle, appName: name, role: role, subrole: subrole,
                              secure: secure, editable: editable, valueLength: nil, beforeCaret: nil,
                              selectionLen: nil, caret: nil)
        guard !secure, editable else { return snap }

        if let selection = range(element, kAXSelectedTextRangeAttribute) {
            snap.selectionLen = max(selection.length, 0)
            if let value = string(element, kAXValueAttribute) {
                snap.valueLength = value.utf16.count
                snap.beforeCaret = TextWindow.beforeCaret(value: value, caretUTF16: selection.location)
            }
            let loc = selection.location
            let atCaret = bounds(element, for: CFRange(location: loc, length: 0))
            let previous = loc > 0 ? bounds(element, for: CFRange(location: loc - 1, length: 1)) : nil
            let next = bounds(element, for: CFRange(location: loc, length: 1))
            snap.caret = CaretRectResolver.resolve(atCaret: atCaret, previousChar: previous, nextChar: next)
        }
        return snap
    }

    /// Chromium and Electron only expose their accessibility tree after being asked (macos-native steering).
    public static func enableManualAccessibility(pid: pid_t) {
        let app = AXUIElementCreateApplication(pid)
        AXUIElementSetAttributeValue(app, "AXManualAccessibility" as CFString, kCFBooleanTrue)
    }
}
