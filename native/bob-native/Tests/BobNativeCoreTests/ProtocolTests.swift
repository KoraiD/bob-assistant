import Foundation
import Testing
@testable import BobNativeCore

/// Reads the fixture files shared with the Rust tests (`src-tauri/src/protocol.rs`).
enum Fixtures {
    static func lines(_ name: String, file: String = #filePath) throws -> [String] {
        let dir = URL(fileURLWithPath: file).deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Fixtures")
        let text = try String(contentsOf: dir.appendingPathComponent(name), encoding: .utf8)
        return text.split(separator: "\n").map(String.init).filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
    }
}

/// Compares two JSON documents structurally (ignores key order and number formatting).
func jsonEqual(_ a: Data, _ b: Data) -> Bool {
    guard let x = try? JSONSerialization.jsonObject(with: a, options: [.fragmentsAllowed]),
          let y = try? JSONSerialization.jsonObject(with: b, options: [.fragmentsAllowed]) else { return false }
    return (x as AnyObject).isEqual(y)
}

@Suite("Protocol fixtures (shared with Rust)")
struct ProtocolFixtureTests {
    @Test func everyEventFixtureDecodesAndReencodesToTheSameJSON() throws {
        let lines = try Fixtures.lines("native_events.jsonl")
        #expect(lines.count >= 6)
        for line in lines {
            let event = try ProtocolCodec.decodeEvent(line)
            #expect(event != .unknown, "decoded as unknown: \(line)")
            let encoded = try ProtocolCodec.encodeLine(event)
            #expect(jsonEqual(encoded, Data(line.utf8)), "round trip changed: \(line)")
        }
    }

    @Test func everyCommandFixtureDecodesAndReencodesToTheSameJSON() throws {
        for line in try Fixtures.lines("core_commands.jsonl") {
            let command = try ProtocolCodec.decodeCommand(line)
            #expect(command != .unknown, "decoded as unknown: \(line)")
            let encoded = try ProtocolCodec.encodeLine(command)
            #expect(jsonEqual(encoded, Data(line.utf8)), "round trip changed: \(line)")
        }
    }

    @Test func unknownTypesAreToleratedNotFatal() throws {
        #expect(try ProtocolCodec.decodeEvent(#"{"type":"from_the_future","x":1}"#) == .unknown)
        #expect(try ProtocolCodec.decodeCommand(#"{"type":"from_the_future"}"#) == .unknown)
    }

    @Test func missingTypeThrowsInsteadOfCrashing() {
        #expect(throws: (any Error).self) { try ProtocolCodec.decodeEvent(#"{"version":1}"#) }
        #expect(throws: (any Error).self) { try ProtocolCodec.decodeEvent("not json") }
    }

    @Test func textStateEncodesAnExplicitNullCaret() throws {
        let data = try ProtocolCodec.encodeLine(
            NativeEvent.textState(appBundleId: "a", secure: false, beforeCaret: "", selectionLen: 0, caret: nil))
        #expect(String(decoding: data, as: UTF8.self).contains(#""caret":null"#))
    }

    @Test func protocolVersionMatchesTheCore() {
        // Keep in sync with PROTOCOL_VERSION in src-tauri/src/protocol.rs.
        #expect(BobNativeInfo.protocolVersion == 5)
    }
}

@Suite("LineDecoder")
struct LineDecoderTests {
    @Test func handlesSplitAndJoinedChunks() {
        var d = LineDecoder()
        #expect(d.push(Data(#"{"type":"qu"#.utf8)).isEmpty)
        let frames = d.push(Data("it\"}\n{\"type\":\"hide_chip\"}\n".utf8))
        #expect(frames == [.line(#"{"type":"quit"}"#), .line(#"{"type":"hide_chip"}"#)])
    }

    @Test func skipsBlankLinesAndTrimsCRLF() {
        var d = LineDecoder()
        #expect(d.push(Data("\n\r\nabc\r\n".utf8)) == [.line("abc")])
    }

    @Test func dropsOversizedLinesAndRecovers() {
        var d = LineDecoder()
        var frames = d.push(Data(repeating: UInt8(ascii: "a"), count: LineDecoder.maxLineBytes + 10))
        frames += d.push(Data("tail\nok\n".utf8))
        #expect(frames == [.tooLong, .line("ok")])
    }

    @Test func reportsInvalidUTF8AndContinues() {
        var d = LineDecoder()
        #expect(d.push(Data([0xff, 0xfe, 0x0A, 0x6F, 0x6B, 0x0A])) == [.invalidUTF8, .line("ok")])
    }

    /// Randomized check of requirement NB-4 (seeded, so failures are reproducible):
    /// decoding must not depend on how the byte stream is chunked.
    @Test func decodingIsIndependentOfChunking() throws {
        var rng = SeededGenerator(seed: 0xB0B)
        let samples = try Fixtures.lines("native_events.jsonl") + Fixtures.lines("core_commands.jsonl")
        for _ in 0..<300 {
            let picked = (0..<Int.random(in: 0...8, using: &rng)).map { _ in samples.randomElement(using: &rng)! }
            let stream = Data(picked.map { $0 + "\n" }.joined().utf8)
            var cuts = (0..<Int.random(in: 0...10, using: &rng)).map { _ in Int.random(in: 0...stream.count, using: &rng) }
            cuts = Array(Set(cuts + [0, stream.count])).sorted()
            var d = LineDecoder()
            var out = [String]()
            for (a, b) in zip(cuts, cuts.dropFirst()) {
                for case let .line(l) in d.push(stream.subdata(in: a..<b)) { out.append(l) }
            }
            #expect(out == picked)
        }
    }
}

/// Small deterministic RNG (SplitMix64) so randomized tests are reproducible.
struct SeededGenerator: RandomNumberGenerator {
    var state: UInt64
    init(seed: UInt64) { state = seed }
    mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
        return z ^ (z >> 31)
    }
}
