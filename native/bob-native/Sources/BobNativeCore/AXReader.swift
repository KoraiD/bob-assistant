import AppKit
import ApplicationServices

/// Which Accessibility technique produced the caret rect. Reported by `--probe` so the compatibility
/// matrix records how each app is supported.
public enum CaretSource: String, Sendable {
    /// `AXBoundsForRange` on the zero-length range at the caret (native AppKit text views).
    case rangeAtCaret = "range_at_caret"
    /// `AXBoundsForRange` on the character before / after the caret.
    case rangePreviousChar = "range_prev_char"
    case rangeNextChar = "range_next_char"
    /// `AXBoundsForTextMarkerRange` on `AXSelectedTextMarkerRange` (Chromium, Electron, WebKit).
    case textMarker = "text_marker"
    /// Text-marker bounds of the character before the caret (when the collapsed range has no size).
    case textMarkerPreviousChar = "text_marker_prev_char"
}

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
    public var caretSource: CaretSource?
    /// The focused element's frame in AX coordinates, used to validate caret rects.
    public var elementFrame: Rect?
    /// Title of the focused window (PC-3). Not read for secure fields.
    public var windowTitle: String?
    /// Text after the caret, bounded (PC-3). Not read for secure fields.
    public var afterCaret: String?
    public var pid: pid_t = 0
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

    static func attribute(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else { return nil }
        return value
    }

    static func parameterized(_ element: AXUIElement, _ name: String, _ parameter: CFTypeRef) -> CFTypeRef? {
        var out: CFTypeRef?
        guard AXUIElementCopyParameterizedAttributeValue(element, name as CFString, parameter, &out) == .success
        else { return nil }
        return out
    }

    static func string(_ element: AXUIElement, _ name: String) -> String? {
        attribute(element, name) as? String
    }

    static func axValue<T: BitwiseCopyable>(_ value: CFTypeRef?, _ type: AXValueType, _ initial: T) -> T? {
        guard let value, CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
        var out = initial
        guard AXValueGetValue(value as! AXValue, type, &out) else { return nil }  // type id checked above
        return out
    }

    static func range(_ element: AXUIElement, _ name: String) -> CFRange? {
        axValue(attribute(element, name), .cfRange, CFRange())
    }

    static func rect(fromCG r: CGRect) -> Rect {
        Rect(x: r.origin.x, y: r.origin.y, width: r.size.width, height: r.size.height)
    }

    /// `kAXBoundsForRangeParameterizedAttribute` for `range`, as returned by the app.
    static func bounds(_ element: AXUIElement, for range: CFRange) -> Rect? {
        var r = range
        guard let param = AXValueCreate(.cfRange, &r),
              let cg = axValue(parameterized(element, kAXBoundsForRangeParameterizedAttribute, param), .cgRect, CGRect.zero)
        else { return nil }
        return rect(fromCG: cg)
    }

    /// The element's frame (AXPosition + AXSize), in AX coordinates.
    static func frame(_ element: AXUIElement) -> Rect? {
        guard let origin = axValue(attribute(element, kAXPositionAttribute), .cgPoint, CGPoint.zero),
              let size = axValue(attribute(element, kAXSizeAttribute), .cgSize, CGSize.zero),
              size.width > 0, size.height > 0 else { return nil }
        return Rect(x: origin.x, y: origin.y, width: size.width, height: size.height)
    }

    // MARK: text markers (Chromium / Electron / WebKit)

    static func isTextMarkerRange(_ v: CFTypeRef?) -> Bool {
        guard let v else { return false }
        return CFGetTypeID(v) == AXTextMarkerRangeGetTypeID()
    }

    static func markerBounds(_ element: AXUIElement, _ range: CFTypeRef) -> Rect? {
        guard let cg = axValue(parameterized(element, kAXBoundsForTextMarkerRangeParameterizedAttribute, range),
                               .cgRect, CGRect.zero) else { return nil }
        return rect(fromCG: cg)
    }

    /// Bounds of the caret via text markers, composed like the range API (glyph box first).
    static func textMarkerCaret(_ element: AXUIElement) -> (Rect, CaretSource)? {
        let selected = attribute(element, kAXSelectedTextMarkerRangeAttribute)
        guard isTextMarkerRange(selected), let selected else { return nil }
        let range = selected as! AXTextMarkerRange  // type id checked above

        let collapsed = markerBounds(element, range)
        var previousBox: Rect?
        var previousIsNewline = false
        let caretMarker = AXTextMarkerRangeCopyEndMarker(range)
        if let previous = parameterized(element, kAXPreviousTextMarkerForTextMarkerParameterizedAttribute, caretMarker),
           CFGetTypeID(previous) == AXTextMarkerGetTypeID() {
            let prevRange = AXTextMarkerRangeCreate(kCFAllocatorDefault, previous as! AXTextMarker, caretMarker)
            let text = parameterized(element, kAXStringForTextMarkerRangeParameterizedAttribute, prevRange) as? String
            previousIsNewline = text == "\n" || text == "\r"
            // An empty string means there is no previous character inside this field.
            if let text, !text.isEmpty { previousBox = markerBounds(element, prevRange) }
        }
        guard let (rect, pick) = CaretRectResolver.compose(atCaret: collapsed, previousChar: previousBox, nextChar: nil,
                                                           previousIsNewline: previousIsNewline) else { return nil }
        return (rect, pick == .previousChar ? .textMarkerPreviousChar : .textMarker)
    }

    // MARK: snapshot

    public static func pid(of element: AXUIElement) -> pid_t? {
        var pid: pid_t = 0
        return AXUIElementGetPid(element, &pid) == .success ? pid : nil
    }

    static var primaryScreenHeight: Double {
        Double(NSScreen.screens.first?.frame.height ?? 0)
    }

    /// Orientation evidence per app (main thread only, like every AX call in this helper).
    nonisolated(unsafe) static var orientation = OrientationMemory()

    static func accept(_ raw: Rect, frame: Rect?, app: String, source: CaretSource) -> Rect? {
        let verdict = CaretOrientation.classify(raw, elementFrame: frame, primaryScreenHeight: primaryScreenHeight)
        return orientation.resolve(verdict, app: app, source: source.rawValue)
    }

    /// Resolve the caret: native range bounds first, then text markers. Every candidate is validated
    /// against the element frame (and flipped if needed) so a wrong rect is never used (GT-4).
    static func caret(_ element: AXUIElement, selection: CFRange, value: String?, frame: Rect?,
                      app: String) -> (Rect, CaretSource)? {
        let loc = selection.location
        let units = value.map { Array($0.utf16) }
        let prevIsNewline = units.map { loc > 0 && loc <= $0.count && ($0[loc - 1] == 0x0A || $0[loc - 1] == 0x0D) } ?? false
        if let (raw, pick) = CaretRectResolver.compose(
            atCaret: bounds(element, for: CFRange(location: loc, length: 0)),
            previousChar: loc > 0 ? bounds(element, for: CFRange(location: loc - 1, length: 1)) : nil,
            nextChar: bounds(element, for: CFRange(location: loc, length: 1)),
            previousIsNewline: prevIsNewline) {
            let source: CaretSource = pick == .previousChar ? .rangePreviousChar : pick == .atCaret ? .rangeAtCaret : .rangeNextChar
            if let r = accept(raw, frame: frame, app: app, source: source) { return (r, source) }
        }
        if let (raw, source) = textMarkerCaret(element), let r = accept(raw, frame: frame, app: app, source: source) {
            return (r, source)
        }
        return nil
    }

    /// Build a snapshot. For secure fields the text is never read (requirement GT-3).
    public static func snapshot(of element: AXUIElement) -> AXSnapshot {
        var bundle = ""
        var name = ""
        let elementPID = pid(of: element) ?? 0
        if elementPID != 0, let app = NSRunningApplication(processIdentifier: elementPID) {
            bundle = app.bundleIdentifier ?? ""
            name = app.localizedName ?? ""
        }
        let role = string(element, kAXRoleAttribute)
        let subrole = string(element, kAXSubroleAttribute)
        let secure = subrole == (kAXSecureTextFieldSubrole as String)
        let editable = role.map { editableRoles.contains($0) } ?? false
        var snap = AXSnapshot(appBundleId: bundle, appName: name, role: role, subrole: subrole,
                              secure: secure, editable: editable, valueLength: nil, beforeCaret: nil,
                              selectionLen: nil, caret: nil, caretSource: nil, elementFrame: nil)
        snap.pid = elementPID
        guard !secure, editable else { return snap }

        snap.windowTitle = focusedWindowTitle(pid: elementPID)
        snap.elementFrame = frame(element)
        if let selection = range(element, kAXSelectedTextRangeAttribute) {
            snap.selectionLen = max(selection.length, 0)
            if let value = string(element, kAXValueAttribute) {
                snap.valueLength = value.utf16.count
                snap.beforeCaret = TextWindow.beforeCaret(value: value, caretUTF16: selection.location)
                snap.afterCaret = TextWindow.afterCaret(value: value, caretUTF16: selection.location,
                                                        selectionUTF16: selection.length)
            }
            let value = string(element, kAXValueAttribute)
            if let (rect, source) = caret(element, selection: selection, value: value, frame: snap.elementFrame, app: bundle) {
                snap.caret = rect
                snap.caretSource = source
            }
        } else if let (rect, source) = textMarkerCaret(element),
                  let r = accept(rect, frame: snap.elementFrame, app: bundle, source: source) {
            snap.caret = r
            snap.caretSource = source
        }
        return snap
    }

    /// Title of the app's focused window (empty titles count as none).
    static func focusedWindowTitle(pid: pid_t) -> String? {
        guard pid != 0 else { return nil }
        let app = AXUIElementCreateApplication(pid)
        guard let window = attribute(app, kAXFocusedWindowAttribute),
              CFGetTypeID(window) == AXUIElementGetTypeID() else { return nil }
        let title = string(window as! AXUIElement, kAXTitleAttribute)  // type id checked above
        return title?.isEmpty == false ? title : nil
    }

    /// Chromium and Electron only expose their accessibility tree after being asked (macos-native steering).
    public static func enableManualAccessibility(pid: pid_t) {
        let app = AXUIElementCreateApplication(pid)
        AXUIElementSetAttributeValue(app, "AXManualAccessibility" as CFString, kCFBooleanTrue)
    }
}
