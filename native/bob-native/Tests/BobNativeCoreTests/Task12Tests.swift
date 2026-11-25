import Foundation
import Testing
@testable import BobNativeCore

@Suite("Clipboard context (proposal 16)", .serialized)
struct ClipboardWatcherTests {
    @Test func privateItemsQuietAppsAndEmptyTextAreNeverReported() {
        let plain = ["public.utf8-plain-text"]
        #expect(ClipboardWatcher.reportable(types: plain, text: "Meeting at 3", frontmost: "com.apple.mail", exclude: []) == "Meeting at 3")
        #expect(ClipboardWatcher.reportable(types: plain + ["org.nspasteboard.ConcealedType"], text: "hunter2",
                                            frontmost: nil, exclude: []) == nil)
        #expect(ClipboardWatcher.reportable(types: plain + ["org.nspasteboard.TransientType"], text: "x", frontmost: nil, exclude: []) == nil)
        #expect(ClipboardWatcher.reportable(types: plain, text: "secret", frontmost: "com.1password.1password",
                                            exclude: ["com.1password.1password"]) == nil)
        #expect(ClipboardWatcher.reportable(types: plain, text: "  \n", frontmost: nil, exclude: []) == nil)
        let long = String(repeating: "a", count: 5000)
        #expect(ClipboardWatcher.reportable(types: plain, text: long, frontmost: nil, exclude: [])?.count == ClipboardWatcher.maxChars)
    }

    @Test func bobsOwnClipboardUseIsIgnored() {
        ClipboardWatcher.ignoreChanges(for: 60)
        #expect(ClipboardWatcher.reportable(types: ["public.utf8-plain-text"], text: "borrowed", frontmost: nil, exclude: []) == nil)
        #expect(ClipboardWatcher.reportable(types: ["public.utf8-plain-text"], text: "later", frontmost: nil, exclude: [],
                                            now: Date().addingTimeInterval(120)) == "later")
        ClipboardWatcher.ignoreChanges(for: -1)
    }
}

@Suite("Dictation sounds (proposal 12)")
struct DictationSoundsTests {
    @Test func cuesAreShortQuietValidWAVs() {
        for cue in [DictationSounds.Cue.start, .stop, .error] {
            let wav = DictationSounds.wav(for: cue)
            #expect(String(decoding: wav.prefix(4), as: UTF8.self) == "RIFF")
            #expect(String(decoding: wav[8..<12], as: UTF8.self) == "WAVE")
            let seconds = Double(wav.count - 44) / 2 / 22_050
            #expect(seconds > 0.08 && seconds < 0.2, "cue length \(seconds) s")
            // Peak stays well below full scale (soft by design) and the envelope starts silent (no click).
            let samples = stride(from: 44, to: wav.count - 1, by: 2).map { Int16(bitPattern: UInt16(wav[$0]) | UInt16(wav[$0 + 1]) << 8) }
            #expect((samples.map { abs(Int($0)) }.max() ?? 0) < Int(Int16.max) * 7 / 10)
            #expect(abs(Int(samples[0])) < 200)
        }
        #expect(DictationSounds.notes(.start).first! < DictationSounds.notes(.start).last!, "start rises")
        #expect(DictationSounds.notes(.stop).first! > DictationSounds.notes(.stop).last!, "stop falls")
    }
}

@Suite("Typo fixes")
struct TypoFinderTests {
    @Test func onlyFinishedPlainWordsAreChecked() {
        #expect(TypoFinder.lastWord("I saw teh ")?.word == "teh")
        #expect(TypoFinder.lastWord("I saw teh, ")?.trailing == ", ")
        #expect(TypoFinder.lastWord("I saw teh") == nil, "still typing")
        #expect(TypoFinder.lastWord("mail me at bob@exmaple.com ") == nil)
        #expect(TypoFinder.lastWord("see src/lib/foo.rs ") == nil)
        #expect(TypoFinder.lastWord("Hello Dzsenifer ") == nil, "a name mid-sentence")
        #expect(TypoFinder.lastWord("Teh ")?.sentenceStart == true)
        #expect(TypoFinder.lastWord("use getValue ") == nil)
        #expect(TypoFinder.lastWord("an ") == nil, "too short")
    }

    @Test @MainActor func theMacSpellCheckerFindsCommonTypos() {
        // Words the spelling check always flags. ("teh" is left out: macOS's autocorrection adapts to recent
        // use, so whether it offers "the" depends on the Mac's history.)
        #expect(TypoFinder.find(in: "We will recieve ")?.fix == "receive")
        #expect(TypoFinder.find(in: "Recieve ")?.fix == "Receive")
        #expect(TypoFinder.find(in: "It was definately ")?.fix == "definitely")
        #expect(TypoFinder.find(in: "This is fine ") == nil)
    }

    @Test func distanceAndCase() {
        #expect(TypoFinder.distance("teh", "the") == 2)
        #expect(TypoFinder.distance("recieve", "receive") == 2)
        #expect(TypoFinder.matchCase("the", like: "Teh") == "The")
    }
}

@Suite("Keyboard layout")
struct KeyLayoutTests {
    @Test func lettersMapToKeysInTheCurrentLayout() {
        // C and V are in the same place on QWERTY, QWERTZ and AZERTY; the lookup must find a key for each.
        for letter in ["c", "v", "z", "a"] as [Character] {
            let code = KeyLayout.code(for: letter, fallback: 999)
            #expect(code != 999, "no key types \(letter)")
        }
    }
}
