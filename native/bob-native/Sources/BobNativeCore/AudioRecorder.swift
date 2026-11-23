import AVFoundation
import Foundation

/// 16 kHz mono Float32, the format whisper expects.
public enum DictationAudio {
    public static let format = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1,
                                             interleaved: false)!
    /// VD-11: one recording is at most 5 minutes.
    public static let maxSeconds = 300

    /// Convert one buffer to 16 kHz mono with `converter`, appending to `out`.
    static func convert(_ buffer: AVAudioPCMBuffer, with converter: AVAudioConverter, into out: inout [Float]) {
        let ratio = format.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount(Double(buffer.frameLength) * ratio + 32)
        guard let converted = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: capacity) else { return }
        var fed = false
        var error: NSError?
        converter.convert(to: converted, error: &error) { _, status in
            if fed { status.pointee = .noDataNow; return nil }
            fed = true
            status.pointee = .haveData
            return buffer
        }
        guard error == nil, let data = converted.floatChannelData else { return }
        out.append(contentsOf: UnsafeBufferPointer(start: data[0], count: Int(converted.frameLength)))
    }

    /// Read an audio file into 16 kHz mono samples (measurement tool and tests only).
    public static func load(url: URL) throws -> [Float] {
        let file = try AVAudioFile(forReading: url)
        guard let converter = AVAudioConverter(from: file.processingFormat, to: format),
              let buffer = AVAudioPCMBuffer(pcmFormat: file.processingFormat,
                                            frameCapacity: AVAudioFrameCount(file.length)) else { return [] }
        try file.read(into: buffer)
        var out: [Float] = []
        convert(buffer, with: converter, into: &out)
        return out
    }
}

/// Microphone capture for dictation (VD-1). Samples stay in memory and are handed to the transcriber; they
/// are never written to disk (VD-8). Start and stop on the main thread.
public final class AudioRecorder: @unchecked Sendable {
    private let engine = AVAudioEngine()
    private let lock = NSLock()
    private var samples: [Float] = []
    private var converter: AVAudioConverter?
    public private(set) var isRecording = false
    /// Called (main thread, about 20 times a second) with the input level 0...1 for the orb's meter.
    public var onLevel: ((Float) -> Void)?
    /// Called once when the 5-minute cap is reached (VD-11).
    public var onCap: (() -> Void)?

    public init() {}

    public enum Failure: Error, CustomStringConvertible {
        case permission, noInput, engine(String)
        public var description: String {
            switch self {
            case .permission: return "microphone"
            case .noInput: return "no microphone found"
            case let .engine(m): return "the microphone could not start (\(m))"
            }
        }
    }

    public func start() throws {
        guard !isRecording else { return }
        guard Permissions.microphoneStatus() == .granted else { throw Failure.permission }
        let input = engine.inputNode
        let inFormat = input.outputFormat(forBus: 0)
        guard inFormat.sampleRate > 0, inFormat.channelCount > 0 else { throw Failure.noInput }
        guard let conv = AVAudioConverter(from: inFormat, to: DictationAudio.format) else {
            throw Failure.engine("unsupported input format")
        }
        converter = conv
        lock.withLock { samples.removeAll(keepingCapacity: true) }
        let cap = DictationAudio.maxSeconds * Int(DictationAudio.format.sampleRate)
        var lastLevel = Date.distantPast
        var capped = false
        input.installTap(onBus: 0, bufferSize: 2048, format: inFormat) { [weak self] buffer, _ in
            guard let self, let conv = self.converter else { return }
            var chunk: [Float] = []
            DictationAudio.convert(buffer, with: conv, into: &chunk)
            var reachedCap = false
            self.lock.withLock {
                let room = cap - self.samples.count
                if room > 0 { self.samples.append(contentsOf: chunk.prefix(room)) }
                reachedCap = self.samples.count >= cap
            }
            let now = Date()
            if now.timeIntervalSince(lastLevel) > 0.05 {
                lastLevel = now
                let level = SpeechGate.level(chunk[...])
                DispatchQueue.main.async { self.onLevel?(level) }
            }
            if reachedCap && !capped {
                capped = true
                DispatchQueue.main.async { self.onCap?() }
            }
        }
        engine.prepare()
        do {
            try engine.start()
        } catch {
            input.removeTap(onBus: 0)
            throw Failure.engine(error.localizedDescription)
        }
        isRecording = true
    }

    /// Stop and return the recording (empty when `keep` is false). The buffer is cleared either way.
    public func stop(keep: Bool) -> [Float] {
        guard isRecording else { return [] }
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        isRecording = false
        return lock.withLock {
            let out = keep ? samples : []
            samples = []
            return out
        }
    }
}
