// BOB Test Bench: types like a person into its own editor while a test copy of BOB suggests, and checks
// every step. Started by scripts/test-bench.sh, which runs the test BOB in a separate profile (BOB_HOME).
//
// Safety: it holds your own BOB's instance lock for the whole run (so it refuses to start while your BOB runs,
// and your BOB cannot start during the run) and posts keys only while its own editor has focus and is in front.
// Using the mouse, or another window coming to the front, pauses it; any key press stops it. It never types
// into another app.
import AppKit
import ApplicationServices
import Carbon.HIToolbox
import TestBenchCore

// MARK: options

struct Options {
    var out = URL(fileURLWithPath: "bench-results/local/testbench")
    var corpus = URL(fileURLWithPath: "native/test-bench/corpus.txt")
    var seed: UInt64 = 1
    var minutes = 5.0
    var episodes = 1_000
    var steps = 160
    var profiles: [EditorProfile] = EditorProfile.allCases
    var realLock: String?
    var preflight = false
    /// Start typing only once this file (the test BOB's helper socket) exists.
    var readyFile: String?
    /// Diagnostics: stay in the background, never post keys, and insert words into the editor in code.
    var diag = false
    /// With `--diag`: come to the front, so BOB's helper observes the editor (still no key events).
    var diagActivate = false
    /// With `--diag`: come to the front this many seconds after launch.
    var diagFrontAfter = 0.0
}

func parseOptions() -> Options {
    var o = Options()
    var args = CommandLine.arguments.dropFirst().makeIterator()
    while let a = args.next() {
        let v = { args.next() ?? "" }
        switch a {
        case "--out": o.out = URL(fileURLWithPath: v())
        case "--corpus": o.corpus = URL(fileURLWithPath: v())
        case "--seed": o.seed = UInt64(v()) ?? 1
        case "--minutes": o.minutes = Double(v()) ?? 5
        case "--episodes": o.episodes = Int(v()) ?? 1_000
        case "--steps": o.steps = Int(v()) ?? 160
        case "--profiles": o.profiles = v().split(separator: ",").compactMap { EditorProfile(rawValue: String($0)) }
        case "--real-lock": o.realLock = v()
        case "--preflight": o.preflight = true
        case "--ready-file": o.readyFile = v()
        case "--diag": o.diag = true
        case "--diag-front": o.diag = true; o.diagActivate = true
        case "--diag-front-after": o.diag = true; o.diagFrontAfter = Double(v()) ?? 1.5
        default:
            print("unknown option \(a)")
            exit(64)
        }
    }
    if o.profiles.isEmpty { o.profiles = EditorProfile.allCases }
    return o
}

let options = parseOptions()

// MARK: your BOB's lock

/// Take the single-instance lock of your own BOB (`$TMPDIR/bob-<uid>/bob.lock`). Fails while your BOB runs.
func takeRealLock(_ path: String) -> Int32? {
    let dir = (path as NSString).deletingLastPathComponent
    try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true,
                                             attributes: [.posixPermissions: 0o700])
    let fd = open(path, O_RDWR | O_CREAT, 0o600)
    guard fd >= 0 else { return nil }
    if flock(fd, LOCK_EX | LOCK_NB) != 0 {
        close(fd)
        return nil
    }
    return fd
}

guard let lockPath = options.realLock else {
    print("--real-lock is required (your BOB's bob.lock); use scripts/test-bench.sh")
    exit(64)
}
guard let realLock = takeRealLock(lockPath) else {
    say("Your BOB is running. Quit it from the menu bar (BOB > Quit), then start the Test Bench again.")
    exit(3)
}
if options.preflight {
    close(realLock)
    print("preflight ok: your BOB is not running")
    exit(0)
}
/// Print and keep a copy in `result.txt` (the app is started with `open`, so its output is not on screen).
func say(_ line: String) {
    print(line)
    try? FileManager.default.createDirectory(at: options.out, withIntermediateDirectories: true)
    let url = options.out.appendingPathComponent("result.txt")
    if let h = try? FileHandle(forWritingTo: url) {
        h.seekToEndOfFile()
        h.write(Data((line + "\n").utf8))
        try? h.close()
    } else {
        try? Data((line + "\n").utf8).write(to: url)
    }
}

// MARK: keys

