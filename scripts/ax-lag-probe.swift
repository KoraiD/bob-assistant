// Measures how Chrome's Accessibility value and caret bounds lag behind typed keys.
// Types only into a page it opened itself, and aborts if focus is anywhere else.
import AppKit
import ApplicationServices

func attr(_ e: AXUIElement, _ a: String) -> AnyObject? {
    var v: AnyObject?
    return AXUIElementCopyAttributeValue(e, a as CFString, &v) == .success ? v : nil
}
func focused() -> AXUIElement? {
    let sys = AXUIElementCreateSystemWide()
    guard let f = attr(sys, kAXFocusedUIElementAttribute) else { return nil }
    return (f as! AXUIElement)
}
struct Read { let before: String; let caretX: Double; let ms: Double }
func read(_ e: AXUIElement, t0: Date) -> Read? {
    guard let value = attr(e, kAXValueAttribute) as? String,
          let r = attr(e, kAXSelectedTextRangeAttribute) else { return nil }
    var range = CFRange()
    AXValueGetValue(r as! AXValue, .cfRange, &range)
    let ns = value as NSString
    let loc = min(max(0, range.location), ns.length)
    let before = ns.substring(to: loc)
    var x = -1.0
    var caretRange = CFRange(location: max(0, loc - 1), length: loc > 0 ? 1 : 0)
    if let axr = AXValueCreate(.cfRange, &caretRange) {
        var b: AnyObject?
        if AXUIElementCopyParameterizedAttributeValue(e, kAXBoundsForRangeParameterizedAttribute as CFString, axr, &b) == .success {
            var rect = CGRect.zero
            AXValueGetValue(b as! AXValue, .cgRect, &rect)
            x = rect.maxX
        }
    }
    return Read(before: before, caretX: x, ms: Date().timeIntervalSince(t0) * 1000)
}
func type(_ s: String) {
    let src = CGEventSource(stateID: .hidSystemState)
    for unit in Array(s.utf16) {
        var u = unit
        let down = CGEvent(keyboardEventSource: src, virtualKey: 0, keyDown: true)!
        down.keyboardSetUnicodeString(stringLength: 1, unicodeString: &u)
        let up = CGEvent(keyboardEventSource: src, virtualKey: 0, keyDown: false)!
        up.keyboardSetUnicodeString(stringLength: 1, unicodeString: &u)
        down.post(tap: .cghidEventTap); up.post(tap: .cghidEventTap)
    }
}

let kind = CommandLine.arguments.dropFirst().first ?? "contenteditable"
let field = kind == "textarea"
    ? "<textarea id=e style='font-size:20px;width:600px;height:80px'>BOBPROBE </textarea>"
    : "<div contenteditable id=e style='font-size:20px;border:1px solid #888;padding:8px'>BOBPROBE&nbsp;</div>"
let html = "<html><body>\(field)<script>const e=document.getElementById('e');e.focus();const s=getSelection();if(e.tagName==='TEXTAREA'){e.selectionStart=e.selectionEnd=e.value.length}else{s.selectAllChildren(e);s.collapseToEnd()}</script></body></html>"
let url = "data:text/html," + html.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed)!
let p = Process(); p.executableURL = URL(fileURLWithPath: "/usr/bin/open"); p.arguments = ["-a", "Google Chrome", url]
try! p.run(); p.waitUntilExit()
Thread.sleep(forTimeInterval: 2.5)
if let chrome = NSRunningApplication.runningApplications(withBundleIdentifier: "com.google.Chrome").first {
    let app = AXUIElementCreateApplication(chrome.processIdentifier)
    AXUIElementSetAttributeValue(app, "AXManualAccessibility" as CFString, kCFBooleanTrue)
    AXUIElementSetAttributeValue(app, "AXEnhancedUserInterface" as CFString, kCFBooleanTrue)
}
Thread.sleep(forTimeInterval: 1.0)
guard NSWorkspace.shared.frontmostApplication?.bundleIdentifier == "com.google.Chrome",
      let e = focused(), let v = attr(e, kAXValueAttribute) as? String, v.contains("BOBPROBE") else {
    print("abort: the probe page is not focused"); exit(2)
}
let text = "Investigate the possibility"
var staleCaret = 0, nbsp = 0, lags: [Double] = [], caretLags: [Double] = []
for ch in text {
    guard let f = focused(), let v = attr(f, kAXValueAttribute) as? String, v.contains("BOBPROBE") else {
        print("abort: focus moved"); exit(3)
    }
    let start = read(f, t0: Date())!
    let t0 = Date()
    type(String(ch))
    var textAt: Double?, caretAt: Double?, sawStale = false
    while Date().timeIntervalSince(t0) < 0.5 {
        guard let r = read(f, t0: t0) else { break }
        let textNew = r.before.count > start.before.count
        let caretNew = abs(r.caretX - start.caretX) > 0.5
        if textNew && textAt == nil { textAt = r.ms }
        if caretNew && caretAt == nil { caretAt = r.ms }
        if caretNew && !textNew { sawStale = true }
        if textAt != nil && caretAt != nil { if r.before.hasSuffix("\u{a0}") { nbsp += 1 }; break }
        usleep(1000)
    }
    if sawStale { staleCaret += 1 }
    lags.append(textAt ?? 500); caretLags.append(caretAt ?? 500)
    print(String(format: "%@  text +%.1f ms  caret +%.1f ms%@", String(ch) == " " ? "␠" : String(ch), textAt ?? -1, caretAt ?? -1, sawStale ? "  STALE (caret moved before text)" : ""))
    usleep(60_000)
}
let sorted = lags.sorted()
print(String(format: "\n%@: %d keys, text lag p50 %.1f ms, max %.1f ms; caret-before-text %d; trailing nbsp %d",
             kind, text.count, sorted[sorted.count / 2], sorted.last!, staleCaret, nbsp))
