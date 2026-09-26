import Foundation

/// Opt-in native primary connection. Not activated by CarPlay attachment.
/// The workshop supplies configuration/credentials; this layer owns neither
/// agent execution nor a separate key store. Audio capture plugs in separately.
@available(iOS 17.0, *)
@MainActor
final class MedousaLiveSocketTransport {
    enum Output {
        case ready(String)
        case audio(Data)
        case event([String: Any])
        case closed([String: Any]?)
        case failed(String)
    }

    private var lifecycle = MedousaLiveSocketLifecycle()
    private var socket: URLSessionWebSocketTask?
    private var session: URLSession?
    private var receiver: Task<Void, Never>?
    private var sender: Task<Void, Never>?
    private var deadline: Task<Void, Never>?
    private var queue: [String] = []
    private var queuedBytes = 0
    private let output: (Output) -> Void

    init(output: @escaping (Output) -> Void) { self.output = output }

    deinit {
        deadline?.cancel()
        receiver?.cancel()
        sender?.cancel()
        socket?.cancel(with: .goingAway, reason: nil)
        session?.invalidateAndCancel()
    }

    func start(authorization: String, configuration: [String: Any]) {
        guard lifecycle.begin() else { return }
        // Reject alternate endpoints, codecs, managed delegation, or retained
        // sessions: native startup must preserve the existing Medousa contract.
        guard !authorization.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !authorization.contains("\r"), !authorization.contains("\n"),
              configuration["model"] as? String == "gpt-live-1",
              configuration["store"] as? Bool == false,
              (configuration["delegation"] as? [String: Any])?["type"] as? String == "client",
              let audio = configuration["audio"] as? [String: Any],
              let format = audio["format"] as? [String: Any],
              format["type"] as? String == "audio/pcm", format["rate"] as? Int == 24_000
        else { fail("Invalid native Live configuration"); return }

        var request = URLRequest(url: URL(string: "wss://api.openai.com/v1/live/sessions")!)
        request.setValue("Bearer \(authorization)", forHTTPHeaderField: "Authorization")
        let session = URLSession(configuration: .ephemeral)
        self.session = session
        let socket = session.webSocketTask(with: request)
        socket.maximumMessageSize = 256 * 1024
        self.socket = socket
        socket.resume()
        receiver = Task { [weak self] in
            do {
                while !Task.isCancelled {
                    let message = try await socket.receive()
                    guard let self, !Task.isCancelled else { return }
                    self.receive(message)
                }
            } catch {
                guard !Task.isCancelled else { return }
                self?.fail("Native Live connection ended before session finalization")
            }
        }
        enqueue(["type": "session.start", "session": configuration])
        armDeadline(seconds: 15, message: "Native Live did not confirm startup")
    }

    func appendAudio(_ bytes: Data) {
        guard lifecycle.acceptsMicrophone else { return }
        guard MedousaLiveSocketLifecycle.validPCM(bytes) else {
            fail("Invalid native Live microphone audio"); return
        }
        enqueue(["type": "session.input_audio.append", "audio": bytes.base64EncodedString()])
    }

    func setMuted(_ muted: Bool) {
        guard lifecycle.phase == .ready, lifecycle.muted != muted else { return }
        lifecycle.setMuted(muted)
        // The local gate is immediate, independent of remote acknowledgment.
        // Already queued PCM is removed so mute/close never flush old speech.
        discardQueuedAudio()
        enqueue(["type": muted ? "session.input_audio.mute" : "session.input_audio.unmute"])
    }

    func appendContext(_ text: String, spoken: Bool, id: String) {
        guard lifecycle.phase == .ready, !text.isEmpty, !id.isEmpty,
              text.utf8.count <= (spoken ? 400 : 64 * 1024) else { return }
        enqueue(["type": spoken ? "session.commentary.append" : "session.thinking.append",
                 "event_id": "\(spoken ? "result" : "context")-\(id)",
                 "delegation_id": id, "content": text])
    }

