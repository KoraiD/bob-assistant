import Darwin
import Foundation

public enum SocketError: Error, Equatable {
    case pathTooLong(Int)
    case system(String, Int32)
    case closed
}

/// Minimal blocking Unix-domain-socket client speaking JSON lines (requirement NB-1).
/// Reads run on a dedicated thread; writes are serialized by a lock.
public final class SocketClient: @unchecked Sendable {
    private var fd: Int32
    private let writeLock = NSLock()
    private var closedFlag = false

    /// Wrap an already-connected socket (used by tests with `socketpair`).
    public init(fd: Int32) {
        self.fd = fd
        Self.disableSigPipe(fd)
    }

    deinit { close() }

    /// Connect to the listening core at `path`.
    public static func connect(path: String) throws -> SocketClient {
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let capacity = MemoryLayout.size(ofValue: addr.sun_path)
        let bytes = Array(path.utf8)
        guard bytes.count < capacity else { throw SocketError.pathTooLong(bytes.count) }

        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw SocketError.system("socket", errno) }

        withUnsafeMutableBytes(of: &addr.sun_path) { raw in
            for (i, b) in bytes.enumerated() { raw[i] = b }
            raw[bytes.count] = 0
        }
        let len = socklen_t(MemoryLayout<sockaddr_un>.size)
        let rc = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.connect(fd, $0, len) }
        }
        guard rc == 0 else {
            let e = errno
            Darwin.close(fd)
            throw SocketError.system("connect", e)
        }
        return SocketClient(fd: fd)
    }

    private static func disableSigPipe(_ fd: Int32) {
        var one: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &one, socklen_t(MemoryLayout<Int32>.size))
    }

    /// Send one message as a JSON line.
    public func send<T: Encodable>(_ message: T) throws {
        let data = try ProtocolCodec.encodeLine(message)
        writeLock.lock()
        defer { writeLock.unlock() }
        guard !closedFlag else { throw SocketError.closed }
        var offset = 0
        try data.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
            while offset < raw.count {
                let n = Darwin.write(fd, raw.baseAddress!.advanced(by: offset), raw.count - offset)
                if n < 0 {
                    if errno == EINTR { continue }
                    throw SocketError.system("write", errno)
                }
                offset += n
            }
        }
    }

    /// Start a background reader. `onCommand` is called on the reader thread for every decoded
    /// command (unknown types are skipped, NB-3). `onClose` is called once at EOF or error.
    public func startReading(onCommand: @escaping (CoreCommand) -> Void, onClose: @escaping () -> Void) {
        let fd = self.fd
        let thread = Thread {
            var decoder = LineDecoder()
            var buffer = [UInt8](repeating: 0, count: 8192)
            while true {
                let n = buffer.withUnsafeMutableBytes { Darwin.read(fd, $0.baseAddress, $0.count) }
                if n < 0 && errno == EINTR { continue }
                if n <= 0 { break }
                for frame in decoder.push(Data(buffer[0..<n])) {
                    guard case let .line(line) = frame,
                          let command = try? ProtocolCodec.decodeCommand(line),
                          command != .unknown else { continue }
                    onCommand(command)
                }
            }
            onClose()
        }
        thread.name = "bob-native.socket-reader"
        thread.start()
    }

    public func close() {
        writeLock.lock()
        defer { writeLock.unlock() }
        guard !closedFlag else { return }
        closedFlag = true
        Darwin.shutdown(fd, SHUT_RDWR)
        Darwin.close(fd)
    }
}
