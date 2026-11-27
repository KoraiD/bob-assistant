import AVFoundation
import Foundation
import Testing
@testable import BobNativeCore

@Suite("Test Bench audio file")
struct TestAudioFileTests {
    /// One second of a 220 Hz tone at 16 kHz mono, written to a temporary WAV file.
    func writeTone() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("bob-tone-\(UUID().uuidString).wav")
        let format = DictationAudio.format
        let frames = AVAudioFrameCount(16_000)
        let buffer = try #require(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames))
        buffer.frameLength = frames
        let data = try #require(buffer.floatChannelData)
        for frame in 0..<Int(frames) { data[0][frame] = 0.2 * sin(Float(frame) * 2 * .pi * 220 / 16_000) }
        let file = try AVAudioFile(forWriting: url, settings: format.settings)
        try file.write(from: buffer)
        return url
    }

    @Test func theRecorderReturnsTheFilesSamplesWithoutTheMicrophone() throws {
        let url = try writeTone()
        defer { try? FileManager.default.removeItem(at: url) }
        let expected = try DictationAudio.load(url: url)
        #expect(expected.count > 15_000)
        let recorder = AudioRecorder(testAudioFile: url)
        try recorder.start()
        #expect(recorder.isRecording)
        #expect(recorder.stop(keep: true) == expected)
        #expect(!recorder.isRecording)
    }

    @Test func aCancelledRecordingIsEmpty() throws {
        let url = try writeTone()
        defer { try? FileManager.default.removeItem(at: url) }
        let recorder = AudioRecorder(testAudioFile: url)
        try recorder.start()
        #expect(recorder.stop(keep: false).isEmpty)
        // Stopping again is harmless.
        #expect(recorder.stop(keep: true).isEmpty)
    }

    @Test func aMissingFileFailsToStart() {
        let recorder = AudioRecorder(testAudioFile: URL(fileURLWithPath: "/nonexistent/bob.wav"))
        #expect(throws: AudioRecorder.Failure.self) { try recorder.start() }
        #expect(!recorder.isRecording)
    }

    @Test func theHookIsOffInNormalBuilds() {
        if !TestHooks.enabled {
            #expect(TestHooks.audioFile == nil)
            // Code review SEC-02: the stall switch is compiled out too, whatever the environment says.
            #expect(TestHooks.mainStallMs == nil)
        }
    }
    @Test func theStallSwitchAcceptsOnlyAPositiveBoundedValue() {
        #expect(TestHooks.stallMs(from: ["BOB_TEST_MAIN_STALL_MS": "1500"]) == 1500)
        for bad in ["", "0", "-5", "abc", "10001"] {
            #expect(TestHooks.stallMs(from: ["BOB_TEST_MAIN_STALL_MS": bad]) == nil, "\(bad)")
        }
        #expect(TestHooks.stallMs(from: [:]) == nil)
    }
}
