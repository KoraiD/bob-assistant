import Foundation

// Swift mirror of `src-tauri/src/protocol.rs`. Both sides are tested against the shared fixtures in
// `Tests/Fixtures/`. Messages are newline-delimited JSON objects with a `type` field.
//
// PRIVACY: `textState.beforeCaret`, `contextState` and `screenText` hold user text. Never log them (NB-8).

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

/// How much of the screen the helper may read for context (PC-4). Mirrors `ContextScope` in Rust.
public enum ContextScope: String, Codable, Equatable, Sendable {
    case accessibility
    case activeWindow = "active_window"
    case allScreens = "all_screens"
}

/// Where the selection was read from (SA-2).
public enum SelectionSource: String, Codable, Equatable, Sendable {
    case accessibility
    case copy
}

/// A global hotkey: virtual key code plus modifier names ("cmd", "shift", "option", "control").
public struct Hotkey: Codable, Equatable, Sendable {
    public var keyCode: Int
    public var modifiers: [String]
    public init(keyCode: Int, modifiers: [String]) {
        self.keyCode = keyCode
        self.modifiers = modifiers
    }
    enum CodingKeys: String, CodingKey {
        case keyCode = "key_code"
        case modifiers
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
    case permissions(accessibility: Bool, microphone: MicStatus, screenRecording: Bool)
    case focus(appBundleId: String, appName: String, secure: Bool, editable: Bool)
    case textState(appBundleId: String, secure: Bool, beforeCaret: String, selectionLen: Int, caret: Rect?)
    /// Window title and text after the caret (PC-3). Never for secure fields.
    case contextState(appBundleId: String, windowTitle: String?, afterCaret: String)
    /// Text recognized on screen (PC-4).
    case screenText(scope: ContextScope, appBundleId: String, text: String, ms: Int)
    /// The Selection Actions hotkey was pressed (SA-1); empty text when nothing is selected or secure.
    case selection(appBundleId: String, text: String, secure: Bool, source: SelectionSource?, anchor: Rect?)
    case menuChoice(index: Int)
    case menuCancel
    case replaceResult(ok: Bool, method: String?, error: String?)
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
    case setContextMode(scope: ContextScope, exclude: [String])
    case requestScreenRecording
    case showMenu(title: String, items: [String], anchor: Rect?)
    case menuStatus(text: String, busy: Bool, error: Bool)
    case hideMenu
    case replaceSelection(expected: String, text: String)
    case setHotkey(Hotkey)
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
                                microphone: try c.decode(MicStatus.self, forKey: Keys("microphone")),
                                screenRecording: try c.decodeIfPresent(Bool.self, forKey: Keys("screen_recording")) ?? false)
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
        case "context_state":
            self = .contextState(appBundleId: try c.decode(String.self, forKey: Keys("app_bundle_id")),
                                 windowTitle: try c.decodeIfPresent(String.self, forKey: Keys("window_title")),
                                 afterCaret: try c.decode(String.self, forKey: Keys("after_caret")))
        case "screen_text":
            self = .screenText(scope: try c.decode(ContextScope.self, forKey: Keys("scope")),
                               appBundleId: try c.decode(String.self, forKey: Keys("app_bundle_id")),
                               text: try c.decode(String.self, forKey: Keys("text")),
                               ms: try c.decode(Int.self, forKey: Keys("ms")))
        case "selection":
            self = .selection(appBundleId: try c.decode(String.self, forKey: Keys("app_bundle_id")),
                              text: try c.decode(String.self, forKey: Keys("text")),
                              secure: try c.decode(Bool.self, forKey: Keys("secure")),
                              source: try c.decodeIfPresent(SelectionSource.self, forKey: Keys("source")),
                              anchor: try c.decodeIfPresent(Rect.self, forKey: Keys("anchor")))
        case "menu_choice":
            self = .menuChoice(index: try c.decode(Int.self, forKey: Keys("index")))
        case "menu_cancel":
            self = .menuCancel
        case "replace_result":
            self = .replaceResult(ok: try c.decode(Bool.self, forKey: Keys("ok")),
                                  method: try c.decodeIfPresent(String.self, forKey: Keys("method")),
                                  error: try c.decodeIfPresent(String.self, forKey: Keys("error")))
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
        case let .permissions(accessibility, microphone, screenRecording):
            try c.encode("permissions", forKey: Keys("type"))
            try c.encode(accessibility, forKey: Keys("accessibility"))
            try c.encode(microphone, forKey: Keys("microphone"))
            try c.encode(screenRecording, forKey: Keys("screen_recording"))
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
        case let .contextState(appBundleId, windowTitle, afterCaret):
            try c.encode("context_state", forKey: Keys("type"))
            try c.encode(appBundleId, forKey: Keys("app_bundle_id"))
            try c.encode(windowTitle, forKey: Keys("window_title"))  // explicit null when absent
            try c.encode(afterCaret, forKey: Keys("after_caret"))
        case let .screenText(scope, appBundleId, text, ms):
            try c.encode("screen_text", forKey: Keys("type"))
            try c.encode(scope, forKey: Keys("scope"))
            try c.encode(appBundleId, forKey: Keys("app_bundle_id"))
            try c.encode(text, forKey: Keys("text"))
            try c.encode(ms, forKey: Keys("ms"))
        case let .selection(appBundleId, text, secure, source, anchor):
            try c.encode("selection", forKey: Keys("type"))
            try c.encode(appBundleId, forKey: Keys("app_bundle_id"))
            try c.encode(text, forKey: Keys("text"))
            try c.encode(secure, forKey: Keys("secure"))
            try c.encode(source, forKey: Keys("source"))  // explicit null when absent
            try c.encode(anchor, forKey: Keys("anchor"))
        case let .menuChoice(index):
            try c.encode("menu_choice", forKey: Keys("type"))
            try c.encode(index, forKey: Keys("index"))
        case .menuCancel:
            try c.encode("menu_cancel", forKey: Keys("type"))
        case let .replaceResult(ok, method, error):
            try c.encode("replace_result", forKey: Keys("type"))
            try c.encode(ok, forKey: Keys("ok"))
            try c.encode(method, forKey: Keys("method"))
            try c.encode(error, forKey: Keys("error"))
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
        case "set_context_mode":
            self = .setContextMode(scope: try c.decode(ContextScope.self, forKey: Keys("scope")),
                                   exclude: try c.decodeIfPresent([String].self, forKey: Keys("exclude")) ?? [])
        case "request_screen_recording": self = .requestScreenRecording
        case "show_menu":
            self = .showMenu(title: try c.decode(String.self, forKey: Keys("title")),
                             items: try c.decode([String].self, forKey: Keys("items")),
                             anchor: try c.decodeIfPresent(Rect.self, forKey: Keys("anchor")))
        case "menu_status":
            self = .menuStatus(text: try c.decode(String.self, forKey: Keys("text")),
                               busy: try c.decode(Bool.self, forKey: Keys("busy")),
                               error: try c.decode(Bool.self, forKey: Keys("error")))
        case "hide_menu": self = .hideMenu
        case "replace_selection":
            self = .replaceSelection(expected: try c.decode(String.self, forKey: Keys("expected")),
                                     text: try c.decode(String.self, forKey: Keys("text")))
        case "set_hotkey": self = .setHotkey(try c.decode(Hotkey.self, forKey: Keys("hotkey")))
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
        case let .setContextMode(scope, exclude):
            try c.encode("set_context_mode", forKey: Keys("type"))
            try c.encode(scope, forKey: Keys("scope"))
            try c.encode(exclude, forKey: Keys("exclude"))
        case .requestScreenRecording: try c.encode("request_screen_recording", forKey: Keys("type"))
        case let .showMenu(title, items, anchor):
            try c.encode("show_menu", forKey: Keys("type"))
            try c.encode(title, forKey: Keys("title"))
            try c.encode(items, forKey: Keys("items"))
            try c.encode(anchor, forKey: Keys("anchor"))
        case let .menuStatus(text, busy, error):
            try c.encode("menu_status", forKey: Keys("type"))
            try c.encode(text, forKey: Keys("text"))
            try c.encode(busy, forKey: Keys("busy"))
            try c.encode(error, forKey: Keys("error"))
        case .hideMenu: try c.encode("hide_menu", forKey: Keys("type"))
        case let .replaceSelection(expected, text):
            try c.encode("replace_selection", forKey: Keys("type"))
            try c.encode(expected, forKey: Keys("expected"))
            try c.encode(text, forKey: Keys("text"))
        case let .setHotkey(hotkey):
            try c.encode("set_hotkey", forKey: Keys("type"))
            try c.encode(hotkey, forKey: Keys("hotkey"))
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
