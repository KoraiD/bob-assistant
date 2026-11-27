import ApplicationServices
import Carbon.HIToolbox

/// What the helper could learn about the focused field right before it types.
public enum FocusedField: Equatable, Sendable {
    case readable(secure: Bool)
    /// No focused element, or Accessibility did not answer (a busy app, the 0.4 s timeout, an app
    /// without Accessibility).
    case unreadable
}

/// Why typing was refused; the raw value is the `insert_refused` reason sent to the core.
public enum InsertRefusal: String, Equatable, Sendable {
    case secureInput = "secure_input"
    case secureField = "secure_field"
    case unreadable
}

/// The last check before the helper types (code review SEC-09). It fails closed: text is typed only when
/// macOS secure input is off and the focused field could be read and is not a password field.
public enum InsertGuard {
    /// Pure decision, tested without the OS.
    public static func decide(secureEventInput: Bool, field: FocusedField) -> InsertRefusal? {
        // Secure input first: it is cheap, needs no Accessibility call, and is on while a password field
        // (or an app protecting its keyboard input) has the keyboard.
        if secureEventInput { return .secureInput }
        switch field {
        case .unreadable: return .unreadable
        case .readable(secure: true): return .secureField
        case .readable(secure: false): return nil
        }
    }

    /// The live check: `IsSecureEventInputEnabled` and the focused element's role and subrole.
    public static func check() -> InsertRefusal? {
        decide(secureEventInput: IsSecureEventInputEnabled(), field: focusedField())
    }

    static func focusedField() -> FocusedField {
        guard let focused = AXReader.focusedElement(),
              AXReader.string(focused, kAXRoleAttribute) != nil else { return .unreadable }
        return .readable(secure: AXReader.string(focused, kAXSubroleAttribute) == (kAXSecureTextFieldSubrole as String))
    }
}
