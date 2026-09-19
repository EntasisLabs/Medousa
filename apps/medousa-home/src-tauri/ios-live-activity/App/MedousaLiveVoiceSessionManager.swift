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
    let liveSessionId: String?
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
    private var nativeLiveSessionId: String?
    private var lastError: String?
    private var carPlayConnected = false
    private var nativeAudio: MedousaLiveNativeAudioEngine?
    private var nativeTransport: MedousaLiveSocketTransport?
    private var pendingNativeBootstrap: (authorization: String, configuration: [String: Any])?
    private var nativeEvents: [String] = []
    private var nativeEventBytes = 0

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
        nativeLiveSessionId = nil
        nativeEvents.removeAll()
        nativeEventBytes = 0
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

    /// Consumes a Rust-created bootstrap entirely inside the native process.
    /// The authorization material is retained only until microphone permission
    /// and AVAudioSession activation complete; it is never persisted or rendered.
    func startNative(json: String) -> String {
        guard let data = json.data(using: .utf8),
              let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let workshopName = payload["workshopName"] as? String,
              let sessionId = payload["sessionId"] as? String,
              let authorization = payload["authorization"] as? String,
              let configuration = payload["configuration"] as? [String: Any],
              !authorization.isEmpty
        else { return encodedStatus(error: "Invalid native Live bootstrap") }

        pendingNativeBootstrap = (authorization, configuration)
        guard let request = try? JSONSerialization.data(withJSONObject: [
            "workshopName": workshopName,
            "sessionId": sessionId,
        ]), let requestJson = String(data: request, encoding: .utf8) else {
            pendingNativeBootstrap = nil
            return encodedStatus(error: "Invalid native Live owner")
        }
        let status = start(json: requestJson)
        if active { beginPendingNativeTransport() }
        return status
    }

    func setMuted(_ nextMuted: Bool) -> String {
        guard active else {
            return encodedStatus(error: "No Medousa Live session is active")
        }
        muted = nextMuted
        nativeAudio?.setMuted(nextMuted)
        nativeTransport?.setMuted(nextMuted)
        phase = nextMuted ? "muted" : "listening"
        publishLiveActivity()
        return encodedStatus()
    }

    func stop() -> String {
        nativeAudio?.stop()
        nativeAudio = nil
        nativeTransport?.close()
        nativeTransport = nil
        pendingNativeBootstrap = nil
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
        nativeLiveSessionId = nil
        nativeEvents.removeAll()
        nativeEventBytes = 0
        endLiveActivity()
        return encodedStatus()
    }

    func drainNativeEvents() -> String {
        let events = nativeEvents
        nativeEvents.removeAll()
        nativeEventBytes = 0
        return "[\(events.joined(separator: ","))]"
    }

    func sendNativeEvent(json: String) -> Bool {
        guard let data = json.data(using: .utf8), data.count <= 65 * 1024,
              let event = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return false }
        return nativeTransport?.sendClientEvent(event) == true
    }

    func status() -> String {
        encodedStatus()
    }

    func setCarPlayConnected(_ connected: Bool) {
        carPlayConnected = connected
        guard active else { return }
        do { try configureAudioCategory() }
        catch { lastError = "Could not change voice audio route: \(error.localizedDescription)" }
    }

    /// Starts the opt-in native transport after a trusted bootstrap supplies a
    /// short-lived credential and the daemon-built Live configuration. This is
    /// intentionally not exposed to CarPlay or JavaScript yet.
    private func startNativeTransport(authorization: String, configuration: [String: Any]) {
        guard active, nativeTransport == nil else { return }
        guard let audio = MedousaLiveNativeAudioEngine() else {
            fail("Native Live audio is unavailable")
            return
        }
        nativeAudio = audio
        let transport = MedousaLiveSocketTransport { [weak self] output in
            guard let self else { return }
            switch output {
            case let .ready(id):
                self.nativeLiveSessionId = id
                do {
                    try audio.start { [weak self] bytes in self?.nativeTransport?.appendAudio(bytes) }
                    audio.setMuted(self.muted)
                    self.phase = self.muted ? "muted" : "listening"
                    self.publishLiveActivity()
                } catch {
                    self.fail("Could not start native Live audio: \(error.localizedDescription)")
                }
            case let .audio(bytes):
                do { try audio.play(bytes) }
                catch { self.fail("Could not play native Live audio: \(error.localizedDescription)") }
            case .closed:
                self.queueNativeEvent(["type": "session.closed"])
                audio.stop()
                self.nativeAudio = nil
                self.nativeTransport = nil
                self.nativeLiveSessionId = nil
                self.active = false
                self.phase = "idle"
            case let .failed(message):
                audio.stop()
                self.nativeAudio = nil
                self.nativeTransport = nil
                self.fail(message)
            case let .event(event):
                self.queueNativeEvent(event)
            }
        }
        nativeTransport = transport
        phase = "connecting"
        publishLiveActivity()
        transport.start(authorization: authorization, configuration: configuration)
    }

    private func configureAudioCategory() throws {
        try AVAudioSession.sharedInstance().setCategory(
            .playAndRecord,
            mode: carPlayConnected ? .default : .voiceChat,
            options: carPlayConnected ? [.allowBluetoothHFP] : [.allowBluetoothHFP, .defaultToSpeaker]
        )
    }

    private func activateAudioSession() {
        do {
            let audio = AVAudioSession.sharedInstance()
            try configureAudioCategory()
            try audio.setActive(true)
            active = true
            muted = false
            phase = "listening"
            lastError = nil
            publishLiveActivity()
            beginPendingNativeTransport()
        } catch {
            fail("Could not start voice audio: \(error.localizedDescription)")
        }
    }

    private func fail(_ message: String) {
        nativeAudio?.stop()
        nativeAudio = nil
        nativeTransport = nil
        nativeLiveSessionId = nil
        pendingNativeBootstrap = nil
        active = false
        muted = false
        phase = "failed"
        lastError = message
        publishLiveActivity()
    }

    private func beginPendingNativeTransport() {
        guard active, nativeTransport == nil, let bootstrap = pendingNativeBootstrap else { return }
        pendingNativeBootstrap = nil
        startNativeTransport(
            authorization: bootstrap.authorization,
            configuration: bootstrap.configuration
        )
    }

    private func queueNativeEvent(_ event: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: event),
              data.count <= 64 * 1024,
              let json = String(data: data, encoding: .utf8) else { return }
        while nativeEvents.count >= 128 || nativeEventBytes + data.count > 256 * 1024 {
            guard !nativeEvents.isEmpty else { return }
            nativeEventBytes -= nativeEvents.removeFirst().utf8.count
        }
        nativeEvents.append(json)
        nativeEventBytes += data.count
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
            liveSessionId: nativeLiveSessionId,
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
