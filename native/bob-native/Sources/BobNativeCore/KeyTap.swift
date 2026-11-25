import ApplicationServices
import CoreGraphics
import Foundation
import os

/// Keys that drive the Selection Actions menu (SA-4).
public enum MenuKey: Equatable, Sendable {
    case up, down, enter, escape
}

/// Which keys the helper may swallow, and when (GT-10 to GT-13, SA-4). Pure, so it is unit-testable.
public enum KeyDecision: Equatable, Sendable {
    case pass
    case swallow(Key)
    case menu(MenuKey)

    public static let tabKeyCode: Int64 = 48
    public static let escapeKeyCode: Int64 = 53
    public static let returnKeyCode: Int64 = 36
    public static let keypadEnterKeyCode: Int64 = 76
    public static let upKeyCode: Int64 = 126
    public static let downKeyCode: Int64 = 125

    /// Modifiers that make a key press a shortcut we must never steal (Cmd+Tab, Ctrl+Tab, Option+Esc...).
    /// Fn is included for Tab/Esc; arrow keys always carry it, so the menu check strips it first.
    static let blocking: CGEventFlags = [.maskCommand, .maskControl, .maskAlternate, .maskSecondaryFn]
    static let blockingForMenu: CGEventFlags = [.maskCommand, .maskControl, .maskAlternate]

    /// - Parameters:
    ///   - keyCode: virtual key code of a key-down event.
    ///   - flags: the event's modifier flags.
    ///   - capturing: the core asked for capture (a chip is visible).
    ///   - synthetic: the event was posted by BOB itself (text insertion).
    ///   - menuOpen: the Selection Actions menu is visible; it takes precedence over Ghost capture.
    public static func decide(keyCode: Int64, flags: CGEventFlags, capturing: Bool, synthetic: Bool = false,
                              menuOpen: Bool = false) -> KeyDecision {
        guard !synthetic else { return .pass }
        if menuOpen {
            guard flags.intersection(blockingForMenu).isEmpty else { return .pass }
            // Arrow keys carry the numeric-pad and Fn flags; only Cmd/Ctrl/Option/Shift matter here.
            guard !flags.contains(.maskShift) else { return .pass }
            switch keyCode {
            case upKeyCode: return .menu(.up)
            case downKeyCode: return .menu(.down)
            case returnKeyCode, keypadEnterKeyCode: return .menu(.enter)
            case escapeKeyCode: return .menu(.escape)
            default: return .pass
            }
        }
        guard capturing, flags.intersection(blocking).isEmpty else { return .pass }
        switch keyCode {
        case tabKeyCode: return .swallow(flags.contains(.maskShift) ? .shiftTab : .tab)
        case escapeKeyCode: return flags.contains(.maskShift) ? .pass : .swallow(.escape)
        default: return .pass
        }
    }
}

/// Marker stored in `eventSourceUserData` of every event BOB posts, so the tap never captures them.
public let bobSyntheticEventMarker: Int64 = 0x0B0B_0B0B

/// An active `CGEventTap` on key-down events. macOS holds every keystroke until the tap answers, so the tap
/// runs on its own thread: Accessibility reads, drawing and model loading on the main thread can never delay
/// typing (measured: a 1.5 s main-thread stall made macOS disable a main-thread tap and drop half the keys).
/// The callback only reads atomic flags and returns. Requires Accessibility permission.
public final class KeyTap: @unchecked Sendable {
    private let capturing = OSAllocatedUnfairLock(initialState: false)
    private let menuOpen = OSAllocatedUnfairLock(initialState: false)
    /// Called on the main run loop for every swallowed menu key.
    public var onMenuKey: ((MenuKey) -> Void)?

    public var isMenuOpen: Bool {
        get { menuOpen.withLock { $0 } }
        set { menuOpen.withLock { $0 = newValue } }
    }
    private var tap: CFMachPort?
    private var source: CFRunLoopSource?
    private var tapThread: Thread?
    private var tapLoop: CFRunLoop?
    /// Called on the main run loop for every swallowed key.
    public var onKey: ((Key) -> Void)?

    public init() {}

    public var isCapturing: Bool {
        get { capturing.withLock { $0 } }
        set { capturing.withLock { $0 = newValue } }
    }

    public var isInstalled: Bool { tap != nil }

    /// Install the tap. Returns false when the system refuses (no Accessibility permission).
    @discardableResult
    public func install() -> Bool {
        guard tap == nil else { return true }
        let mask = CGEventMask(1 << CGEventType.keyDown.rawValue)
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        guard let port = CGEvent.tapCreate(
            tap: .cgSessionEventTap, place: .headInsertEventTap, options: .defaultTap,
            eventsOfInterest: mask, callback: keyTapCallback, userInfo: refcon
        ) else { return false }
        guard let src = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, port, 0) else { return false }
        tap = port
        source = src
        let started = DispatchSemaphore(value: 0)
        let thread = Thread { [weak self] in
            guard let self else { started.signal(); return }
            self.tapLoop = CFRunLoopGetCurrent()
            CFRunLoopAddSource(CFRunLoopGetCurrent(), src, .commonModes)
            CGEvent.tapEnable(tap: port, enable: true)
            started.signal()
            CFRunLoopRun()
        }
        thread.name = "dev.bob.keytap"
        thread.qualityOfService = .userInteractive
        thread.start()
        tapThread = thread
        started.wait()
        return true
    }

    public func uninstall() {
        if let port = tap { CGEvent.tapEnable(tap: port, enable: false) }
        if let loop = tapLoop {
            if let src = source { CFRunLoopRemoveSource(loop, src, .commonModes) }
            CFRunLoopStop(loop)
        }
        tap = nil
        source = nil
        tapLoop = nil
        tapThread = nil
    }

    fileprivate func handle(type: CGEventType, event: CGEvent) -> Unmanaged<CGEvent>? {
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            // macOS disables slow or interrupted taps; turn it back on (macos-native steering).
            FileHandle.standardError.write(Data("bob-native: key tap was disabled by macOS (\(type == .tapDisabledByTimeout ? "timeout" : "user input")); re-enabled\n".utf8))
            if let port = tap { CGEvent.tapEnable(tap: port, enable: true) }
            return Unmanaged.passUnretained(event)
        }
        guard type == .keyDown else { return Unmanaged.passUnretained(event) }
        // How long the key waited before the tap saw it (the whole system waits for an active tap).
        let lagMs = Double(clock_gettime_nsec_np(CLOCK_UPTIME_RAW) &- event.timestamp) / 1e6
        Stall.report("key tap (key waited)", ms: lagMs, threshold: 30)
        let decision = KeyDecision.decide(
            keyCode: event.getIntegerValueField(.keyboardEventKeycode),
            flags: event.flags,
            capturing: isCapturing,
            synthetic: event.getIntegerValueField(.eventSourceUserData) == bobSyntheticEventMarker,
            menuOpen: isMenuOpen
        )
        switch decision {
        case .pass:
            return Unmanaged.passUnretained(event)
        case let .swallow(key):
            let callback = onKey
            DispatchQueue.main.async { callback?(key) }
            return nil
        case let .menu(key):
            let callback = onMenuKey
            DispatchQueue.main.async { callback?(key) }
            return nil
        }
    }
}

private func keyTapCallback(proxy: CGEventTapProxy, type: CGEventType, event: CGEvent, refcon: UnsafeMutableRawPointer?) -> Unmanaged<CGEvent>? {
    guard let refcon else { return Unmanaged.passUnretained(event) }
    return Unmanaged<KeyTap>.fromOpaque(refcon).takeUnretainedValue().handle(type: type, event: event)
}
