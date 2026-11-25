import Foundation

/// Reports slow work that could delay typing (helper stderr, which the core logs). Durations only, never text;
/// at most one line per kind every two seconds.
public enum Stall {
    nonisolated(unsafe) private static var last: [String: Date] = [:]
    private static let lock = NSLock()
    /// Shortest gap between two lines of the same kind.
    static let interval: TimeInterval = 2

    public static func report(_ kind: String, ms: Double, threshold: Double) {
        guard shouldReport(kind, ms: ms, threshold: threshold, now: Date()) else { return }
        FileHandle.standardError.write(Data("bob-native: slow \(kind): \(Int(ms)) ms\n".utf8))
    }

    /// Whether a duration is worth a line: over the threshold and not throttled. Records the line when it is.
    static func shouldReport(_ kind: String, ms: Double, threshold: Double, now: Date) -> Bool {
        guard ms >= threshold else { return false }
        lock.lock()
        defer { lock.unlock() }
        if let t = last[kind], now.timeIntervalSince(t) < interval { return false }
        last[kind] = now
        return true
    }

    /// Time `body` and report it when slow.
    @discardableResult
    public static func measure<T>(_ kind: String, threshold: Double = 30, _ body: () -> T) -> T {
        let start = DispatchTime.now()
        let result = body()
        report(kind, ms: Double(DispatchTime.now().uptimeNanoseconds - start.uptimeNanoseconds) / 1e6, threshold: threshold)
        return result
    }
}
