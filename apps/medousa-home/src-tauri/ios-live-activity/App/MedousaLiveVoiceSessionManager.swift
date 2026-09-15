import AVFAudio
import Foundation

private struct LiveVoiceStartRequest: Decodable {
    let workshopName: String
    let sessionId: String
}

private struct LiveVoiceStatus: Encodable {
    let available: Bool
    let active: Bool
    let muted: Bool
    let phase: String
    let workshopName: String?
    let sessionId: String?
    let error: String?
}

/// Owns the native audio-session lifetime for Medousa Live.
///
/// Audio transport deliberately plugs in behind this owner. Keeping AVAudioSession
/// out of the webview is what lets an explicitly started conversation survive app
/// backgrounding and the locked screen once the transport is attached.
@available(iOS 17.0, *)
@MainActor
final class MedousaLiveVoiceSessionManager {
    static let shared = MedousaLiveVoiceSessionManager()

    private var active = false
    private var muted = false
    private var phase = "idle"
    private var workshopName: String?
    private var sessionId: String?
    private var lastError: String?

    private init() {}

    func start(json: String) -> String {
        guard let data = json.data(using: .utf8),
              let request = try? JSONDecoder().decode(LiveVoiceStartRequest.self, from: data),
              !request.workshopName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !request.sessionId.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        else {
            return encodedStatus(error: "workshopName and sessionId are required")
        }

        workshopName = request.workshopName
        sessionId = request.sessionId
        phase = "connecting"
        lastError = nil

        switch AVAudioApplication.shared.recordPermission {
        case .granted:
            activateAudioSession()
        case .denied:
            fail("Microphone access is disabled in iOS Settings")
        case .undetermined:
            AVAudioApplication.requestRecordPermission { [weak self] granted in
                Task { @MainActor in
                    guard let self else { return }
                    if granted {
                        self.activateAudioSession()
                    } else {
                        self.fail("Microphone access was not granted")
                    }
                }
            }
        @unknown default:
            fail("Microphone permission is unavailable")
        }

        return encodedStatus()
    }

    func setMuted(_ nextMuted: Bool) -> String {
        guard active else {
            return encodedStatus(error: "No Medousa Live session is active")
        }
        muted = nextMuted
        phase = nextMuted ? "muted" : "listening"
        publishLiveActivity()
        return encodedStatus()
    }

    func stop() -> String {
        do {
            try AVAudioSession.sharedInstance().setActive(
                false,
                options: [.notifyOthersOnDeactivation]
            )
        } catch {
            lastError = error.localizedDescription
        }
        active = false
        muted = false
        phase = "idle"
        workshopName = nil
        sessionId = nil
        endLiveActivity()
        return encodedStatus()
    }

    func status() -> String {
        encodedStatus()
    }

    private func activateAudioSession() {
        do {
            let audio = AVAudioSession.sharedInstance()
            try audio.setCategory(
                .playAndRecord,
                mode: .voiceChat,
                options: [.allowBluetoothHFP, .defaultToSpeaker]
            )
            try audio.setActive(true)
            active = true
            muted = false
            phase = "listening"
            lastError = nil
            publishLiveActivity()
        } catch {
            fail("Could not start voice audio: \(error.localizedDescription)")
        }
    }

    private func fail(_ message: String) {
        active = false
        muted = false
        phase = "failed"
        lastError = message
        publishLiveActivity()
    }

    private func publishLiveActivity() {
        guard let workshopName else { return }
        let eyebrow: String
        let headline: String
        switch phase {
        case "listening":
            eyebrow = "Live · Listening"
            headline = "Talk to Medousa"
        case "muted":
            eyebrow = "Live · Muted"
            headline = "Medousa Live is paused"
        case "failed":
            eyebrow = "Live · Needs attention"
            headline = lastError ?? "Voice session stopped"
        default:
            eyebrow = "Live · Connecting"
            headline = "Starting Medousa Live"
        }
        let payload: [String: Any] = [
            "mood": phase == "failed" ? "blocked" : "working",
            "workshopName": workshopName,
            "eyebrow": eyebrow,
            "headline": headline,
            "subline": "Tap Medousa to return to the conversation",
            "motionSummary": phase,
            "blockedCount": phase == "failed" ? 1 : 0,
            "primaryCardId": NSNull(),
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else { return }
        _ = MedousaLiveActivityManager.shared.sync(json: json)
    }

    private func endLiveActivity() {
        let payload: [String: Any] = [
            "mood": "quiet",
            "workshopName": "Medousa",
            "eyebrow": "Live",
            "headline": "Voice session ended",
            "blockedCount": 0,
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else { return }
        _ = MedousaLiveActivityManager.shared.sync(json: json)
    }

    private func encodedStatus(error overrideError: String? = nil) -> String {
        let status = LiveVoiceStatus(
            available: true,
            active: active,
            muted: muted,
            phase: phase,
            workshopName: workshopName,
            sessionId: sessionId,
            error: overrideError ?? lastError
        )
        guard let data = try? JSONEncoder().encode(status),
              let json = String(data: data, encoding: .utf8)
        else {
            return "{\"available\":false,\"active\":false,\"muted\":false,\"phase\":\"failed\",\"error\":\"encode failed\"}"
        }
        return json
    }
}
