import Foundation

/// Marker in `eventSourceUserData` of every key the Test Bench posts (BOB uses 0x0B0B0B0B for its own).
public let benchEventMarker: Int64 = 0x7E57_BE7C
public let bobEventMarker: Int64 = 0x0B0B_0B0B

/// What the guard looks at before every key. The Test Bench only ever types into its own editor.
public struct GuardState: Sendable {
    public var frontmostPID: Int32?
    public var ownPID: Int32
    public var windowIsKey: Bool
    public var editorHasFocus: Bool
    /// Set by the input monitor when a key or mouse event came from someone else.
    public var foreignInput: String?
    public var elapsedMs: Double
    public var limitMs: Double

    public init(frontmostPID: Int32?, ownPID: Int32, windowIsKey: Bool, editorHasFocus: Bool,
                foreignInput: String?, elapsedMs: Double, limitMs: Double) {
        self.frontmostPID = frontmostPID
        self.ownPID = ownPID
        self.windowIsKey = windowIsKey
        self.editorHasFocus = editorHasFocus
        self.foreignInput = foreignInput
        self.elapsedMs = elapsedMs
        self.limitMs = limitMs
    }
}

/// Nil when typing may go on; otherwise why the run stops.
public func stopReason(_ s: GuardState) -> String? {
    if let input = s.foreignInput { return "you used the \(input), so the run stopped" }
    guard s.frontmostPID == s.ownPID else { return "another app came to the front" }
    guard s.windowIsKey, s.editorHasFocus else { return "the Test Bench editor lost focus" }
    if s.elapsedMs >= s.limitMs { return "time is up" }
    return nil
}

/// Whether an observed input event came from the Test Bench or BOB (allowed) or from a person (stop).
public func isOwnEvent(userData: Int64) -> Bool {
    userData == benchEventMarker || userData == bobEventMarker
}
