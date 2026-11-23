import Foundation

// Swift mirror of `src-tauri/src/protocol.rs`. Both sides are tested against the shared fixtures in
// `Tests/Fixtures/`. Messages are newline-delimited JSON objects with a `type` field.
//
// PRIVACY: `textState.beforeCaret` may hold text typed by the user. Never log it (requirement NB-8).

/// A rectangle in screen points. Whether the origin is top-left (Accessibility) or bottom-left
/// (AppKit) is stated at each use; the wire format always uses **top-left** origin.
public struct Rect: Codable, Equatable, Sendable {
    public var x: Double
    public var y: Double
    public var width: Double
    public var height: Double

    public init(x: Double, y: Double, width: Double, height: Double) {
        self.x = x
        self.y = y
        self.width = width
        self.height = height
    }
}

public enum MicStatus: String, Codable, Equatable, Sendable {
    case granted
    case denied
    case undetermined
}

/// Keys the helper captures while the core asks for it.
public enum Key: String, Codable, Equatable, Sendable {
    case tab
    case shiftTab = "shift_tab"
    case escape
}

/// Messages sent by this helper to the core.
public enum NativeEvent: Equatable, Sendable {
    case hello(version: Int, pid: Int)
    case permissions(accessibility: Bool, microphone: MicStatus)
    case focus(appBundleId: String, appName: String, secure: Bool, editable: Bool)
    case textState(appBundleId: String, secure: Bool, beforeCaret: String, selectionLen: Int, caret: Rect?)
    case error(message: String)
    case key(Key)
    /// Any `type` this build does not know (decode only).
    case unknown
}

/// Messages sent by the core to this helper.
public enum CoreCommand: Equatable, Sendable {
    case helloAck(version: Int)
    case reject(reason: String)
    case showChip(text: String, anchor: Rect)
    case hideChip
    case insertText(text: String)
    case captureKeys(active: Bool)
    case requestPermissions
    case quit
    /// Any `type` this build does not know (decode only).
    case unknown
}

private struct Keys: CodingKey {
    var stringValue: String
    var intValue: Int? { nil }
    init(_ s: String) { stringValue = s }
    init?(stringValue: String) { self.stringValue = stringValue }
    init?(intValue: Int) { return nil }
}

extension NativeEvent: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        let type = try c.decode(String.self, forKey: Keys("type"))
        switch type {
        case "hello":
            self = .hello(version: try c.decode(Int.self, forKey: Keys("version")),
                          pid: try c.decode(Int.self, forKey: Keys("pid")))
        case "permissions":
            self = .permissions(accessibility: try c.decode(Bool.self, forKey: Keys("accessibility")),
                                microphone: try c.decode(MicStatus.self, forKey: Keys("microphone")))
        case "focus":
            self = .focus(appBundleId: try c.decode(String.self, forKey: Keys("app_bundle_id")),
                          appName: try c.decode(String.self, forKey: Keys("app_name")),
                          secure: try c.decode(Bool.self, forKey: Keys("secure")),
                          editable: try c.decode(Bool.self, forKey: Keys("editable")))
        case "text_state":
            self = .textState(appBundleId: try c.decode(String.self, forKey: Keys("app_bundle_id")),
                              secure: try c.decode(Bool.self, forKey: Keys("secure")),
                              beforeCaret: try c.decode(String.self, forKey: Keys("before_caret")),
                              selectionLen: try c.decode(Int.self, forKey: Keys("selection_len")),
                              caret: try c.decodeIfPresent(Rect.self, forKey: Keys("caret")))
        case "error":
            self = .error(message: try c.decode(String.self, forKey: Keys("message")))
        case "key":
            self = .key(try c.decode(Key.self, forKey: Keys("key")))
        default:
            self = .unknown
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        switch self {
        case let .hello(version, pid):
            try c.encode("hello", forKey: Keys("type"))
            try c.encode(version, forKey: Keys("version"))
            try c.encode(pid, forKey: Keys("pid"))
        case let .permissions(accessibility, microphone):
            try c.encode("permissions", forKey: Keys("type"))
            try c.encode(accessibility, forKey: Keys("accessibility"))
            try c.encode(microphone, forKey: Keys("microphone"))
        case let .focus(appBundleId, appName, secure, editable):
            try c.encode("focus", forKey: Keys("type"))
            try c.encode(appBundleId, forKey: Keys("app_bundle_id"))
            try c.encode(appName, forKey: Keys("app_name"))
            try c.encode(secure, forKey: Keys("secure"))
            try c.encode(editable, forKey: Keys("editable"))
        case let .textState(appBundleId, secure, beforeCaret, selectionLen, caret):
            try c.encode("text_state", forKey: Keys("type"))
            try c.encode(appBundleId, forKey: Keys("app_bundle_id"))
            try c.encode(secure, forKey: Keys("secure"))
            try c.encode(beforeCaret, forKey: Keys("before_caret"))
            try c.encode(selectionLen, forKey: Keys("selection_len"))
            try c.encode(caret, forKey: Keys("caret"))  // explicit null when absent
        case let .error(message):
            try c.encode("error", forKey: Keys("type"))
            try c.encode(message, forKey: Keys("message"))
        case let .key(key):
            try c.encode("key", forKey: Keys("type"))
            try c.encode(key, forKey: Keys("key"))
        case .unknown:
            try c.encode("unknown", forKey: Keys("type"))
        }
    }
}

