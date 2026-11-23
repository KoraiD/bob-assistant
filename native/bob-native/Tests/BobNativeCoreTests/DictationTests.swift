import Carbon.HIToolbox
import CoreGraphics
import Foundation
import Testing
@testable import BobNativeCore

@Suite("SpeechGate (VD-5)")
struct SpeechGateTests {
    func tone(seconds: Double, amplitude: Float, from start: Double = 0) -> [Float] {
        (0..<Int(seconds * 16_000)).map { i in amplitude * sin(Float(i + Int(start * 16_000)) * 2 * .pi * 220 / 16_000) }
    }

    @Test func silenceAndLowNoiseAreNotSpeech() {
        #expect(!SpeechGate.hasSpeech([Float](repeating: 0, count: 32_000)))
        var rng = SystemRandomNumberGenerator()
        let hiss = (0..<32_000).map { _ in Float.random(in: -0.001...0.001, using: &rng) }
        #expect(!SpeechGate.hasSpeech(hiss))
        #expect(!SpeechGate.hasSpeech([]))
    }

    @Test func aVoiceAboveTheNoiseFloorIsSpeech() {
        var rng = SystemRandomNumberGenerator()
        let noise = (0..<16_000).map { _ in Float.random(in: -0.002...0.002, using: &rng) }
        let voice = tone(seconds: 0.6, amplitude: 0.1)
        #expect(SpeechGate.hasSpeech(noise + voice + noise))
    }

    @Test func aClickIsNotSpeech() {
        var samples = [Float](repeating: 0, count: 16_000)
        for i in 0..<480 { samples[8000 + i] = 0.5 }  // one 30 ms burst
        #expect(!SpeechGate.hasSpeech(samples))
    }

    @Test func levelIsBounded() {
        #expect(SpeechGate.level([0, 0, 0][...]) == 0)
        #expect(SpeechGate.level([1, -1, 1][...]) == 1)
        #expect(SpeechGate.level([][...]) == 0)
    }
}

@Suite("TranscriptCleaner")
struct TranscriptCleanerTests {
    @Test func dropsWhisperAnnotationsAndNormalizesSpaces() {
        #expect(TranscriptCleaner.clean(" [BLANK_AUDIO] Hello  there.\n") == "Hello there.")
        #expect(TranscriptCleaner.clean("(music) ♪♪ Thanks (applause)") == "Thanks")
        #expect(TranscriptCleaner.clean("A (small) detail") == "A (small) detail")
    }
}

@Suite("OrbLayout")
struct OrbLayoutTests {
    let screen = CGRect(x: 0, y: 0, width: 1440, height: 900)

    @Test func sitsAboveRightOfTheCaretAndStaysOnScreen() {
        let p = OrbLayout.origin(caret: CGRect(x: 100, y: 400, width: 1, height: 18), mouse: .zero, visibleFrame: screen)
        #expect(p == CGPoint(x: 107, y: 424))
        let top = OrbLayout.origin(caret: CGRect(x: 1430, y: 880, width: 1, height: 18), mouse: .zero, visibleFrame: screen)
        #expect(top.y < 880 && top.x + OrbLayout.size.width <= screen.maxX)
        let mouse = OrbLayout.origin(caret: nil, mouse: CGPoint(x: 50, y: 50), visibleFrame: screen)
        #expect(mouse == CGPoint(x: 64, y: 64))
    }
}

@Suite("Dictation hotkey")
struct DictationHotkeyTests {
    @Test func defaultIsOptionSpaceAndAcceptable() {
        #expect(HotkeyCenter.defaultDictationHotkey == Hotkey(keyCode: kVK_Space, modifiers: ["option"]))
        #expect(HotkeyCenter.isAcceptable(HotkeyCenter.defaultDictationHotkey))
        #expect(HotkeyCenter.Slot.allCases.count == 2)
    }
}

/// Golden test (spec voice-dictation): synthetic speech from macOS `say`, transcribed by the installed model.
/// Skipped when no model is installed (CI); the audio is generated at test time, never committed.
@Suite("Golden transcription")
struct GoldenTranscriptionTests {
    static let modelPath: String? = {
        let dir = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/dev.bob.assistant/models")
        for name in ["ggml-base.bin", "ggml-large-v3-turbo-q5_0.bin"] {
            let p = dir.appendingPathComponent(name).path
            if FileManager.default.fileExists(atPath: p) { return p }
        }
        return nil
    }()

    @Test(.enabled(if: modelPath != nil, "no dictation model installed"))
    func saySpeechIsTranscribed() async throws {
        let wav = FileManager.default.temporaryDirectory.appendingPathComponent("bob-golden-\(UUID().uuidString).wav")
        defer { try? FileManager.default.removeItem(at: wav) }
        let say = Process()
        say.executableURL = URL(fileURLWithPath: "/usr/bin/say")
        say.arguments = ["-o", wav.path, "--data-format=LEI16@16000", "Thanks for the update, I will call you tomorrow."]
        try say.run()
        say.waitUntilExit()
        let samples = try DictationAudio.load(url: wav)
        #expect(SpeechGate.hasSpeech(samples))
        let t = Transcriber()
        let loaded: Transcriber.Failure? = await withCheckedContinuation { c in t.setModel(path: Self.modelPath) { c.resume(returning: $0) } }
        #expect(loaded == nil)
        let result = await withCheckedContinuation { c in t.transcribe(samples, language: "en") { c.resume(returning: $0) } }
        let text = try result.get().text.lowercased()
        #expect(text.contains("update"))
        #expect(text.contains("tomorrow"))
    }
}

extension Transcriber.Failure: Equatable {
    public static func == (a: Self, b: Self) -> Bool { a.description == b.description }
}
