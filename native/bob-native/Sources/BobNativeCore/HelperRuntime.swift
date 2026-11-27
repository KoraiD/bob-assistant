import AppKit
import Foundation

public enum HelperMode {
    /// Normal operation: connect to the core at this socket path.
    case connect(socketPath: String)
    /// Compatibility spike: observe the focused field for `seconds` and print per-event findings.
    case probe(seconds: Double)
    /// Standalone demo: show a ghost chip at the caret in any app.
}

/// Wires the observer, the chip and the socket together. Not unit-tested (needs a window server and
/// permissions); its pieces are tested individually and the whole is covered by the manual matrix.
/// State is only touched on the main thread; background callbacks (transcriber, capture) hop back with
/// `DispatchQueue.main.async`, hence `@unchecked Sendable`.
public final class HelperRuntime: @unchecked Sendable {
    private let mode: HelperMode
    private let observer = FocusObserver()
    private let chip = ChipPanel()
    private let keyTap = KeyTap()
    private let screen = ScreenContext()
    private let hotkeys = HotkeyCenter()
    private let menu = ActionMenuPanel()
    /// Copied text for the optional clipboard context (off unless the core turns it on).
    private let clipboardWatcher = ClipboardWatcher()
    /// The app whose selection the menu is working on (SA-7: replacement only goes there).
    private var selectionPID: pid_t = 0
    // Voice Dictation: audio and transcription stay in this process (VD-8).
    private let orb = OrbPanel()
    private let recorder = AudioRecorder(testAudioFile: TestHooks.audioFile)
    /// The chip text on screen; written on the main thread, read by the key tap's thread (code review GT-14).
    private let shownChip = Locked<String?>(nil)
    private let transcriber = Transcriber()
    private var dictationModel: String?
    private var dictationLanguage = ""
    /// Vocabulary hint passed to whisper (custom words).
    private var dictationPrompt: String?
    /// Last known caret (AX coordinates), to place the orb.
    private var lastCaret: Rect?
    private var client: SocketClient?
    private var lastAccessibility = false
    private var lastScreenRecording = false
    /// Last `context_state` sent, so unchanged surroundings are not re-sent on every keystroke.
    private var lastContext: NativeEvent?
    private var permissionTimer: Timer?
    /// The text last checked for a typo (re-reads of the same text are not checked again).
    private var lastTypoCheck = ""

    public init(mode: HelperMode) { self.mode = mode }

    /// Never returns: runs the AppKit main loop.
    public func run() -> Never {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)  // no Dock icon, never steals focus

