import AppKit
import ApplicationServices

/// System overlays that briefly become frontmost without being a real app switch. Activating them must
/// not hide the chip or move observation away from the app the user is typing in (otherwise taking a
/// screenshot, or recording the demo video, makes the suggestion disappear).
public enum TransientApps {
    public static let bundleIds: Set<String> = [
        "com.apple.screencaptureui",      // Cmd+Shift+4 / Cmd+Shift+5 capture UI
        "com.apple.screenshot.launcher",  // Screenshot.app
    ]

    public static func ignores(_ bundleId: String?) -> Bool {
        guard let bundleId else { return false }
        return bundleIds.contains(bundleId)
    }
}

/// When to try again to observe an app whose Accessibility was not ready (code review GT-12: an app that
/// comes to the front while it launches refuses the subscriptions, and nothing retried until the next
/// activation). Pure, so it is unit-testable.
public enum AttachRetry {
    /// Delays between attempts, about 7.75 s in all.
    public static let delaysMs = [250, 500, 1000, 2000, 4000]

    public static func needed(appSubscribed: Bool, elementObserved: Bool) -> Bool {
        !appSubscribed || !elementObserved
    }

    /// `AXObserverAddNotification` results that mean the subscription is in place.
    public static func subscribed(_ result: AXError) -> Bool {
        result == .success || result == .notificationAlreadyRegistered
    }
}

/// Observes the frontmost app's focused text element with `AXObserver` (no polling, requirement NB-7).
/// Must be created and used on the main thread; callbacks arrive on the main run loop.
public final class FocusObserver {
    public enum Reason: String {
        case appActivated, focusChanged, valueChanged, selectionChanged
        /// A re-read shortly after a change found newer text or caret (apps that update Accessibility late).
        case settled
    }

    /// Re-reads after each delivered snapshot. Some apps (Chrome, Electron) report the new caret before the
    /// new text, or update the value after the notification; without a re-read the chip could keep showing
    /// text the user already typed. Each re-read costs about a millisecond and is only delivered on change.
    public static let settleDelaysMs = [40, 120, 300]

    struct Key: Equatable {
        let before: String?
        let selectionLen: Int?
        let caret: Rect?
    }
    private var lastDelivered: Key?
    private var settleGeneration = 0
    private var watchTimer: Timer?

