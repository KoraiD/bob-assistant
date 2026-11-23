import ApplicationServices
import CoreGraphics
import Foundation
import os

/// Which keys the helper may swallow, and when (GT-10 to GT-13). Pure, so it is unit-testable.
public enum KeyDecision: Equatable, Sendable {
    case pass
    case swallow(Key)

    public static let tabKeyCode: Int64 = 48
    public static let escapeKeyCode: Int64 = 53

    /// Modifiers that make a key press a shortcut we must never steal (Cmd+Tab, Ctrl+Tab, Option+Esc...).
    static let blocking: CGEventFlags = [.maskCommand, .maskControl, .maskAlternate, .maskSecondaryFn]

    /// - Parameters:
    ///   - keyCode: virtual key code of a key-down event.
    ///   - flags: the event's modifier flags.
    ///   - capturing: the core asked for capture (a chip is visible).
    ///   - synthetic: the event was posted by BOB itself (text insertion).
    public static func decide(keyCode: Int64, flags: CGEventFlags, capturing: Bool, synthetic: Bool = false) -> KeyDecision {
        guard capturing, !synthetic, flags.intersection(blocking).isEmpty else { return .pass }
        switch keyCode {
        case tabKeyCode: return .swallow(flags.contains(.maskShift) ? .shiftTab : .tab)
        case escapeKeyCode: return flags.contains(.maskShift) ? .pass : .swallow(.escape)
        default: return .pass
        }
    }
}

/// Marker stored in `eventSourceUserData` of every event BOB posts, so the tap never captures them.
public let bobSyntheticEventMarker: Int64 = 0x0B0B_0B0B

/// An active `CGEventTap` on key-down events. The callback only reads an atomic flag and returns,
/// so it can never stall typing (perf steering). Requires Accessibility permission.
public final class KeyTap: @unchecked Sendable {
    private let capturing = OSAllocatedUnfairLock(initialState: false)
    private var tap: CFMachPort?
    private var source: CFRunLoopSource?
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
        let src = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, port, 0)
        CFRunLoopAddSource(CFRunLoopGetMain(), src, .commonModes)
        CGEvent.tapEnable(tap: port, enable: true)
        tap = port
        source = src
        return true
    }

    public func uninstall() {
        if let port = tap { CGEvent.tapEnable(tap: port, enable: false) }
        if let src = source { CFRunLoopRemoveSource(CFRunLoopGetMain(), src, .commonModes) }
        tap = nil
        source = nil
    }

    fileprivate func handle(type: CGEventType, event: CGEvent) -> Unmanaged<CGEvent>? {
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            // macOS disables slow or interrupted taps; turn it back on (macos-native steering).
            if let port = tap { CGEvent.tapEnable(tap: port, enable: true) }
            return Unmanaged.passUnretained(event)
        }
        guard type == .keyDown else { return Unmanaged.passUnretained(event) }
        let decision = KeyDecision.decide(
            keyCode: event.getIntegerValueField(.keyboardEventKeycode),
            flags: event.flags,
            capturing: isCapturing,
            synthetic: event.getIntegerValueField(.eventSourceUserData) == bobSyntheticEventMarker
        )
        switch decision {
        case .pass:
            return Unmanaged.passUnretained(event)
        case let .swallow(key):
            let callback = onKey
            DispatchQueue.main.async { callback?(key) }
            return nil
        }
    }
}

private func keyTapCallback(proxy: CGEventTapProxy, type: CGEventType, event: CGEvent, refcon: UnsafeMutableRawPointer?) -> Unmanaged<CGEvent>? {
    guard let refcon else { return Unmanaged.passUnretained(event) }
    return Unmanaged<KeyTap>.fromOpaque(refcon).takeUnretainedValue().handle(type: type, event: event)
}