/// Key codes by the character they type in the current layout without modifiers. Built on the main thread
/// (the input source API asserts that it runs there), once before the run.
func layoutKeyCodes() -> [String: CGKeyCode] {
    dispatchPrecondition(condition: .onQueue(.main))
    var table: [String: CGKeyCode] = [" ": CGKeyCode(kVK_Space)]
    guard let src = TISCopyCurrentKeyboardLayoutInputSource()?.takeRetainedValue(),
          let raw = TISGetInputSourceProperty(src, kTISPropertyUnicodeKeyLayoutData) else { return table }
    let data = Unmanaged<CFData>.fromOpaque(raw).takeUnretainedValue() as Data
    data.withUnsafeBytes { buf in
        guard let base = buf.baseAddress else { return }
        let layout = base.assumingMemoryBound(to: UCKeyboardLayout.self)
        for c in 0..<UInt16(128) {
            var dead: UInt32 = 0
            var len = 0
            var chars = [UniChar](repeating: 0, count: 4)
            if UCKeyTranslate(layout, c, UInt16(kUCKeyActionDown), 0, UInt32(LMGetKbdType()),
                              OptionBits(kUCKeyTranslateNoDeadKeysBit), &dead, 4, &len, &chars) == noErr, len == 1 {
                let s = String(utf16CodeUnits: chars, count: 1)
                if table[s] == nil { table[s] = CGKeyCode(c) }
            }
        }
    }
    return table
}

func postKey(code: CGKeyCode, text: String? = nil, flags: CGEventFlags = []) {
    let src = CGEventSource(stateID: .hidSystemState)
    src?.userData = benchEventMarker
    for down in [true, false] {
        guard let e = CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: down) else { continue }
        e.flags = flags
        if let text {
            var units = Array(text.utf16)
            e.keyboardSetUnicodeString(stringLength: units.count, unicodeString: &units)
        }
        e.setIntegerValueField(.eventSourceUserData, value: benchEventMarker)
        e.post(tap: .cghidEventTap)
        if down { usleep(6_000) }
    }
}

// MARK: chip

/// Reads BOB's chip: the helper's small on-screen panel and its label, through Accessibility.
final class ChipReader {
    private var pid: pid_t = 0
    private var app: AXUIElement?
    private var lastLookup = 0.0
    private(set) var unreadable = 0

    private func helper() -> AXUIElement? {
        let now = CACurrentMediaTime()
        if app == nil || now - lastLookup > 2 {
            lastLookup = now
            let found = DispatchQueue.main.sync {
                NSWorkspace.shared.runningApplications.first { $0.executableURL?.lastPathComponent == "bob-native" }?
                    .processIdentifier
            }
            if let found, found != pid {
                pid = found
                let el = AXUIElementCreateApplication(found)
                AXUIElementSetMessagingTimeout(el, 0.15)
                app = el
            } else if found == nil {
                pid = 0
                app = nil
            }
        }
        return app
    }

    /// Whether the helper shows a chip-sized window (not the action menu or the orb).
    private func chipOnScreen() -> Bool {
        guard pid != 0 else { return false }
        let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
        return list.contains { w in
            guard (w[kCGWindowOwnerPID as String] as? Int32) == pid,
                  (w[kCGWindowAlpha as String] as? Double ?? 0) > 0.1,
                  let b = w[kCGWindowBounds as String] as? [String: Double] else { return false }
            return (b["Height"] ?? 0) < 60 && (b["Width"] ?? 0) < 330 && (b["Width"] ?? 0) > 4
        }
    }

    private func attr(_ e: AXUIElement, _ name: String) -> AnyObject? {
        var v: AnyObject?
        return AXUIElementCopyAttributeValue(e, name as CFString, &v) == .success ? v : nil
    }

    private func label(in e: AXUIElement, depth: Int) -> String? {
        if attr(e, kAXRoleAttribute) as? String == kAXStaticTextRole, let s = attr(e, kAXValueAttribute) as? String,
           !s.isEmpty {
            return s
        }
        guard depth > 0, let kids = attr(e, kAXChildrenAttribute) as? [AXUIElement] else { return nil }
        for k in kids { if let s = label(in: k, depth: depth - 1) { return s } }
        return nil
    }

