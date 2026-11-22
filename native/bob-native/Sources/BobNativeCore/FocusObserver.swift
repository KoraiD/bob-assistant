import AppKit
import ApplicationServices

/// Observes the frontmost app's focused text element with `AXObserver` (no polling, requirement NB-7).
/// Must be created and used on the main thread; callbacks arrive on the main run loop.
public final class FocusObserver {
    public enum Reason: String {
        case appActivated, focusChanged, valueChanged, selectionChanged
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

    public init() {}

    public func start() {
        workspaceToken = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main
        ) { [weak self] note in
            guard let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication else { return }
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
        let appElement = AXUIElementCreateApplication(pid)
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        AXObserverAddNotification(obs, appElement, kAXFocusedUIElementChangedNotification as CFString, refcon)
        observeFocusedElement()
        schedule(reason)
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
        case kAXFocusedUIElementChangedNotification:
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
            self.onSnapshot?(AXReader.snapshot(of: element), reason)
        }
    }
}
