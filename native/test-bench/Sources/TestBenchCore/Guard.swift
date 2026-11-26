import Foundation

/// Marker in `eventSourceUserData` of every key the Test Bench posts (BOB uses 0x0B0B0B0B for its own).
public let benchEventMarker: Int64 = 0x7E57_BE7C
public let bobEventMarker: Int64 = 0x0B0B_0B0B

/// What the guard looks at before every key. The Test Bench only ever types into its own editor.
public struct GuardState: Sendable {
    /// The editor has focus, its window is key and the Test Bench is the frontmost app.
    public var editorInFront: Bool
    /// Seconds since the mouse was last used.
    public var mouseIdleS: Double
    /// Set when someone else pressed a key during the run.
    public var foreignKey: Bool
    public var activeMs: Double
    public var limitMs: Double
    /// How long the run has been paused so far (this pause).
    public var pausedMs: Double
    public var maxPauseMs: Double

    public init(editorInFront: Bool, mouseIdleS: Double, foreignKey: Bool, activeMs: Double, limitMs: Double,
                pausedMs: Double = 0, maxPauseMs: Double = 15 * 60_000) {
        self.editorInFront = editorInFront
        self.mouseIdleS = mouseIdleS
        self.foreignKey = foreignKey
        self.activeMs = activeMs
        self.limitMs = limitMs
        self.pausedMs = pausedMs
        self.maxPauseMs = maxPauseMs
    }
}

public enum GuardDecision: Equatable, Sendable {
    case type
    case pause
    case stop(String)
}

/// Keys from anyone else stop the run; the mouse, or another window in front, only pauses it until the mouse
/// rests for 3 s with the editor in front again.
public func decide(_ s: GuardState) -> GuardDecision {
    if s.foreignKey { return .stop("you used the keyboard, so the run stopped") }
    if s.activeMs >= s.limitMs { return .stop("time is up") }
    if s.editorInFront && s.mouseIdleS >= 3 { return .type }
    if s.pausedMs > s.maxPauseMs { return .stop("paused for 15 minutes, so the run stopped") }
    return .pause
}

/// Whether an observed input event came from the Test Bench or BOB (allowed) or from a person.
public func isOwnEvent(userData: Int64) -> Bool {
    userData == benchEventMarker || userData == bobEventMarker
}
