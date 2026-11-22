import Foundation

/// One decoded frame from the byte stream.
public enum Frame: Equatable, Sendable {
    case line(String)
    /// A line exceeded `LineDecoder.maxLineBytes` and was dropped up to the next newline.
    case tooLong
    case invalidUTF8
}

/// Incremental newline-delimited frame decoder: feed it arbitrary byte chunks (requirement NB-4).
/// Mirrors `LineDecoder` in `src-tauri/src/protocol.rs`.
public struct LineDecoder: Sendable {
    public static let maxLineBytes = 1 << 20

    private var buffer = [UInt8]()
    private var discarding = false

    public init() {}

    public mutating func push(_ chunk: Data) -> [Frame] {
        var out = [Frame]()
        for byte in chunk {
            if byte == 0x0A {
                if discarding {
                    discarding = false
                    buffer.removeAll(keepingCapacity: true)
                } else if !buffer.isEmpty {
                    let bytes = buffer
                    buffer.removeAll(keepingCapacity: true)
                    if let s = String(validatingUTF8Bytes: bytes) {
                        let trimmed = s.hasSuffix("\r") ? String(s.dropLast()) : s
                        if !trimmed.isEmpty { out.append(.line(trimmed)) }
                    } else {
                        out.append(.invalidUTF8)
                    }
                }
            } else if !discarding {
                buffer.append(byte)
                if buffer.count > Self.maxLineBytes {
                    buffer.removeAll(keepingCapacity: true)
                    discarding = true
                    out.append(.tooLong)
                }
            }
        }
        return out
    }
}

extension String {
    /// Strict UTF-8 decoding: nil when the bytes are not valid UTF-8.
    init?(validatingUTF8Bytes bytes: [UInt8]) {
        // Foundation's initializer returns nil for invalid UTF-8 and works on macOS 14
        // (`String(validating:as:)` needs macOS 15).
        guard let s = String(bytes: bytes, encoding: .utf8) else { return nil }
        self = s
    }
}
