/// Identity and protocol constants for the native helper.
public enum BobNativeInfo {
    /// Helper name, used in logs and the IPC hello message.
    public static let name = "bob-native"

    /// Wire protocol version. Must equal `PROTOCOL_VERSION` in `src-tauri/src/protocol.rs`.
    /// Bumped whenever the IPC message schema changes incompatibly.
    public static let protocolVersion = 2

    /// One-line banner printed by `bob-native --version`.
    public static func banner() -> String {
        "\(name) protocol=\(protocolVersion)"
    }
}
