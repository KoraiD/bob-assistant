import Carbon.HIToolbox
import Foundation

/// The Selection Actions hotkey (SA-1), registered with Carbon's `RegisterEventHotKey`: system-wide, no event
/// tap, no extra permission, and the key press is not delivered to the frontmost app.
public final class HotkeyCenter {
    public static let defaultHotkey = Hotkey(keyCode: kVK_ANSI_J, modifiers: ["cmd", "shift"])

    private var ref: EventHotKeyRef?
    private var handler: EventHandlerRef?
    /// Called on the main thread when the hotkey is pressed.
    public var onPress: (() -> Void)?

    public init() {}

    /// Carbon modifier mask for modifier names; unknown names make the hotkey invalid (nil).
    public static func carbonModifiers(_ names: [String]) -> UInt32? {
        var mask: UInt32 = 0
        for name in names {
            switch name.lowercased() {
            case "cmd", "command": mask |= UInt32(cmdKey)
            case "shift": mask |= UInt32(shiftKey)
            case "option", "alt": mask |= UInt32(optionKey)
            case "control", "ctrl": mask |= UInt32(controlKey)
            default: return nil
            }
        }
        return mask
    }

    /// A hotkey must use Cmd, Control or Option, so plain typing can never trigger it.
    public static func isAcceptable(_ hotkey: Hotkey) -> Bool {
        guard let mask = carbonModifiers(hotkey.modifiers), (0...0x7F).contains(hotkey.keyCode) else { return false }
        return mask & UInt32(cmdKey | controlKey | optionKey) != 0
    }

    /// Register `hotkey`, replacing the previous one. Returns false when it is unacceptable or taken.
    @discardableResult
    public func register(_ hotkey: Hotkey) -> Bool {
        unregister()
        guard Self.isAcceptable(hotkey), let mask = Self.carbonModifiers(hotkey.modifiers) else { return false }
        installHandlerOnce()
        let id = EventHotKeyID(signature: OSType(0x424F_4253), id: 1)  // "BOBS"
        var newRef: EventHotKeyRef?
        let status = RegisterEventHotKey(UInt32(hotkey.keyCode), mask, id, GetApplicationEventTarget(), 0, &newRef)
        guard status == noErr else { return false }
        ref = newRef
        return true
    }

    public func unregister() {
        if let ref { UnregisterEventHotKey(ref) }
        ref = nil
    }

    private func installHandlerOnce() {
        guard handler == nil else { return }
        var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        InstallEventHandler(GetApplicationEventTarget(), { _, _, refcon in
            guard let refcon else { return OSStatus(eventNotHandledErr) }
            let me = Unmanaged<HotkeyCenter>.fromOpaque(refcon).takeUnretainedValue()
            DispatchQueue.main.async { me.onPress?() }
            return noErr
        }, 1, &spec, refcon, &handler)
    }
}
