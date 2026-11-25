import Carbon.HIToolbox
import Foundation

/// Global hotkeys, registered with Carbon's `RegisterEventHotKey`: system-wide, no event tap, no extra
/// permission, and the key press is not delivered to the frontmost app. One slot per feature: Selection
/// Actions (press, SA-1) and Voice Dictation (hold: press and release, VD-1).
public final class HotkeyCenter {
    /// A registered hotkey's role: the Selection Actions menu, dictation, or a custom action's own shortcut.
    public struct Slot: Hashable, Sendable {
        public let rawValue: UInt32
        public init?(rawValue: UInt32) {
            guard rawValue == 1 || rawValue == 2 || rawValue >= Self.firstAction else { return nil }
            self.rawValue = rawValue
        }
        private init(_ raw: UInt32) { rawValue = raw }
        public static let selection = Slot(1)
        public static let dictation = Slot(2)
        static let firstAction: UInt32 = 100
        /// The shortcut of custom action `index`.
        public static func action(_ index: Int) -> Slot { Slot(firstAction + UInt32(max(index, 0))) }
        /// The custom action index, for an action slot.
        public var actionIndex: Int? { rawValue >= Self.firstAction ? Int(rawValue - Self.firstAction) : nil }
    }

    public static let defaultHotkey = Hotkey(keyCode: kVK_ANSI_J, modifiers: ["cmd", "shift"])
    public static let defaultDictationHotkey = Hotkey(keyCode: kVK_Space, modifiers: ["option"])
    static let signature = OSType(0x424F_4253)  // "BOBS"

    private var refs: [Slot: EventHotKeyRef] = [:]
    private var handler: EventHandlerRef?
    /// Called on the main thread.
    public var onPress: ((Slot) -> Void)?
    public var onRelease: ((Slot) -> Void)?

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

    /// Register `hotkey` in `slot`, replacing what was there. False when unacceptable or already taken
    /// (by BOB's other slot or another app).
    @discardableResult
    public func register(_ hotkey: Hotkey, slot: Slot = .selection) -> Bool {
        unregister(slot)
        guard Self.isAcceptable(hotkey), let mask = Self.carbonModifiers(hotkey.modifiers) else { return false }
        installHandlerOnce()
        var newRef: EventHotKeyRef?
        let id = EventHotKeyID(signature: Self.signature, id: slot.rawValue)
        let status = RegisterEventHotKey(UInt32(hotkey.keyCode), mask, id, GetApplicationEventTarget(), 0, &newRef)
        guard status == noErr, let newRef else { return false }
        refs[slot] = newRef
        return true
    }

    public func unregister(_ slot: Slot) {
        if let ref = refs.removeValue(forKey: slot) { UnregisterEventHotKey(ref) }
    }

    /// Replace all custom action shortcuts. Returns the indexes that could not be registered (invalid, or
    /// already taken by BOB's other hotkeys or another app).
    @discardableResult
    public func registerActions(_ hotkeys: [ActionHotkey]) -> [Int] {
        for slot in refs.keys where slot.actionIndex != nil { unregister(slot) }
        return hotkeys.compactMap { register($0.hotkey, slot: .action($0.index)) ? nil : $0.index }
    }

    private func installHandlerOnce() {
        guard handler == nil else { return }
        var specs = [
            EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed)),
            EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyReleased)),
        ]
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        InstallEventHandler(GetApplicationEventTarget(), { _, event, refcon in
            guard let refcon, let event else { return OSStatus(eventNotHandledErr) }
            var id = EventHotKeyID()
            let status = GetEventParameter(event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID),
                                           nil, MemoryLayout<EventHotKeyID>.size, nil, &id)
            guard status == noErr, id.signature == HotkeyCenter.signature,
                  let slot = Slot(rawValue: id.id) else { return OSStatus(eventNotHandledErr) }
            let pressed = GetEventKind(event) == UInt32(kEventHotKeyPressed)
            let me = Unmanaged<HotkeyCenter>.fromOpaque(refcon).takeUnretainedValue()
            DispatchQueue.main.async { pressed ? me.onPress?(slot) : me.onRelease?(slot) }
            return noErr
        }, specs.count, &specs, refcon, &handler)
    }
}