    /// Accept only the two sideband presentation events Medousa already uses.
    /// Swift never accepts arbitrary Live commands from the webview.
    func sendClientEvent(_ event: [String: Any]) -> Bool {
        guard lifecycle.phase == .ready,
              let type = event["type"] as? String,
              ["session.commentary.append", "session.thinking.append"].contains(type),
              let eventId = event["event_id"] as? String, !eventId.isEmpty, eventId.count <= 128,
              let delegationId = event["delegation_id"] as? String, !delegationId.isEmpty,
              delegationId.count <= 128,
              let content = event["content"] as? String, !content.isEmpty,
              content.utf8.count <= (type == "session.commentary.append" ? 400 : 64 * 1024)
        else { return false }
        enqueue([
            "type": type,
            "event_id": eventId,
            "delegation_id": delegationId,
            "content": content,
        ])
        return true
    }

    func close() {
        guard lifecycle.phase != .closing && lifecycle.phase != .closed && lifecycle.phase != .failed else { return }
        discardQueuedAudio()
        guard lifecycle.close() else {
            fail("Native Live stopped before startup was confirmed"); return
        }
        enqueue(["type": "session.close"])
        armDeadline(seconds: 5, message: "Native Live final usage was not confirmed")
    }

    private func receive(_ message: URLSessionWebSocketTask.Message) {
        guard case let .string(text) = message,
              let data = text.data(using: .utf8),
              let event = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let type = event["type"] as? String else {
            fail("Native Live returned an invalid event"); return
        }
        if type == "error" { fail("OpenAI rejected a native Live command"); return }
        guard lifecycle.receive(event) else { return }
        switch type {
        case "session.started":
            deadline?.cancel()
            output(.ready(lifecycle.sessionId!))
        case "session.output_audio.delta":
            guard lifecycle.phase == .ready,
                  let encoded = event["delta"] as? String,
                  let bytes = Data(base64Encoded: encoded),
                  !bytes.isEmpty, bytes.count.isMultiple(of: 2) else {
                if lifecycle.phase == .ready { fail("Native Live returned invalid PCM audio") }
                return
            }
            output(.audio(bytes))
        case "session.closed":
            let usage = lifecycle.finalUsage
            cleanup()
            output(.closed(usage))
        default:
            output(.event(event))
        }
    }

    private func enqueue(_ event: [String: Any]) {
        guard lifecycle.phase != .failed && lifecycle.phase != .closed,
              let data = try? JSONSerialization.data(withJSONObject: event),
              let text = String(data: data, encoding: .utf8) else { return }
        // Bound memory and latency under network backpressure. Never silently
        // drop/reorder audio and pretend the user was heard.
        guard queue.count < 64, queuedBytes + data.count <= 256 * 1024 else {
            fail("Native Live connection could not keep up with audio"); return
        }
        queue.append(text)
        queuedBytes += data.count
        guard sender == nil, let socket else { return }
        sender = Task { [weak self] in
            guard let self else { return }
            defer { self.sender = nil }
            while !self.queue.isEmpty && !Task.isCancelled {
                let text = self.queue.removeFirst()
                self.queuedBytes -= text.utf8.count
                do { try await socket.send(.string(text)) }
                catch {
                    if !Task.isCancelled { self.fail("Could not send native Live audio or control") }
                    return
                }
            }
        }
    }

    private func discardQueuedAudio() {
        queue.removeAll { text in
            guard let data = text.data(using: .utf8),
                  let event = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return true }
            return event["type"] as? String == "session.input_audio.append"
        }
        queuedBytes = queue.reduce(0) { $0 + $1.utf8.count }
    }

    private func armDeadline(seconds: UInt64, message: String) {
        deadline?.cancel()
        deadline = Task { [weak self] in
            do { try await Task.sleep(nanoseconds: seconds * 1_000_000_000) }
            catch { return }
            self?.fail(message)
        }
    }

    private func fail(_ message: String) {
        guard lifecycle.phase != .closed else { return }
        let alreadyFailed = lifecycle.phase == .failed && socket == nil
        lifecycle.fail()
        cleanup()
        if !alreadyFailed { output(.failed(message)) }
    }

    private func cleanup() {
        deadline?.cancel(); deadline = nil
        receiver?.cancel(); receiver = nil
        sender?.cancel(); sender = nil
        queue.removeAll(); queuedBytes = 0
        socket?.cancel(with: .normalClosure, reason: nil); socket = nil
        session?.invalidateAndCancel(); session = nil
    }
}
