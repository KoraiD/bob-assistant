import Darwin
import Foundation
import Testing
@testable import BobNativeCore

@Suite("SocketClient")
struct SocketClientTests {
    private func makePair() -> (SocketClient, Int32) {
        var fds: [Int32] = [0, 0]
        #expect(socketpair(AF_UNIX, SOCK_STREAM, 0, &fds) == 0)
        return (SocketClient(fd: fds[0]), fds[1])
    }

    private func readLine(from fd: Int32) -> String {
        var bytes = [UInt8]()
        var byte: UInt8 = 0
        while Darwin.read(fd, &byte, 1) == 1 {
            if byte == 0x0A { break }
            bytes.append(byte)
        }
        return String(decoding: bytes, as: UTF8.self)
    }

    @Test func sendsOneJSONLinePerMessage() throws {
        let (client, peer) = makePair()
        defer { client.close(); Darwin.close(peer) }
        try client.send(NativeEvent.hello(version: 1, pid: 42))
        let line = readLine(from: peer)
        #expect(try ProtocolCodec.decodeEvent(line) == .hello(version: 1, pid: 42))
    }

    @Test func deliversDecodedCommandsAndSkipsUnknownOnes() async throws {
        let (client, peer) = makePair()
        defer { client.close(); Darwin.close(peer) }

        let received = LockedBox<[CoreCommand]>([])
        let closed = DispatchSemaphore(value: 0)
        client.startReading(onCommand: { cmd in received.mutate { $0.append(cmd) } },
                            onClose: { closed.signal() })

        let payload = #"{"type":"hello_ack","version":1}"# + "\n"
            + #"{"type":"from_the_future"}"# + "\n"
            + "garbage\n"
            + #"{"type":"hide_chip"}"# + "\n"
        // Write in awkward chunks to exercise reassembly.
        let bytes = Array(payload.utf8)
        for chunk in stride(from: 0, to: bytes.count, by: 7) {
            let slice = Array(bytes[chunk..<min(chunk + 7, bytes.count)])
            _ = slice.withUnsafeBytes { Darwin.write(peer, $0.baseAddress, $0.count) }
        }
        Darwin.shutdown(peer, SHUT_WR)  // EOF -> onClose

        #expect(closed.wait(timeout: .now() + 5) == .success)
        #expect(received.value == [.helloAck(version: 1), .hideChip])
    }

    @Test func sendingAfterCloseThrows() {
        let (client, peer) = makePair()
        defer { Darwin.close(peer) }
        client.close()
        #expect(throws: SocketError.closed) { try client.send(NativeEvent.error(message: "x")) }
    }

    @Test func connectRejectsOverlongPathsInsteadOfTruncating() {
        let long = "/tmp/" + String(repeating: "a", count: 200)
        #expect(throws: SocketError.pathTooLong(long.utf8.count)) { try SocketClient.connect(path: long) }
    }

    @Test func connectToAMissingSocketFailsCleanly() {
        #expect(throws: (any Error).self) { try SocketClient.connect(path: "/tmp/bob-does-not-exist.sock") }
    }
}

/// Tiny thread-safe box for collecting values from the reader thread.
final class LockedBox<T>: @unchecked Sendable {
    private var stored: T
    private let lock = NSLock()
    init(_ value: T) { stored = value }
    var value: T { lock.lock(); defer { lock.unlock() }; return stored }
    func mutate(_ body: (inout T) -> Void) { lock.lock(); body(&stored); lock.unlock() }
}