        switch mode {
        case let .connect(path): startConnected(path: path)
        case let .probe(seconds): startProbe(seconds: seconds)
        }
        app.run()
        exit(0)
    }

    // MARK: connected mode

    private func startConnected(path: String) {
        do {
            let client = try SocketClient.connect(path: path)
            self.client = client
            client.startReading(
                onCommand: { [weak self] cmd in DispatchQueue.main.async { self?.handle(cmd) } },
                onClose: { [weak self] in self?.exitCleanly(0) ?? exit(0) }  // the core went away (NB-5)
            )
            try client.send(NativeEvent.hello(version: BobNativeInfo.protocolVersion,
                                              pid: Int(ProcessInfo.processInfo.processIdentifier)))
        } catch {
            FileHandle.standardError.write(Data("bob-native: cannot connect to core: \(error)\n".utf8))
            exit(2)
        }
    }

    private func handle(_ command: CoreCommand) {
        switch command {
        case .helloAck:
            sendPermissions()
            startObserving()
            // Re-check permissions periodically: the user may grant them while we run (NB-6).
            permissionTimer = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
                self?.permissionTick()
            }
        case let .reject(reason):
            FileHandle.standardError.write(Data("bob-native: rejected by core: \(reason)\n".utf8))
            exit(3)
        case let .showChip(text, anchor):
            chip.show(text: text, caretAX: anchor)
            shownChip.value = chip.isVisible ? text : nil
        case .hideChip:
            chip.hide()
            shownChip.value = nil
        case let .insertText(text):
            // VD-9 / GT-3: never type into a secure field, whoever asked. Fails closed (code review SEC-09).
            if let refusal = InsertGuard.check() {
                try? client?.send(NativeEvent.insertRefused(reason: refusal.rawValue))
                break
            }
            TextInserter.insert(text)
        case let .captureKeys(active):
            keyTap.isCapturing = active
            // While a suggestion is visible, re-check the field a few times a second: some apps (Chrome,
            // Slack) skip change notifications, which left a stale or frozen chip.
            observer.setWatching(active)
            if !active {
                chip.hide()
                shownChip.value = nil
            }
        case .requestPermissions:
            _ = Permissions.accessibilityTrusted(prompt: true)
            Permissions.requestMicrophoneIfPossible { [weak self] _ in
                DispatchQueue.main.async { self?.sendPermissions() }
            }
            sendPermissions()
        case let .setContextMode(scope, exclude, clipboard):
            screen.configure(scope: scope, exclude: exclude)
            clipboardWatcher.exclude = Set(exclude)
            clipboardWatcher.setEnabled(clipboard)
        case .requestScreenRecording:
            ScreenContext.requestPermission()
            sendPermissions()
        case let .showMenu(title, items, anchor, preview):
            menu.showMenu(title: title, items: items, anchorAX: anchor, preview: preview)
            keyTap.isMenuOpen = true
        case let .menuStatus(text, busy, error):
            keyTap.isMenuOpen = false
            menu.showStatus(text, busy: busy, error: error)
        case .hideMenu:
            keyTap.isMenuOpen = false
            menu.hideMenu()
        case let .replaceSelection(expected, text):
            SelectionIO.replace(expected: expected, text: text, pid: selectionPID) { [weak self] ok, method, error in
                try? self?.client?.send(NativeEvent.replaceResult(ok: ok, method: method, error: error))
            }
        case let .replaceBeforeCaret(expected, text):
            // Only if the text before the caret is still what the fix was made for, and never in a password field.
            if let refusal = InsertGuard.check() {
                try? client?.send(NativeEvent.insertRefused(reason: refusal.rawValue))
                break
            }
            guard let focused = AXReader.focusedElement() else { break }
            let snap = AXReader.snapshot(of: focused)
            guard !snap.secure, (snap.beforeCaret ?? "").replacingOccurrences(of: "\u{a0}", with: " ").hasSuffix(expected) else { break }
            TextInserter.deleteBackward(expected.count)
            TextInserter.insert(text)
        case let .copyText(text):
            ClipboardWatcher.ignoreChanges(for: 2)
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)
        case let .setDictation(hotkey, modelPath, language, prompt, sounds):
            dictationPrompt = prompt
            DictationSounds.enabled = sounds
            configureDictation(hotkey: hotkey, modelPath: modelPath, language: language)
        case .startRecording:
            startRecording()
        case let .stopRecording(transcribe):
            stopRecording(transcribe: transcribe)
        case let .orb(state, text):
            orb.show(state, text: text, caretAX: lastCaret)
        case let .setHotkey(hotkey):
            if !hotkeys.register(hotkey) {
                try? client?.send(NativeEvent.error(message: "hotkey unavailable"))
                hotkeys.register(HotkeyCenter.defaultHotkey)
            }
        case let .setActionHotkeys(list):
            let failed = hotkeys.registerActions(list)
            if !failed.isEmpty {
                try? client?.send(NativeEvent.error(message: "action shortcut unavailable: \(failed.map(String.init).joined(separator: ","))"))
            }
        case .quit:
            exitCleanly(0)
        case .unknown:
            break
        }
    }

    private func sendPermissions() {
        let ax = Permissions.accessibilityTrusted()
        let screenRecording = ScreenContext.permitted
        lastAccessibility = ax
        lastScreenRecording = screenRecording
        try? client?.send(NativeEvent.permissions(accessibility: ax, microphone: Permissions.microphoneStatus(),
                                                  screenRecording: screenRecording))
    }

    private func permissionTick() {
        let ax = Permissions.accessibilityTrusted()
        if ax != lastAccessibility || ScreenContext.permitted != lastScreenRecording {
            sendPermissions()
            if ax { observer.stop(); startObserving() }  // observers cannot attach while untrusted
        }
    }

    /// Test hook (`TestHooks.mainStallMs`, hooks builds only): block the main thread every 2.5 s, to stand in for a
    /// slow Accessibility read (an app that is busy or not responding). Used to prove typing never waits for it.
    private func startStallTestIfRequested() {
        guard let ms = TestHooks.mainStallMs else { return }
        FileHandle.standardError.write(Data("bob-native: test stall of \(ms) ms every 2.5 s is on\n".utf8))
        Timer.scheduledTimer(withTimeInterval: 2.5, repeats: true) { _ in usleep(useconds_t(ms) * 1000) }
    }

    private func startObserving() {
        startStallTestIfRequested()
        AXReader.limitMessagingTimeout()
        observeWake()
        observer.onSnapshot = { [weak self] snap, reason in self?.forward(snap, reason) }
        observer.start()
        keyTap.onKey = { [weak self] key in
            try? self?.client?.send(NativeEvent.key(key, shown: self?.shownChip.value))
        }
        hotkeys.onPress = { [weak self] slot in
            switch slot {
            case .selection: self?.selectionHotkey()
            case .dictation: try? self?.client?.send(NativeEvent.dictationKey(pressed: true))
            default: if let i = slot.actionIndex { self?.selectionHotkey(action: i) }
            }
        }
        hotkeys.onRelease = { [weak self] slot in
            if slot == .dictation { try? self?.client?.send(NativeEvent.dictationKey(pressed: false)) }
        }
        hotkeys.register(HotkeyCenter.defaultHotkey, slot: .selection)
        recorder.onLevel = { [weak self] level in self?.orb.setLevel(level) }
        keyTap.onMenuKey = { [weak self] key in self?.menuKey(key) }
        menu.onClick = { [weak self] i in self?.chooseMenu(i) }
        clipboardWatcher.onText = { [weak self] text in try? self?.client?.send(NativeEvent.clipboardText(text: text)) }
        screen.onText = { [weak self] scope, app, text, ms in
            try? self?.client?.send(NativeEvent.screenText(scope: scope, appBundleId: app, text: text, ms: ms))
        }
        if !keyTap.install() {
            FileHandle.standardError.write(Data("bob-native: key tap unavailable (Accessibility not granted yet)\n".utf8))
        }
    }

    /// Exit after releasing the whisper model (see `Transcriber.shutdown`).
    func exitCleanly(_ code: Int32) -> Never {
        _ = recorder.stop(keep: false)
        transcriber.shutdown()
        exit(code)
    }

    // MARK: Voice Dictation

    private func configureDictation(hotkey: Hotkey?, modelPath: String?, language: String) {
        dictationLanguage = language
        if let hotkey {
            if !hotkeys.register(hotkey, slot: .dictation) {
                try? client?.send(NativeEvent.dictationError(reason: "the dictation hotkey is taken by another app"))
            }
        } else {
            hotkeys.unregister(.dictation)
        }
        guard modelPath != dictationModel else { return }
        dictationModel = modelPath
        transcriber.setModel(path: modelPath) { [weak self] failure in
            guard let self else { return }
            if failure == nil, modelPath != nil {
                // Warm up Metal so the first real dictation is not the slow one (measured: ~590 vs ~260 ms).
                self.transcriber.transcribe([Float](repeating: 0, count: 16_000), language: "en") { _ in }
            }
            DispatchQueue.main.async {
                try? self.client?.send(NativeEvent.dictationStatus(modelLoaded: failure == nil && modelPath != nil,
                                                                   error: failure.map { "\($0)" }))
            }
        }
    }

    private func startRecording() {
        if Permissions.microphoneStatus() == .undetermined {
            Permissions.requestMicrophoneIfPossible { [weak self] _ in
                DispatchQueue.main.async { self?.sendPermissions() }
            }
        }
        // Voice edit: if text is selected (Accessibility only, never the clipboard while a key is held),
        // the spoken words become an instruction for it. The replacement goes to this app.
        if let element = AXReader.focusedElement(),
           AXReader.string(element, kAXSubroleAttribute) != (kAXSecureTextFieldSubrole as String),
           let selected = AXReader.string(element, kAXSelectedTextAttribute), !selected.isEmpty,
           let front = NSWorkspace.shared.frontmostApplication {
            selectionPID = front.processIdentifier
            try? client?.send(NativeEvent.dictationSelection(appBundleId: front.bundleIdentifier ?? "",
                                                            text: String(selected.prefix(SelectionIO.maxChars + 1))))
        }
        DictationSounds.play(.start)
        do {
            try recorder.start()
        } catch let failure as AudioRecorder.Failure {
            try? client?.send(NativeEvent.dictationError(reason: "\(failure)"))
        } catch {
            try? client?.send(NativeEvent.dictationError(reason: "the microphone could not start"))
        }
    }

    private func stopRecording(transcribe: Bool) {
        var samples = recorder.stop(keep: transcribe)
        guard transcribe else { return }
        DictationSounds.play(.stop)
        // The start cue may be in the first few hundred samples; it is not speech.
        if DictationSounds.enabled { samples = Array(samples.dropFirst(16 * DictationSounds.startCueMs)) }
        let audioMs = samples.count / 16
        guard SpeechGate.hasSpeech(samples) else {
            try? client?.send(NativeEvent.transcript(text: "", language: nil, speech: false, audioMs: audioMs, transcribeMs: 0))
            return
        }
        let language = dictationLanguage.isEmpty ? nil : dictationLanguage
        transcriber.transcribe(samples, language: language, prompt: dictationPrompt) { [weak self] result in
            DispatchQueue.main.async {
                switch result {
                case let .success(r):
                    try? self?.client?.send(NativeEvent.transcript(text: r.text, language: r.language, speech: true,
                                                                   audioMs: audioMs, transcribeMs: r.ms))
                case .failure(.noModel):
                    try? self?.client?.send(NativeEvent.dictationError(reason: "no_model"))
                case let .failure(f):
                    try? self?.client?.send(NativeEvent.dictationError(reason: "\(f)"))
                }
            }
        }
    }

    // MARK: Wake from sleep

    /// After sleep, models may have been paged out and the first suggestion or dictation would be slow: tell
    /// the core (it warms the built-in brain) and warm the speech model here. Waits a moment for the GPU and
    /// disks to settle first.
    private func observeWake() {
        NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification, object: nil, queue: .main
        ) { [weak self] _ in
            DispatchQueue.main.asyncAfter(deadline: .now() + .seconds(2)) {
                guard let self else { return }
                try? self.client?.send(NativeEvent.woke)
                self.transcriber.warmUp { ms in
                    if let ms { FileHandle.standardError.write(Data("bob-native: woke; speech model warmed up in \(ms) ms\n".utf8)) }
                }
            }
        }
    }

        // MARK: Selection Actions

    /// `action`: the custom action whose own shortcut was pressed (it runs without the menu).
    private func selectionHotkey(action: Int? = nil) {
        SelectionIO.read { [weak self] capture in
            guard let self else { return }
            self.selectionPID = capture.pid
            try? self.client?.send(NativeEvent.selection(
                appBundleId: capture.appBundleId, text: capture.text, secure: capture.secure,
                source: capture.source, anchor: capture.anchor, action: action))
        }
    }

    private func menuKey(_ key: MenuKey) {
        guard menu.isMenuVisible else { keyTap.isMenuOpen = false; return }
        switch menu.move(key) {
        case .moved: break
        case let .chose(i): chooseMenu(i)
        case .cancelled: cancelMenu()
        }
    }

    private func chooseMenu(_ index: Int) {
        keyTap.isMenuOpen = false
        try? client?.send(NativeEvent.menuChoice(index: index))
    }

    private func cancelMenu() {
        keyTap.isMenuOpen = false
        menu.hideMenu()
        try? client?.send(NativeEvent.menuCancel)
    }

    private func forward(_ snap: AXSnapshot, _ reason: FocusObserver.Reason) {
        // Switching away from the app closes an open action menu (SA-7).
        if reason == .appActivated, menu.isMenuVisible, snap.pid != selectionPID { cancelMenu() }
        if let caret = snap.caret { lastCaret = caret }
        if reason == .appActivated { lastCaret = snap.caret }
        if reason == .appActivated || reason == .focusChanged {
            try? client?.send(NativeEvent.focus(appBundleId: snap.appBundleId, appName: snap.appName,
                                                secure: snap.secure, editable: snap.editable))
        }
        guard snap.editable else { return }
        try? client?.send(NativeEvent.textState(
            appBundleId: snap.appBundleId, secure: snap.secure, beforeCaret: snap.beforeCaret ?? "",
            selectionLen: snap.selectionLen ?? 0, caret: snap.caret))
        guard !snap.secure else { return }
        let before = (snap.beforeCaret ?? "").replacingOccurrences(of: "\u{a0}", with: " ")
        if (snap.selectionLen ?? 0) == 0, before != lastTypoCheck {
            lastTypoCheck = before
            // Snapshots are delivered on the main run loop.
            if let t = Stall.measure("typo check", { MainActor.assumeIsolated { TypoFinder.find(in: before) } }) {
                try? client?.send(NativeEvent.typo(word: t.word, fix: t.fix, trailing: t.trailing))
            }
        }
        let context = NativeEvent.contextState(appBundleId: snap.appBundleId, windowTitle: snap.windowTitle,
                                               afterCaret: snap.afterCaret ?? "")
        if context != lastContext {
            lastContext = context
            try? client?.send(context)
        }
        screen.maybeCapture(appBundleId: snap.appBundleId, pid: snap.pid, windowTitle: snap.windowTitle,
                            secure: snap.secure)
    }

    /// Explain which app needs the permission, open the right Settings pane and exit.
    private func failUntrusted() -> Never {
        print(Permissions.accessibilityHelp(termProgram: ProcessInfo.processInfo.environment["TERM_PROGRAM"]))
        NSWorkspace.shared.open(Permissions.settingsURL(for: .accessibility))
        exit(1)
    }

    // MARK: probe

    private func startProbe(seconds: Double) {
        let trusted = Permissions.accessibilityTrusted(prompt: true)
        print(#"{"probe":"start","accessibility_trusted":\#(trusted),"seconds":\#(seconds)}"#)
        guard trusted else { failUntrusted() }
        print("Click into a text field in the app to test, then type for \(Int(seconds)) seconds. No typed text is printed.")
        let started = Date()
        var lastPrint = Date.distantPast
        var samples = 0, withCaret = 0, secureSeen = 0
        var sources = [String: Int]()
        var apps = Set<String>()
        observer.onSnapshot = { snap, reason in
            samples += 1
            if snap.caret != nil { withCaret += 1 }
            if let source = snap.caretSource { sources[source.rawValue, default: 0] += 1 }
            if snap.secure { secureSeen += 1 }
            apps.insert(snap.appBundleId)
            guard Date().timeIntervalSince(lastPrint) > 0.25 else { return }
            lastPrint = Date()
            var row: [String: Any] = [
                "t": (Date().timeIntervalSince(started) * 100).rounded() / 100,
                "reason": reason.rawValue,
                "app": snap.appBundleId,
                "role": snap.role ?? NSNull(),
                "subrole": snap.subrole ?? NSNull(),
                "secure": snap.secure,
                "editable": snap.editable,
                "value_len": snap.valueLength ?? NSNull(),
                "before_caret_len": snap.beforeCaret?.count ?? NSNull(),
                "caret_found": snap.caret != nil,
                "caret_source": snap.caretSource?.rawValue ?? NSNull(),
                "frame_found": snap.elementFrame != nil,
            ]
            if let c = snap.caret { row["caret"] = ["x": c.x, "y": c.y, "w": c.width, "h": c.height] }
            if let data = try? JSONSerialization.data(withJSONObject: row, options: [.sortedKeys]),
               let s = String(data: data, encoding: .utf8) { print(s) }
        }
        observer.start()
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { [observer] in
            let summary: [String: Any] = [
                "probe": "summary", "samples": samples, "samples_with_caret": withCaret,
                "secure_fields_seen": secureSeen, "apps": apps.sorted(), "caret_sources": sources,
                "ax_notifications": observer.notificationCounts,
            ]
            if let data = try? JSONSerialization.data(withJSONObject: summary, options: [.sortedKeys]),
               let s = String(data: data, encoding: .utf8) { print(s) }
            exit(0)
        }
    }
}

/// A value shared between the main thread and the key tap's thread.
final class Locked<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private var stored: Value

    init(_ value: Value) {
        stored = value
    }

    var value: Value {
        get { lock.withLock { stored } }
        set { lock.withLock { stored = newValue } }
    }
}
