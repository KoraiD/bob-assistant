import AppKit
import Foundation

public enum HelperMode {
    /// Normal operation: connect to the core at this socket path.
    case connect(socketPath: String)
    /// Compatibility spike: observe the focused field for `seconds` and print per-event findings.
    case probe(seconds: Double)
    /// Standalone demo: show a ghost chip at the caret in any app.
    case chipDemo
}

/// Wires the observer, the chip and the socket together. Not unit-tested (needs a window server and
/// permissions); its pieces are tested individually and the whole is covered by the manual matrix.
public final class HelperRuntime {
    private let mode: HelperMode
    private let observer = FocusObserver()
    private let chip = ChipPanel()
    private var client: SocketClient?
    private var lastAccessibility = false
    private var permissionTimer: Timer?

    public init(mode: HelperMode) { self.mode = mode }

    /// Never returns: runs the AppKit main loop.
    public func run() -> Never {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)  // no Dock icon, never steals focus

        switch mode {
        case let .connect(path): startConnected(path: path)
        case let .probe(seconds): startProbe(seconds: seconds)
        case .chipDemo: startChipDemo()
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
                onClose: { exit(0) }  // the core went away: the helper exits with it (NB-5)
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
        case .hideChip:
            chip.hide()
        case .requestPermissions:
            _ = Permissions.accessibilityTrusted(prompt: true)
            Permissions.requestMicrophoneIfPossible { [weak self] _ in
                DispatchQueue.main.async { self?.sendPermissions() }
            }
            sendPermissions()
        case .quit:
            NSApp.terminate(nil)
        case .unknown:
            break
        }
    }

    private func sendPermissions() {
        let ax = Permissions.accessibilityTrusted()
        lastAccessibility = ax
        try? client?.send(NativeEvent.permissions(accessibility: ax, microphone: Permissions.microphoneStatus()))
    }

    private func permissionTick() {
        let ax = Permissions.accessibilityTrusted()
        if ax != lastAccessibility {
            sendPermissions()
            if ax { observer.stop(); startObserving() }  // observers cannot attach while untrusted
        }
    }

    private func startObserving() {
        observer.onSnapshot = { [weak self] snap, reason in self?.forward(snap, reason) }
        observer.start()
    }

    private func forward(_ snap: AXSnapshot, _ reason: FocusObserver.Reason) {
        if reason == .appActivated || reason == .focusChanged {
            try? client?.send(NativeEvent.focus(appBundleId: snap.appBundleId, appName: snap.appName,
                                                secure: snap.secure, editable: snap.editable))
        }
        guard snap.editable else { return }
        try? client?.send(NativeEvent.textState(
            appBundleId: snap.appBundleId, secure: snap.secure, beforeCaret: snap.beforeCaret ?? "",
            selectionLen: snap.selectionLen ?? 0, caret: snap.caret))
    }

    /// Explain which app needs the permission, open the right Settings pane and exit.
    private func failUntrusted() -> Never {
        print(Permissions.accessibilityHelp(termProgram: ProcessInfo.processInfo.environment["TERM_PROGRAM"]))
        NSWorkspace.shared.open(Permissions.settingsURL(for: .accessibility))
        exit(1)
    }

    // MARK: chip demo

    private func startChipDemo() {
        guard Permissions.accessibilityTrusted(prompt: true) else { failUntrusted() }
        print("chip demo: click into a text field in any app and type. Press Ctrl+C to stop.")
        observer.onSnapshot = { [weak self] snap, _ in
            guard let self else { return }
            if snap.secure || !snap.editable { self.chip.hide(); return }
            if let caret = snap.caret {
                self.chip.show(text: "BOB is watching", caretAX: caret)
            } else {
                self.chip.hide()
            }
        }
        observer.start()
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
        var apps = Set<String>()
        observer.onSnapshot = { snap, reason in
            samples += 1
            if snap.caret != nil { withCaret += 1 }
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
            ]
            if let c = snap.caret { row["caret"] = ["x": c.x, "y": c.y, "w": c.width, "h": c.height] }
            if let data = try? JSONSerialization.data(withJSONObject: row, options: [.sortedKeys]),
               let s = String(data: data, encoding: .utf8) { print(s) }
        }
        observer.start()
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { [observer] in
            let summary: [String: Any] = [
                "probe": "summary", "samples": samples, "samples_with_caret": withCaret,
                "secure_fields_seen": secureSeen, "apps": apps.sorted(),
                "ax_notifications": observer.notificationCounts,
            ]
            if let data = try? JSONSerialization.data(withJSONObject: summary, options: [.sortedKeys]),
               let s = String(data: data, encoding: .utf8) { print(s) }
            exit(0)
        }
    }
}