    func read() -> String? {
        guard let app = helper(), chipOnScreen() else { return nil }
        let windows = attr(app, kAXWindowsAttribute) as? [AXUIElement] ?? []
        for w in windows {
            var size = CGSize.zero
            if let v = attr(w, kAXSizeAttribute), CFGetTypeID(v) == AXValueGetTypeID() {
                AXValueGetValue(v as! AXValue, .cgSize, &size)  // swiftlint:disable:this force_cast
            }
            guard size.height < 60, size.width < 330 else { continue }
            if let s = label(in: w, depth: 4) { return s }
        }
        unreadable += 1
        return nil
    }
}

// MARK: app

final class BenchTextView: NSTextView {
    /// Shift+Tab would move focus out of the editor; keep it here (BOB takes it when a chip shows).
    override func insertBacktab(_ sender: Any?) {}

    /// The `late` profile: Accessibility answers with the text and selection as they were this long ago.
    var lateMs = 0.0
    private var history: [(ms: Double, text: String, selection: NSRange)] = []

    private func record() {
        guard lateMs > 0 else { return }
        let now = CACurrentMediaTime() * 1000
        history.append((now, string, selectedRange()))
        // Keep two seconds, and always the newest entry older than the delay.
        if let keep = history.lastIndex(where: { $0.ms <= now - 2_000 }) { history.removeFirst(keep) }
    }

    func resetHistory() {
        history = []
        record()
    }

    override func didChangeText() {
        super.didChangeText()
        record()
    }

    override func setSelectedRanges(_ ranges: [NSValue], affinity: NSSelectionAffinity,
                                    stillSelecting stillSelectingFlag: Bool) {
        super.setSelectedRanges(ranges, affinity: affinity, stillSelecting: stillSelectingFlag)
        record()
    }

    private var lateSnapshot: (text: String, selection: NSRange)? {
        guard lateMs > 0, let first = history.first else { return nil }
        let cutoff = CACurrentMediaTime() * 1000 - lateMs
        let entry = history.last(where: { $0.ms <= cutoff }) ?? first
        return (entry.text, entry.selection)
    }

    override func accessibilityValue() -> String? {
        lateSnapshot?.text ?? super.accessibilityValue()
    }

    override func accessibilitySelectedTextRange() -> NSRange {
        lateSnapshot?.selection ?? super.accessibilitySelectedTextRange()
    }
}

