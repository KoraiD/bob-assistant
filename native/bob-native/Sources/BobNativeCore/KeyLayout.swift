import Carbon.HIToolbox
import CoreGraphics

/// Key codes for letters in the current keyboard layout. Shortcuts like Cmd+V are matched by the character
/// the key types, so on Dvorak or other layouts the physical "V" key is not Cmd+V.
public enum KeyLayout {
    /// The key code that types `letter` (lowercase) in the current layout, or `fallback` when none does.
    public static func code(for letter: Character, fallback: Int) -> CGKeyCode {
        guard let source = TISCopyCurrentKeyboardLayoutInputSource()?.takeRetainedValue(),
              let raw = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData) else {
            return CGKeyCode(fallback)
        }
        let data = Unmanaged<CFData>.fromOpaque(raw).takeUnretainedValue() as Data
        let target = String(letter).lowercased()
        return data.withUnsafeBytes { buf -> CGKeyCode in
            guard let layout = buf.baseAddress?.assumingMemoryBound(to: UCKeyboardLayout.self) else {
                return CGKeyCode(fallback)
            }
            for code in 0..<UInt16(128) {
                var dead: UInt32 = 0
                var chars = [UniChar](repeating: 0, count: 4)
                var length = 0
                let status = UCKeyTranslate(layout, code, UInt16(kUCKeyActionDown), 0, UInt32(LMGetKbdType()),
                                            OptionBits(kUCKeyTranslateNoDeadKeysBit), &dead, chars.count, &length, &chars)
                if status == noErr, length == 1, String(utf16CodeUnits: chars, count: 1) == target {
                    return CGKeyCode(code)
                }
            }
            return CGKeyCode(fallback)
        }
    }
}
