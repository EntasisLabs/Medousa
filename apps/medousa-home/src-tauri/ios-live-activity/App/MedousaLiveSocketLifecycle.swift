import Foundation

/// Transport state is independent of phone scenes and transcript/UI rendering.
/// A socket closing is not evidence that OpenAI finalized the voice session.
struct MedousaLiveSocketLifecycle {
    enum Phase: Equatable { case idle, connecting, ready, closing, closed, failed }
    private(set) var phase: Phase = .idle
    private(set) var sessionId: String?
    private(set) var finalUsage: [String: Any]?
    private(set) var muted = false

    var acceptsMicrophone: Bool { phase == .ready && !muted }

    mutating func begin() -> Bool {
        guard phase == .idle else { return false }
        phase = .connecting
        return true
    }

    mutating func receive(_ event: [String: Any]) -> Bool {
        guard let type = event["type"] as? String else { return false }
        switch type {
        case "session.started":
            guard phase == .connecting,
                  let session = event["session"] as? [String: Any],
                  let id = session["id"] as? String, !id.isEmpty else { return false }
            sessionId = id
            phase = .ready
        case "session.closed":
            guard phase == .ready || phase == .closing else { return false }
            finalUsage = event["usage"] as? [String: Any]
            phase = .closed
        default:
            guard phase == .ready || phase == .closing else { return false }
        }
        return true
    }

    mutating func setMuted(_ value: Bool) {
        guard phase == .ready else { return }
        muted = value
    }

    /// Returns whether a graceful session.close can be sent.
    mutating func close() -> Bool {
        guard phase == .ready else {
            if phase == .connecting || phase == .idle { phase = .failed }
            return false
        }
        phase = .closing
        return true
    }

    mutating func fail() {
        guard phase != .closed else { return }
        phase = .failed
    }

    static func validPCM(_ bytes: Data) -> Bool {
        // One second maximum per append; PCM16 samples must be complete.
        !bytes.isEmpty && bytes.count <= 48_000 && bytes.count.isMultiple(of: 2)
    }
}
