import Foundation
import whisper

/// Decides whether a recording contains speech (VD-5). Pure, unit-tested. Energy-based: the recording is
/// cut into 30 ms frames, and it counts as speech when enough frames are clearly louder than the quietest
/// part of the recording (the noise floor) and than an absolute minimum.
public enum SpeechGate {
    public static let sampleRate = 16_000
    static let frame = 480  // 30 ms at 16 kHz
    /// Absolute RMS floor (about -50 dBFS): anything quieter is silence whatever the noise floor is.
    static let minRMS: Float = 0.003
    /// A speech frame is this many times louder than the noise floor.
    static let ratio: Float = 3
    /// At least this much speech in total.
    static let minSpeechFrames = 8  // 240 ms

    public static func frameRMS(_ samples: [Float]) -> [Float] {
        stride(from: 0, to: samples.count, by: frame).map { start in
            let end = min(start + frame, samples.count)
            var sum: Float = 0
            for i in start..<end { sum += samples[i] * samples[i] }
            return (sum / Float(max(end - start, 1))).squareRoot()
        }
    }

    public static func hasSpeech(_ samples: [Float]) -> Bool {
        let rms = frameRMS(samples)
        guard rms.count >= minSpeechFrames else { return false }
        // Noise floor: the 10th percentile frame.
        let floor = rms.sorted()[rms.count / 10]
        let threshold = max(minRMS, floor * ratio)
        return rms.filter { $0 > threshold }.count >= minSpeechFrames
    }

    /// Peak level 0...1 for the orb's meter.
    public static func level(_ samples: ArraySlice<Float>) -> Float {
        guard !samples.isEmpty else { return 0 }
        var sum: Float = 0
        for s in samples { sum += s * s }
        let rms = (sum / Float(samples.count)).squareRoot()
        // Map about -50...-10 dBFS to 0...1.
        let db = 20 * log10(max(rms, 1e-6))
        return min(max((db + 50) / 40, 0), 1)
    }
}

/// On-device speech to text with whisper.cpp (Metal). One model stays loaded; calls are serialized on a
/// private queue. Audio stays in this process and is never written to disk (VD-8).
public final class Transcriber: @unchecked Sendable {
    public struct Result: Sendable {
        public var text: String
        public var language: String
        public var noSpeechProb: Float
        public var ms: Int
    }

    public enum Failure: Error, CustomStringConvertible {
        case noModel, loadFailed(String), inferenceFailed
        public var description: String {
            switch self {
            case .noModel: return "no dictation model is installed"
            case let .loadFailed(p): return "cannot load the dictation model \((p as NSString).lastPathComponent)"
            case .inferenceFailed: return "transcription failed"
            }
        }
    }

    private let queue = DispatchQueue(label: "dev.bob.whisper", qos: .userInitiated)
    private var ctx: OpaquePointer?
    private var loadedPath: String?

    public init() {}

    deinit {
        if let ctx { whisper_free(ctx) }
    }

    /// Load (or switch to) the model at `path` in the background; nil unloads it.
    public func setModel(path: String?, completion: (@Sendable (Failure?) -> Void)? = nil) {
        queue.async { [self] in
            guard path != loadedPath else { completion?(nil); return }
            if let ctx { whisper_free(ctx) }
            ctx = nil
            loadedPath = nil
            guard let path else { completion?(nil); return }
            var params = whisper_context_default_params()
            params.use_gpu = true
            params.flash_attn = true
            guard let c = whisper_init_from_file_with_params(path, params) else {
                completion?(.loadFailed(path))
                return
            }
            ctx = c
            loadedPath = path
            completion?(nil)
        }
    }

    public var isLoaded: Bool { queue.sync { ctx != nil } }

    /// Free the model now (waits for a running transcription). Must run before the process exits:
    /// ggml's Metal backend asserts in a static destructor if model buffers are still alive at `exit`.
    public func shutdown() {
        queue.sync {
            if let ctx { whisper_free(ctx) }
            ctx = nil
            loadedPath = nil
        }
    }

    /// Transcribe 16 kHz mono samples. `language` is a whisper code ("en", "hu") or nil for auto-detect.
    /// `prompt` biases the vocabulary (names, terms). `audioCtx` shortens the encoder window for short
    /// clips (0 = the full 30 s window).
    public func transcribe(_ samples: [Float], language: String?, prompt: String? = nil, audioCtx: Int32 = 0,
                           completion: @escaping @Sendable (Swift.Result<Result, Failure>) -> Void) {
        queue.async { [self] in
            guard let ctx else { completion(.failure(.noModel)); return }
            let started = Date()
            var p = whisper_full_default_params(WHISPER_SAMPLING_GREEDY)
            p.n_threads = Int32(min(max(ProcessInfo.processInfo.activeProcessorCount - 2, 2), 8))
            p.print_progress = false
            p.print_realtime = false
            p.print_special = false
            p.print_timestamps = false
            p.no_timestamps = true
            p.no_context = true
            p.translate = false
            p.suppress_blank = true
            p.suppress_nst = true
            p.audio_ctx = audioCtx
            let lang = language ?? "auto"
            let status: Int32 = lang.withCString { langPtr in
                (prompt ?? "").withCString { promptPtr in
                    p.language = langPtr
                    p.detect_language = false
                    p.initial_prompt = (prompt?.isEmpty ?? true) ? nil : promptPtr
                    return samples.withUnsafeBufferPointer { buf in
                        whisper_full(ctx, p, buf.baseAddress, Int32(buf.count))
                    }
                }
            }
            guard status == 0 else { completion(.failure(.inferenceFailed)); return }
            var text = ""
            var noSpeech: Float = 0
            let n = whisper_full_n_segments(ctx)
            for i in 0..<n {
                if let t = whisper_full_get_segment_text(ctx, i) { text += String(cString: t) }
                noSpeech = max(noSpeech, whisper_full_get_segment_no_speech_prob(ctx, i))
            }
            let langID = whisper_full_lang_id(ctx)
            let detected = langID >= 0 ? String(cString: whisper_lang_str(langID)) : lang
            completion(.success(Result(text: TranscriptCleaner.clean(text), language: detected,
                                       noSpeechProb: noSpeech, ms: Int(Date().timeIntervalSince(started) * 1000))))
        }
    }
}

/// Removes whisper's non-speech annotations ("[BLANK_AUDIO]", "(music)", "♪") and normalizes spaces.
public enum TranscriptCleaner {
    public static func clean(_ raw: String) -> String {
        var s = raw
        for pattern in [#"\[[^\]]*\]"#, #"\((?:music|applause|laughter|silence|inaudible|noise)[^)]*\)"#, "♪+"] {
            s = s.replacingOccurrences(of: pattern, with: " ", options: [.regularExpression, .caseInsensitive])
        }
        return s.split(whereSeparator: { $0 == " " || $0 == "\n" || $0 == "\t" }).joined(separator: " ")
    }
}