final class Bench: NSObject, NSApplicationDelegate, BenchIO {
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 860, height: 560),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
    let status = NSTextField(labelWithString: "")
    let scroll = BenchTextView.scrollableTextView()
    var editor: BenchTextView { scroll.documentView as! BenchTextView }  // swiftlint:disable:this force_cast
    let chips = ChipReader()
    let lock = NSLock()
    var foreign: String?
    var lastForeign = CACurrentMediaTime()
    var lastMouse = 0.0
    /// Pauses so far, and paused time (not counted against the time limit).
    var resumeCount = 0
    var pausedMs = 0.0
    /// A pause longer than this ends the run.
    let maxPauseMs = 15.0 * 60_000
    var startedAt: Double?
    var tap: CFMachPort?
    var findingsFile: FileHandle?
    var runner: Runner?
    let limitMs = options.minutes * 60_000
    let lockFD: Int32
    var keyCodes: [String: CGKeyCode] = [:]

    init(lockFD: Int32) {
        self.lockFD = lockFD
    }

    func applicationDidFinishLaunching(_ note: Notification) {
        NSApp.setActivationPolicy(.regular)
        let bg = NSColor(srgbRed: 0x0C / 255, green: 0x0B / 255, blue: 0x16 / 255, alpha: 1)
        let surface = NSColor(srgbRed: 0x15 / 255, green: 0x13 / 255, blue: 0x2A / 255, alpha: 1)
        let accent = NSColor(srgbRed: 0x8A / 255, green: 0xFF / 255, blue: 0xC1 / 255, alpha: 1)
        window.title = "BOB Test Bench"
        window.backgroundColor = bg
        window.isReleasedWhenClosed = false
        status.textColor = accent
        status.font = .systemFont(ofSize: 13, weight: .medium)
        status.frame = NSRect(x: 16, y: 520, width: 828, height: 24)
        status.autoresizingMask = [.width, .minYMargin]
        scroll.frame = NSRect(x: 16, y: 16, width: 828, height: 496)
        scroll.autoresizingMask = [.width, .height]
        editor.font = .systemFont(ofSize: 17)
        editor.backgroundColor = surface
        editor.textColor = NSColor(white: 0.93, alpha: 1)
        editor.insertionPointColor = accent
        editor.textContainerInset = NSSize(width: 10, height: 10)
        window.contentView?.addSubview(status)
        window.contentView?.addSubview(scroll)
        window.center()
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(editor)
        configure(.plain)
        if options.diag {
            if options.diagActivate { NSApp.activate() }
            if options.diagFrontAfter > 0 {
                DispatchQueue.main.asyncAfter(deadline: .now() + options.diagFrontAfter) { NSApp.activate() }
            }
            return runDiagnostics()
        }
        NSApp.activate()
        waitForAccessibility()
    }

    /// Insert a word every 300 ms for 10 s through the editor's own editing path (as typing would), without
    /// key events or activation, so another process can watch the editor through Accessibility.
    func runDiagnostics() {
        configure(options.profiles.count == 1 ? options.profiles[0] : .plain)
        editor.resetHistory()
        if editor.lateMs > 0 { say(String(format: "diag: late editor, %.0f ms", editor.lateMs)) }
        var n = 0
        Timer.scheduledTimer(withTimeInterval: 0.3, repeats: true) { [weak self] t in
            guard let self else { return }
            n += 1
            self.editor.insertText("word\(n) ", replacementRange: self.editor.selectedRange())
            if n % 5 == 1 {
                let front = NSWorkspace.shared.frontmostApplication?.bundleIdentifier ?? "none"
                say("diag: word \(n), frontmost \(front), key window \(self.window.isKeyWindow)")
            }
            if n >= 33 {
                t.invalidate()
                say("diag: inserted \(n) words, editor has \(self.editor.string.count) characters")
                self.finish(code: 0)
            }
        }
    }

    /// Ask macOS for Accessibility once (the system prompt adds "BOB Test Bench" to the list), then continue
    /// as soon as it is switched on.
    func waitForAccessibility() {
        let prompt = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
        if AXIsProcessTrustedWithOptions(prompt) { return armed() }
        let asked = CACurrentMediaTime()
        status.stringValue = "BOB Test Bench needs Accessibility to type into this window and read BOB's chip. "
            + "Switch it on in System Settings > Privacy & Security > Accessibility; the run continues by itself."
        Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] t in
            if AXIsProcessTrusted() {
                t.invalidate()
                self?.armed()
            } else if CACurrentMediaTime() - asked > 180 {
                t.invalidate()
                say("Accessibility was not switched on for BOB Test Bench within 3 minutes. Nothing was typed.")
                self?.finish(code: 4)
            }
        }
    }

    func armed() {
        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
        startMonitor()
        Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] t in
            if self?.tryStart() == true { t.invalidate() }
        }
        Timer.scheduledTimer(withTimeInterval: 180, repeats: false) { [weak self] _ in
            guard self?.startedAt == nil else { return }
            say("The run did not start within 3 minutes (the Test Bench window needs focus and quiet). Nothing was typed.")
            self?.finish(code: 5)
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ app: NSApplication) -> Bool { true }

    func configure(_ p: EditorProfile) {
        let smart = p == .smart
        editor.lateMs = p == .late ? Double.random(in: 60...250) : 0
        editor.isRichText = false
        editor.isAutomaticQuoteSubstitutionEnabled = smart
        editor.isAutomaticDashSubstitutionEnabled = smart
        editor.isAutomaticSpellingCorrectionEnabled = false
        editor.isAutomaticTextReplacementEnabled = false
        editor.isContinuousSpellCheckingEnabled = false
        editor.isAutomaticTextCompletionEnabled = false
        editor.inlinePredictionType = .no
    }

    // Input monitor: any key or mouse event that the Test Bench or BOB did not post stops the run.
    func startMonitor() {
        let types: [CGEventType] = [.keyDown, .keyUp, .flagsChanged, .mouseMoved, .leftMouseDown, .rightMouseDown,
                                    .otherMouseDown, .scrollWheel, .leftMouseDragged]
        let mask = types.reduce(CGEventMask(0)) { $0 | (1 << $1.rawValue) }
        let me = Unmanaged.passUnretained(self).toOpaque()
        tap = CGEvent.tapCreate(tap: .cgSessionEventTap, place: .headInsertEventTap, options: .listenOnly,
                                eventsOfInterest: mask, callback: { _, type, event, ctx in
            guard let ctx else { return Unmanaged.passUnretained(event) }
            let bench = Unmanaged<Bench>.fromOpaque(ctx).takeUnretainedValue()
            if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
                if let t = bench.tap { CGEvent.tapEnable(tap: t, enable: true) }
                return Unmanaged.passUnretained(event)
            }
            let isKey = type == .keyDown || type == .keyUp || type == .flagsChanged
            if isKey, isOwnEvent(userData: event.getIntegerValueField(.eventSourceUserData)) {
                return Unmanaged.passUnretained(event)
            }
            bench.sawForeign(isKey ? "keyboard" : "mouse")
            return Unmanaged.passUnretained(event)
        }, userInfo: me)
        guard let tap else {
            say("Cannot watch the keyboard and mouse (Accessibility), so the run is refused. Nothing was typed.")
            finish(code: 4)
            return
        }
        let src = CFMachPortCreateRunLoopSource(nil, tap, 0)
        CFRunLoopAddSource(CFRunLoopGetMain(), src, .commonModes)
        CGEvent.tapEnable(tap: tap, enable: true)
    }

    /// Keys from anyone else stop the run; the mouse only pauses it.
    func sawForeign(_ what: String) {
        lock.withLock {
            let now = CACurrentMediaTime()
            lastForeign = now
            if what == "mouse" {
                lastMouse = now
            } else if startedAt != nil, foreign == nil {
                foreign = what
            }
        }
    }

    var focused: Bool {
        NSWorkspace.shared.frontmostApplication?.processIdentifier == ProcessInfo.processInfo.processIdentifier
            && window.isKeyWindow && window.firstResponder === editor
    }

    var readySince: Double?

    /// Start once the test BOB is up, the editor has focus and nobody touched the mouse or keyboard for 3 s.
    func tryStart() -> Bool {
        if let ready = options.readyFile {
            guard FileManager.default.fileExists(atPath: ready) else {
                status.stringValue = "Waiting for the test BOB to start…"
                return false
            }
            if readySince == nil { readySince = CACurrentMediaTime() }
        }
        let quiet = lock.withLock { CACurrentMediaTime() - lastForeign }
        if let since = readySince, CACurrentMediaTime() - since < 3 {
            status.stringValue = "The test BOB is starting…"
            return false
        }
        guard focused else {
            status.stringValue = "Click this window to hand BOB's Test Bench the keyboard."
            return false
        }
        guard quiet >= 3 else {
            status.stringValue = String(format: "Hands off the mouse and keyboard: starting in %.0f s.",
                                        max(1, 3 - quiet))
            return false
        }
        lock.withLock { startedAt = CACurrentMediaTime(); foreign = nil }
        status.stringValue = "Typing. Moving the mouse pauses; any key stops the run."
        begin()
        return true
    }

    func begin() {
        keyCodes = layoutKeyCodes()
        try? FileManager.default.createDirectory(at: options.out, withIntermediateDirectories: true)
        let findingsURL = options.out.appendingPathComponent("findings.jsonl")
        FileManager.default.createFile(atPath: findingsURL.path, contents: nil)
        findingsFile = try? FileHandle(forWritingTo: findingsURL)
        let corpus = ((try? String(contentsOf: options.corpus, encoding: .utf8)) ?? "")
            .split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        let runner = Runner(io: self, seed: options.seed, corpus: corpus)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        runner.onFinding = { [weak self] f in
            guard let data = try? encoder.encode(f) else { return }
            self?.findingsFile?.write(data + Data("\n".utf8))
            DispatchQueue.main.async { self?.status.stringValue = "Typing. Last finding: \(f.kind.rawValue)" }
        }
        self.runner = runner
        Thread.detachNewThread { [self] in
            runner.run(episodes: options.episodes, stepsPerEpisode: options.steps, profiles: options.profiles,
                       seed: options.seed)
            DispatchQueue.main.async { self.finish(code: 0) }
        }
    }

    func finish(code: Int32) {
        if let runner {
            var s = runner.summary
            if s.stoppedBecause == nil { s.stoppedBecause = "all episodes done" }
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            if let data = try? encoder.encode(s) {
                try? data.write(to: options.out.appendingPathComponent("summary.json"))
            }
            say(String(format: "episodes %d, keys %d, suggestions %d, accepts %d (%d as expected), findings %d "
                + "(%.1f per 1000 keys), chip unreadable %d times, pauses %d (%.0f s). Stopped: %@",
                s.episodes, s.keys, s.suggestions, s.accepts, s.acceptsOK, runner.findings.count,
                s.defectsPer1000Keys, chips.unreadable, resumes, lock.withLock { pausedMs } / 1000,
                s.stoppedBecause ?? ""))
            for (k, n) in s.findings.sorted(by: { $0.key < $1.key }) { say("  \(k): \(n)") }
        }
        try? findingsFile?.close()
        try? Data("\(code)\n".utf8).write(to: options.out.appendingPathComponent("exit-code"))
        close(lockFD)
        exit(code)
    }

    // MARK: BenchIO

    func nowMs() -> Double { ((lock.withLock { startedAt }).map { CACurrentMediaTime() - $0 } ?? 0) * 1000 }
    func sleep(ms: Int) { usleep(useconds_t(max(0, ms)) * 1000) }
    func text() -> String { DispatchQueue.main.sync { editor.string } }
    func chip() -> String? { chips.read() }

    func reset(profile: EditorProfile) {
        DispatchQueue.main.sync {
            configure(profile)
            editor.string = ""
            editor.resetHistory()
            window.makeFirstResponder(nil)  // focus moves into the editor, like a click
            window.makeFirstResponder(editor)
            let late = profile == .late ? String(format: ", reports %.0f ms late", editor.lateMs) : ""
            status.stringValue = "Typing (\(profile.rawValue) editor\(late)). Moving the mouse pauses; any key stops the run."
        }
    }

    var resumes: Int { lock.withLock { resumeCount } }

    /// Typing time so far (paused time does not count against the limit).
    func activeMs() -> Double { nowMs() - lock.withLock { pausedMs } }

    /// Nil while typing may go on. Blocks while paused (mouse in use in the last 3 s, or the editor not in
    /// front with focus); returns a reason to stop for a key press, the time limit or a very long pause.
    func stopReason() -> String? {
        var pauseStart: Double?
        while true {
            let (key, mouseIdle) = lock.withLock { (foreign != nil, CACurrentMediaTime() - lastMouse) }
            let inFront = DispatchQueue.main.sync { focused }
            let state = GuardState(editorInFront: inFront, mouseIdleS: mouseIdle, foreignKey: key,
                                   activeMs: activeMs(), limitMs: limitMs,
                                   pausedMs: pauseStart.map { nowMs() - $0 } ?? 0, maxPauseMs: maxPauseMs)
            switch decide(state) {
            case let .stop(reason):
                return reason
            case .type:
                guard let start = pauseStart else { return nil }
                let paused = nowMs() - start
                DispatchQueue.main.sync {
                    editor.setSelectedRange(NSRange(location: (editor.string as NSString).length, length: 0))
                    status.stringValue = "Typing. Moving the mouse pauses; any key stops the run."
                }
                lock.withLock {
                    pausedMs += paused
                    resumeCount += 1
                }
                return nil
            case .pause:
                if pauseStart == nil {
                    pauseStart = nowMs()
                    DispatchQueue.main.async { [self] in
                        status.stringValue = "Paused. Typing goes on 3 s after the mouse rests with this window in "
                            + "front (click it). Any key stops the run."
                    }
                }
                usleep(100_000)
            }
        }
    }

    func post(_ action: Action) -> String? {
        if let r = stopReason() { return r }
        switch action {
        case let .type(c):
            let code = keyCodes[String(c).lowercased()] ?? 0
            postKey(code: code, text: String(c), flags: c.isUppercase ? .maskShift : [])
        case .backspace: postKey(code: CGKeyCode(kVK_Delete))
        case .accept(.tab): postKey(code: CGKeyCode(kVK_Tab))
        case .accept(.shiftTab): postKey(code: CGKeyCode(kVK_Tab), flags: .maskShift)
        case .escape: postKey(code: CGKeyCode(kVK_Escape))
        case .pause: break
        }
        return nil
    }
}

let app = NSApplication.shared
let bench = Bench(lockFD: realLock)
app.delegate = bench
app.run()
