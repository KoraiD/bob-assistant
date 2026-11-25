// Records the README GIFs: Ghost Text and Selection Actions in a TextEdit window, with real key events.
//
//   bash scripts/record-gifs.sh        (builds this, records, converts with ffmpeg into assets/)
//
// Privacy: it records only the rectangle of its own TextEdit window (never the whole screen), types only
// while its scratch document is frontmost, and stops otherwise. Turn on a Focus mode first so no
// notification lands in the rectangle. Needs Accessibility and Screen Recording for the terminal.
import AppKit
import ApplicationServices
import Carbon.HIToolbox

let outDir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : NSTemporaryDirectory()
let scratch = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("bob-demo.rtf")
/// Where the window goes, in screen points (top-left origin), and so what is recorded.
let frame = CGRect(x: 80, y: 120, width: 980, height: 600)

func attr(_ e: AXUIElement, _ a: String) -> AnyObject? {
    var v: AnyObject?
    return AXUIElementCopyAttributeValue(e, a as CFString, &v) == .success ? v : nil
}
/// TextEdit's focused element (asked of TextEdit itself: the system-wide query can come back empty).
func focused() -> AXUIElement? {
    guard let pid = NSRunningApplication.runningApplications(withBundleIdentifier: "com.apple.TextEdit").first?
        .processIdentifier,
          let f = attr(AXUIElementCreateApplication(pid), kAXFocusedUIElementAttribute) else { return nil }
    return (f as! AXUIElement)
}
/// The key that types `letter` in the current layout (Cmd+Z is the "z" key, wherever it is).
func code(_ letter: String) -> CGKeyCode {
    guard let src = TISCopyCurrentKeyboardLayoutInputSource()?.takeRetainedValue(),
          let raw = TISGetInputSourceProperty(src, kTISPropertyUnicodeKeyLayoutData) else { return 0 }
    let data = Unmanaged<CFData>.fromOpaque(raw).takeUnretainedValue() as Data
    return data.withUnsafeBytes { buf in
        let layout = buf.baseAddress!.assumingMemoryBound(to: UCKeyboardLayout.self)
        for c in 0..<UInt16(128) {
            var dead: UInt32 = 0, len = 0
            var chars = [UniChar](repeating: 0, count: 4)
            if UCKeyTranslate(layout, c, UInt16(kUCKeyActionDown), 0, UInt32(LMGetKbdType()),
                              OptionBits(kUCKeyTranslateNoDeadKeysBit), &dead, 4, &len, &chars) == noErr,
               len == 1, String(utf16CodeUnits: chars, count: 1) == letter { return CGKeyCode(c) }
        }
        return 0
    }
}
func guardFront() {
    let front = NSWorkspace.shared.frontmostApplication?.bundleIdentifier
    guard front == "com.apple.TextEdit", let f = focused(), attr(f, kAXRoleAttribute) as? String == "AXTextArea" else {
        print("stopped: the scratch document is not in front (\(front ?? "nil"))"); exit(2)
    }
}
func text() -> String { focused().flatMap { attr($0, kAXValueAttribute) as? String } ?? "" }
func setText(_ s: String, caret: Int? = nil) {
    guardFront()
    guard let f = focused() else { return }
    AXUIElementSetAttributeValue(f, kAXValueAttribute as CFString, s as CFString)
    var r = CFRange(location: caret ?? (s as NSString).length, length: 0)
    if let v = AXValueCreate(.cfRange, &r) { AXUIElementSetAttributeValue(f, kAXSelectedTextRangeAttribute as CFString, v) }
    Thread.sleep(forTimeInterval: 0.3)
}
func key(_ code: CGKeyCode, flags: CGEventFlags = []) {
    guardFront()
    let src = CGEventSource(stateID: .hidSystemState)
    for down in [true, false] {
        let e = CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: down)!
        e.flags = flags
        e.post(tap: .cghidEventTap)
    }
}
func type(_ s: String, gapMs: UInt32 = 45) {
    let src = CGEventSource(stateID: .hidSystemState)
    for u in Array(s.utf16) {
        guardFront()
        var unit = u
        for down in [true, false] {
            let e = CGEvent(keyboardEventSource: src, virtualKey: 0, keyDown: down)!
            e.keyboardSetUnicodeString(stringLength: 1, unicodeString: &unit)
            e.post(tap: .cghidEventTap)
        }
        usleep(gapMs * 1000)
    }
}
/// BOB's helper windows on screen: the chip (short) and the action menu (340 pt wide).
func waitFor(_ seconds: Double, _ cond: () -> Bool) -> Bool {
    let end = Date().addingTimeInterval(seconds)
    while Date() < end { if cond() { return true }; Thread.sleep(forTimeInterval: 0.05) }
    return cond()
}
func pause(_ s: Double) { Thread.sleep(forTimeInterval: s) }

