/// Identity and protocol constants for the native helper.
///
/// The wire protocol (Swift <-> Rust, JSON lines over a Unix socket) is defined in Task 2;
/// `protocolVersion` lets both sides refuse to talk when they disagree.
public enum BobNativeInfo {
    /// Helper name, used in logs and the IPC hello message.
    public static let name = "bob-native"

    /// Bumped whenever the IPC message schema changes incompatibly.
    public static let protocolVersion = 0

    /// One-line banner printed by `bob-native --version`.
    public static func banner() -> String {
        "\(name) protocol=\(protocolVersion)"
    }
}