    /// Re-check the focused field every 250 ms (only while BOB shows a suggestion). Delivers a snapshot when the
    /// text or caret changed, and re-attaches when focus moved to another field or window without a notification.
    public func setWatching(_ on: Bool) {
        if on, watchTimer == nil {
            watchTimer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in self?.watch() }
        } else if !on {
            watchTimer?.invalidate()
            watchTimer = nil
        }
    }

    private func watch() {
        guard pendingReason == nil, let element = AXReader.focusedElement() else { return }
        if AXReader.pid(of: element) != observedPID {
            schedule(.focusChanged)
            return
        }
        if let old = observedElement, !CFEqual(old, element) {
            observeFocusedElement()
            schedule(.focusChanged)
            return
        }
        let snap = Stall.measure("Accessibility watch") { AXReader.snapshot(of: element) }
        let key = Key(before: snap.beforeCaret, selectionLen: snap.selectionLen, caret: snap.caret)
        if key != lastDelivered { deliver(snap, .settled) }
    }

    /// Called (coalesced, main thread) with a fresh snapshot.
    public var onSnapshot: ((AXSnapshot, Reason) -> Void)?

    /// Count of raw AX notifications received, by name. Used by the compatibility probe.
    public private(set) var notificationCounts: [String: Int] = [:]

    private var observer: AXObserver?
    private var observedPID: pid_t = 0
    private var observedElement: AXUIElement?
    private var pendingReason: Reason?
    private var workspaceToken: NSObjectProtocol?
    /// Bumped on every attach and detach, so retries for an earlier app stop.
    private var attachGeneration = 0
    private var appSubscribed = false

    public init() {}

    public func start() {
        workspaceToken = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main
        ) { [weak self] note in
            guard let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
                  !TransientApps.ignores(app.bundleIdentifier) else { return }
            self?.attach(pid: app.processIdentifier, reason: .appActivated)
        }
        if let front = NSWorkspace.shared.frontmostApplication {
            attach(pid: front.processIdentifier, reason: .appActivated)
        }
    }

    public func stop() {
        if let token = workspaceToken { NSWorkspace.shared.notificationCenter.removeObserver(token) }
        workspaceToken = nil
        detach()
    }

    // MARK: attach / detach

    private func detach() {
        attachGeneration += 1
        appSubscribed = false
        if let obs = observer {
            CFRunLoopRemoveSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(obs), .defaultMode)
        }
        observer = nil
        observedElement = nil
        observedPID = 0
    }

    private func attach(pid: pid_t, reason: Reason) {
        guard pid != ProcessInfo.processInfo.processIdentifier else { return }  // never observe ourselves
        detach()
        var created: AXObserver?
        let callback: AXObserverCallback = { _, element, notification, refcon in
            guard let refcon else { return }
            let me = Unmanaged<FocusObserver>.fromOpaque(refcon).takeUnretainedValue()
            me.handle(element: element, notification: notification as String)
        }
        guard AXObserverCreate(pid, callback, &created) == .success, let obs = created else { return }
        observer = obs
        observedPID = pid
        CFRunLoopAddSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(obs), .defaultMode)

        AXReader.enableManualAccessibility(pid: pid)
        appSubscribed = subscribeApp(obs, pid: pid)
        observeFocusedElement()
        schedule(reason)
        if AttachRetry.needed(appSubscribed: appSubscribed, elementObserved: observedElement != nil) {
            retryAttach(attempt: 0, generation: attachGeneration)
        }
    }

    /// App-level notifications; true when the focus notification is in place.
    private func subscribeApp(_ obs: AXObserver, pid: pid_t) -> Bool {
        let appElement = AXUIElementCreateApplication(pid)
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        let focus = AXObserverAddNotification(obs, appElement, kAXFocusedUIElementChangedNotification as CFString, refcon)
        // Switching windows inside the same app does not always move focus notifications.
        AXObserverAddNotification(obs, appElement, kAXFocusedWindowChangedNotification as CFString, refcon)
        AXObserverAddNotification(obs, appElement, kAXMainWindowChangedNotification as CFString, refcon)
        return AttachRetry.subscribed(focus)
    }

    /// Code review GT-12: try again while the app is still starting; read the field as soon as one appears.
    private func retryAttach(attempt: Int, generation: Int) {
        guard attempt < AttachRetry.delaysMs.count else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(AttachRetry.delaysMs[attempt])) { [weak self] in
            guard let self, generation == self.attachGeneration, let obs = self.observer else { return }
            if !self.appSubscribed { self.appSubscribed = self.subscribeApp(obs, pid: self.observedPID) }
            let hadElement = self.observedElement != nil
            self.observeFocusedElement()
            if !hadElement, self.observedElement != nil { self.schedule(.focusChanged) }
            if AttachRetry.needed(appSubscribed: self.appSubscribed, elementObserved: self.observedElement != nil) {
                self.retryAttach(attempt: attempt + 1, generation: generation)
            }
        }
    }

    /// Subscribe to value/selection changes on the element that currently has focus.
    private func observeFocusedElement() {
        guard let obs = observer, let focused = AXReader.focusedElement(),
              AXReader.pid(of: focused) == observedPID else { return }
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        if let old = observedElement {
            AXObserverRemoveNotification(obs, old, kAXValueChangedNotification as CFString)
            AXObserverRemoveNotification(obs, old, kAXSelectedTextChangedNotification as CFString)
        }
        AXObserverAddNotification(obs, focused, kAXValueChangedNotification as CFString, refcon)
        AXObserverAddNotification(obs, focused, kAXSelectedTextChangedNotification as CFString, refcon)
        observedElement = focused
    }

    // MARK: notifications

    private func handle(element: AXUIElement, notification: String) {
        notificationCounts[notification, default: 0] += 1
        switch notification {
        case kAXFocusedUIElementChangedNotification, kAXFocusedWindowChangedNotification, kAXMainWindowChangedNotification:
            observeFocusedElement()
            schedule(.focusChanged)
        case kAXValueChangedNotification:
            schedule(.valueChanged)
        case kAXSelectedTextChangedNotification:
            schedule(.selectionChanged)
        default:
            break
        }
    }

    /// Coalesce bursts (value + selection fire together per keystroke) into one snapshot.
    private func schedule(_ reason: Reason) {
        let alreadyScheduled = pendingReason != nil
        // A value change is the most informative reason; keep it if present.
        if pendingReason == nil || reason == .valueChanged { pendingReason = reason }
        guard !alreadyScheduled else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(4)) { [weak self] in
            guard let self, let reason = self.pendingReason else { return }
            self.pendingReason = nil
            guard let element = AXReader.focusedElement() else { return }
            self.deliver(Stall.measure("Accessibility read") { AXReader.snapshot(of: element) }, reason)
            self.scheduleSettle()
        }
    }

    private func deliver(_ snap: AXSnapshot, _ reason: Reason) {
        lastDelivered = Key(before: snap.beforeCaret, selectionLen: snap.selectionLen, caret: snap.caret)
        onSnapshot?(snap, reason)
    }

    private func scheduleSettle() {
        settleGeneration += 1
        let generation = settleGeneration
        for delay in Self.settleDelaysMs {
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(delay)) { [weak self] in
                guard let self, generation == self.settleGeneration, self.pendingReason == nil,
                      let element = AXReader.focusedElement(), AXReader.pid(of: element) == self.observedPID else { return }
                let snap = Stall.measure("Accessibility re-read") { AXReader.snapshot(of: element) }
                let key = Key(before: snap.beforeCaret, selectionLen: snap.selectionLen, caret: snap.caret)
                if key != self.lastDelivered { self.deliver(snap, .settled) }
            }
        }
    }
}
