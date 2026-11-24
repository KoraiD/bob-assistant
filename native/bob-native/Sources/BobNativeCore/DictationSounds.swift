import AppKit

/// Soft start and stop sounds for dictation (proposal 12). Synthesized here (two short sine notes with a
/// smooth envelope), so no sound files ship and nothing is borrowed from macOS. Quiet by design.
public enum DictationSounds {
    public enum Cue { case start, stop, error }
    nonisolated(unsafe) public static var enabled = true
    /// Recorded audio at the start that may contain the start cue; trimmed before transcription.
    public static let startCueMs = 140

    nonisolated(unsafe) private static var cache: [Cue: NSSound] = [:]

    public static func play(_ cue: Cue) {
        guard enabled else { return }
        let sound = cache[cue] ?? NSSound(data: wav(for: cue))
        cache[cue] = sound
        sound?.stop()
        sound?.volume = 0.35
        sound?.play()
    }

    /// Notes (Hz) per cue: rising to start, falling to stop, low for an error.
    public static func notes(_ cue: Cue) -> [Double] {
        switch cue {
        case .start: return [660, 990]
        case .stop: return [990, 660]
        case .error: return [330, 262]
        }
    }

    /// 16-bit mono PCM WAV, 22.05 kHz: each note 55 ms with a raised-cosine envelope (no clicks).
    public static func wav(for cue: Cue) -> Data {
        let rate = 22_050.0, noteSeconds = 0.055
        var samples: [Int16] = []
        for f in notes(cue) {
            let n = Int(rate * noteSeconds)
            for i in 0..<n {
                let t = Double(i) / rate
                let env = 0.5 - 0.5 * cos(2 * Double.pi * Double(i) / Double(n - 1))
                let v = sin(2 * Double.pi * f * t) * env * 0.6
                samples.append(Int16(max(-1, min(1, v)) * Double(Int16.max)))
            }
        }
        var d = Data()
        func u32(_ v: UInt32) { withUnsafeBytes(of: v.littleEndian) { d.append(contentsOf: $0) } }
        func u16(_ v: UInt16) { withUnsafeBytes(of: v.littleEndian) { d.append(contentsOf: $0) } }
        let bytes = UInt32(samples.count * 2)
        d.append(contentsOf: Array("RIFF".utf8)); u32(36 + bytes); d.append(contentsOf: Array("WAVE".utf8))
        d.append(contentsOf: Array("fmt ".utf8)); u32(16); u16(1); u16(1); u32(UInt32(rate)); u32(UInt32(rate) * 2)
        u16(2); u16(16)
        d.append(contentsOf: Array("data".utf8)); u32(bytes)
        for s in samples { withUnsafeBytes(of: s.littleEndian) { d.append(contentsOf: $0) } }
        return d
    }
}