/// Record `seconds` of the window rectangle into `name`.mov while `body` runs.
func record(_ name: String, seconds: Int, _ body: () -> Void) {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    let rect = "\(Int(frame.minX)),\(Int(frame.minY)),\(Int(frame.width)),\(Int(frame.height))"
    p.arguments = ["-x", "-v", "-V", "\(seconds)", "-R", rect, "\(outDir)/\(name).mov"]
    try! p.run()
    pause(1.2)  // recording starts
    body()
    p.waitUntilExit()
}

// A rich-text scratch document with a large font, so the GIF is readable.
let rtf = "{\\rtf1\\ansi{\\fonttbl\\f0\\fswiss Helvetica;}\\f0\\fs44 \\par}"
try? rtf.write(to: scratch, atomically: true, encoding: .utf8)
let open = Process()
open.executableURL = URL(fileURLWithPath: "/usr/bin/open")
open.arguments = ["-a", "TextEdit", scratch.path]
try! open.run(); open.waitUntilExit()
_ = waitFor(10) {
    if NSWorkspace.shared.frontmostApplication?.bundleIdentifier != "com.apple.TextEdit" {
        NSRunningApplication.runningApplications(withBundleIdentifier: "com.apple.TextEdit").first?
            .activate(options: [.activateAllWindows])
        pause(0.3)
    }
    return NSWorkspace.shared.frontmostApplication?.bundleIdentifier == "com.apple.TextEdit"
        && focused().flatMap { attr($0, kAXRoleAttribute) as? String } == "AXTextArea"
}
pause(1)
guardFront()
// Place the window.
if let pid = NSWorkspace.shared.frontmostApplication?.processIdentifier,
   let w = attr(AXUIElementCreateApplication(pid), kAXFocusedWindowAttribute) {
    let win = w as! AXUIElement
    var origin = frame.origin, size = frame.size
    if let v = AXValueCreate(.cgPoint, &origin) { AXUIElementSetAttributeValue(win, kAXPositionAttribute as CFString, v) }
    if let v = AXValueCreate(.cgSize, &size) { AXUIElementSetAttributeValue(win, kAXSizeAttribute as CFString, v) }
}
pause(0.8)
key(code("a"), flags: .maskCommand)
key(51)  // empty the document, keeping the large font

// 1. Ghost Text: type, pause, Tab takes a word, Shift+Tab the rest. (Rich-text TextEdit fixes typos
// itself, so the typo fix is not shown here; `make e2e` checks it in plain text.)
record("ghost-text", seconds: 13) {
    type("Thanks for the update! I will get back to", gapMs: 70)
    pause(2.5)
    key(48)  // Tab
    pause(1.4)
    key(48, flags: .maskShift)  // Shift+Tab
    pause(2.5)
}
// 2. Selection Actions: select a rough sentence, the menu, Make professional.
key(code("a"), flags: .maskCommand)
key(51)
type("hey, cant make it tmrw, the build is broke again. lets do friday?", gapMs: 8)
pause(0.6)
record("selection-actions", seconds: 13) {
    key(code("a"), flags: .maskCommand)
    pause(0.8)
    key(code("j"), flags: [.maskCommand, .maskShift])
    pause(1.4)
    for _ in 0..<4 { key(125); pause(0.25) }  // Down to Make professional
    pause(0.5)
    key(36)
    pause(7)
}
// Leave TextEdit as we found it.
key(code("a"), flags: .maskCommand)
key(51)
key(code("w"), flags: .maskCommand)
pause(0.8)
// "Delete" in the save sheet, if one is shown (the document is no longer in front, so no guard here).
let src = CGEventSource(stateID: .hidSystemState)
if NSWorkspace.shared.frontmostApplication?.bundleIdentifier == "com.apple.TextEdit" {
    for down in [true, false] {
        let e = CGEvent(keyboardEventSource: src, virtualKey: code("d"), keyDown: down)!
        e.flags = .maskCommand
        e.post(tap: .cghidEventTap)
    }
}
print("recorded into \(outDir)")
