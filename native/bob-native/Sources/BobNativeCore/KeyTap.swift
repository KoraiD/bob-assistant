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

/// What a key that goes to the app does to the text, for the core while a chip is shown (code review GT-01).
/// Pure, so it is unit-testable. `nil`: nothing to report (a dead key, a function key); the next
/// Accessibility report tells.
public enum EditDecision {
    static let backspaceKeyCode: Int64 = 51
    /// Keys that move the caret or change text in ways the core cannot predict: arrows, Home, End, Page
    /// Up/Down, Forward Delete, Return, Enter, Tab and Esc when they are not swallowed.
    static let otherKeyCodes: Set<Int64> = [123, 124, 125, 126, 115, 119, 116, 121, 117, 36, 76, 48, 53]

    public static func classify(keyCode: Int64, flags: CGEventFlags, text: String) -> (EditKind, String?)? {
        if !flags.intersection([.maskCommand, .maskControl]).isEmpty { return (.other, nil) }
        if keyCode == backspaceKeyCode { return flags.contains(.maskAlternate) ? (.other, nil) : (.backspace, nil) }
        if otherKeyCodes.contains(keyCode) { return (.other, nil) }
        guard !text.isEmpty else { return nil }
        // Function keys report characters in the private use area; control characters are not text.
        if text.unicodeScalars.contains(where: { (0xF700...0xF8FF).contains($0.value) }) { return nil }
        if text.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) { return (.other, nil) }
        return (.type, text)
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
    /// Called on the main run loop for every key that goes to the app while capturing (code review GT-01).
    public var onEdit: ((EditKind, String?) -> Void)?

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
        let keyCode = event.getIntegerValueField(.keyboardEventKeycode)
        let capturing = isCapturing
        let synthetic = event.getIntegerValueField(.eventSourceUserData) == bobSyntheticEventMarker
        let menuOpen = isMenuOpen
        let decision = KeyDecision.decide(keyCode: keyCode, flags: event.flags, capturing: capturing,
                                          synthetic: synthetic, menuOpen: menuOpen)
        switch decision {
        case .pass:
            // While a chip is shown, tell the core what the key does before Accessibility reports it.
            if capturing, !synthetic, !menuOpen, let callback = onEdit,
               let (kind, text) = EditDecision.classify(keyCode: keyCode, flags: event.flags, text: typedText(event)) {
                DispatchQueue.main.async { callback(kind, text) }
            }
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

/// The characters a key-down types (empty for keys that type nothing).
private func typedText(_ event: CGEvent) -> String {
    var length = 0
    var chars = [UniChar](repeating: 0, count: 8)
    event.keyboardGetUnicodeString(maxStringLength: chars.count, actualStringLength: &length, unicodeString: &chars)
    return String(utf16CodeUnits: chars, count: min(length, chars.count))
}

private func keyTapCallback(proxy: CGEventTapProxy, type: CGEventType, event: CGEvent, refcon: UnsafeMutableRawPointer?) -> Unmanaged<CGEvent>? {
    guard let refcon else { return Unmanaged.passUnretained(event) }
    return Unmanaged<KeyTap>.fromOpaque(refcon).takeUnretainedValue().handle(type: type, event: event)
}
