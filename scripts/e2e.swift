// End-to-end check of a running BOB against TextEdit, with real key events.
//
//   swiftc -O scripts/e2e.swift -o /tmp/bob-e2e && /tmp/bob-e2e
//
// Safety: it opens its own scratch file in TextEdit and types only while that document is frontmost; it
// stops as soon as anything else is in front. Needs Accessibility for the app that runs it (the terminal).
// Reads text back through Accessibility, so no Automation permission is needed.
import AppKit
import ApplicationServices
import Carbon.HIToolbox

let scratch = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("bob-e2e.txt")
var failures = 0

func attr(_ e: AXUIElement, _ a: String) -> AnyObject? {
    var v: AnyObject?
    return AXUIElementCopyAttributeValue(e, a as CFString, &v) == .success ? v : nil
}
func focused() -> AXUIElement? {
    guard let f = attr(AXUIElementCreateSystemWide(), kAXFocusedUIElementAttribute) else { return nil }
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
func bobWindows() -> (chip: Bool, menu: Bool) {
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
    var chip = false, menu = false
    for w in list where ["bob-native", "BOB Helper"].contains(w[kCGWindowOwnerName as String] as? String ?? "") {
        let alpha = w[kCGWindowAlpha as String] as? Double ?? 0
        guard alpha > 0.1, let b = w[kCGWindowBounds as String] as? [String: Double] else { continue }
        if (b["Width"] ?? 0) >= 330 { menu = true } else if (b["Height"] ?? 0) < 60 { chip = true }
    }
    return (chip, menu)
}
func waitFor(_ seconds: Double, _ cond: () -> Bool) -> Bool {
    let end = Date().addingTimeInterval(seconds)
    while Date() < end { if cond() { return true }; Thread.sleep(forTimeInterval: 0.05) }
    return cond()
}
func check(_ name: String, _ ok: Bool, _ detail: String = "") {
    print("\(ok ? "PASS" : "FAIL")  \(name)\(detail.isEmpty ? "" : "  (\(detail))")")
    if !ok { failures += 1 }
}

// Open the scratch document.
try? "".write(to: scratch, atomically: true, encoding: .utf8)
let open = Process()
open.executableURL = URL(fileURLWithPath: "/usr/bin/open")
open.arguments = ["-a", "TextEdit", scratch.path]
try! open.run(); open.waitUntilExit()
_ = waitFor(5) {
    NSRunningApplication.runningApplications(withBundleIdentifier: "com.apple.TextEdit").first?.activate()
    return NSWorkspace.shared.frontmostApplication?.bundleIdentifier == "com.apple.TextEdit" && focused() != nil
}
Thread.sleep(forTimeInterval: 1)
guardFront()

// 1. Ghost Text: a suggestion appears after a pause, Tab inserts a word, the chip follows typing.
setText("")
type("Thanks for your email, I will get back to")
let shown = waitFor(2) { bobWindows().chip }
check("Ghost Text shows a suggestion after a pause", shown)
let before = text()
key(48)  // Tab
let grew = waitFor(1.5) { text().count > before.count }
check("Tab inserts the next word", grew, "\"\(text().suffix(30))\"")
type(" x")  // diverge on purpose
Thread.sleep(forTimeInterval: 0.15)
let stale = bobWindows().chip
check("typing something else removes the old suggestion at once", !stale)

// 2. Typo fix. The text is set through Accessibility: typing the space would let TextEdit's own
// autocorrection fix the word first.
setText("We will recieve ")
let typoShown = waitFor(2) { bobWindows().chip }
check("a typo fix is offered", typoShown)
key(48)
let fixed = waitFor(1.5) { text() == "We will receive " }
check("Tab replaces the misspelled word", fixed, "\"\(text())\"")

// 3. Mentions belong to the app.
setText("")
type("Thanks @an")
Thread.sleep(forTimeInterval: 0.8)
check("no suggestion while typing a @mention", !bobWindows().chip)

// 4. Mid-sentence.
let sentence = "Could we move the  to Thursday afternoon?"
setText(sentence, caret: ("Could we move the " as NSString).length)
type("m")
let gap = waitFor(2.5) { bobWindows().chip }
check("a suggestion appears inside a sentence", gap)
key(48)
Thread.sleep(forTimeInterval: 0.6)
let filled = text()
check("the gap is filled without repeating the next words", filled.hasSuffix(" to Thursday afternoon?") &&
      !filled.contains("Thursday afternoon? to") && filled.count > sentence.count + 1, "\"\(filled)\"")

// 5. Switching windows inside the same app removes the suggestion (owner report: it stayed frozen).
setText("")
type("Thanks for your email, I will get back to")
_ = waitFor(2) { bobWindows().chip }
let second = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("bob-e2e-2.txt")
try? "".write(to: second, atomically: true, encoding: .utf8)
let open2 = Process()
open2.executableURL = URL(fileURLWithPath: "/usr/bin/open")
open2.arguments = ["-a", "TextEdit", second.path]
try! open2.run(); open2.waitUntilExit()
let gone = waitFor(1.5) { !bobWindows().chip }
check("switching to another window hides the suggestion", gone)
Thread.sleep(forTimeInterval: 0.5)
key(code("w"), flags: .maskCommand)  // close the second window, back to the first
Thread.sleep(forTimeInterval: 0.8)

// 5. Selection Actions: Fix grammar replaces the selection.
setText("teh results was realy good")
key(code("a"), flags: .maskCommand)  // Cmd+A
Thread.sleep(forTimeInterval: 0.3)
key(code("j"), flags: [.maskCommand, .maskShift])  // Cmd+Shift+J
let menu = waitFor(2) { bobWindows().menu }
check("the action menu opens", menu)
key(36)  // Enter: Fix grammar
let rewritten = waitFor(15) { text() != "teh results was realy good" && !text().isEmpty }
check("Fix grammar replaces the selection", rewritten, "\"\(text())\"")
Thread.sleep(forTimeInterval: 1.5)
key(code("z"), flags: .maskCommand)  // Cmd+Z
let undone = waitFor(2) { text() == "teh results was realy good" }
check("Cmd+Z restores the original in one step", undone, "\"\(text())\"")

// Leave TextEdit as we found it: close the scratch document without saving.
setText("")
key(code("w"), flags: .maskCommand)  // Cmd+W
print(failures == 0 ? "\nall checks passed" : "\n\(failures) check(s) failed")
exit(failures == 0 ? 0 : 1)