extension CoreCommand: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        let type = try c.decode(String.self, forKey: Keys("type"))
        switch type {
        case "hello_ack":
            self = .helloAck(version: try c.decode(Int.self, forKey: Keys("version")))
        case "reject":
            self = .reject(reason: try c.decode(String.self, forKey: Keys("reason")))
        case "show_chip":
            self = .showChip(text: try c.decode(String.self, forKey: Keys("text")),
                             anchor: try c.decode(Rect.self, forKey: Keys("anchor")))
        case "hide_chip": self = .hideChip
        case "insert_text": self = .insertText(text: try c.decode(String.self, forKey: Keys("text")))
        case "capture_keys": self = .captureKeys(active: try c.decode(Bool.self, forKey: Keys("active")))
        case "request_permissions": self = .requestPermissions
        case "quit": self = .quit
        default: self = .unknown
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        switch self {
        case let .helloAck(version):
            try c.encode("hello_ack", forKey: Keys("type"))
            try c.encode(version, forKey: Keys("version"))
        case let .reject(reason):
            try c.encode("reject", forKey: Keys("type"))
            try c.encode(reason, forKey: Keys("reason"))
        case let .showChip(text, anchor):
            try c.encode("show_chip", forKey: Keys("type"))
            try c.encode(text, forKey: Keys("text"))
            try c.encode(anchor, forKey: Keys("anchor"))
        case .hideChip: try c.encode("hide_chip", forKey: Keys("type"))
        case let .insertText(text):
            try c.encode("insert_text", forKey: Keys("type"))
            try c.encode(text, forKey: Keys("text"))
        case let .captureKeys(active):
            try c.encode("capture_keys", forKey: Keys("type"))
            try c.encode(active, forKey: Keys("active"))
        case .requestPermissions: try c.encode("request_permissions", forKey: Keys("type"))
        case .quit: try c.encode("quit", forKey: Keys("type"))
        case .unknown: try c.encode("unknown", forKey: Keys("type"))
        }
    }
}

/// JSON-lines encoding helpers.
public enum ProtocolCodec {
    /// Encode a message as one JSON line including the trailing newline.
    public static func encodeLine<T: Encodable>(_ message: T) throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.withoutEscapingSlashes]
        var data = try encoder.encode(message)
        data.append(0x0A)
        return data
    }

    public static func decodeCommand(_ line: String) throws -> CoreCommand {
        try JSONDecoder().decode(CoreCommand.self, from: Data(line.utf8))
    }

    public static func decodeEvent(_ line: String) throws -> NativeEvent {
        try JSONDecoder().decode(NativeEvent.self, from: Data(line.utf8))
    }
}
