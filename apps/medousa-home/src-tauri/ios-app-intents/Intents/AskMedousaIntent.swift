import AppIntents
import AVFoundation
import Foundation
import Security
import SwiftUI
import UIKit

private struct SiriExecutionContext: Decodable {
    let version: Int
    let workshopId: String
    let baseUrl: String
    let sessionId: String
    let provider: String
    let model: String
    let responseDepthMode: String
    let reasoningEffort: String
    let identityUserId: String?
    let updatedAt: TimeInterval
}

private struct SiriTurnAccepted: Decodable {
    let turnId: String
    let streamUrl: String

    enum CodingKeys: String, CodingKey {
        case turnId = "turn_id"
        case streamUrl = "stream_url"
    }
}

private struct SiriPreferences: Decodable {
    let speechMode: String
    let maxSpokenCharacters: Int
    let defaultWorkshopId: String?
    let defaultSessionId: String?
    let fastResponseModel: String?

    static let defaults = SiriPreferences(
        speechMode: "auto",
        maxSpokenCharacters: 320,
        defaultWorkshopId: nil,
        defaultSessionId: nil,
        fastResponseModel: nil
    )

    static func load(from defaults: UserDefaults) -> SiriPreferences {
        guard let encoded = defaults.string(forKey: "siri.preferences.v1"),
              let data = encoded.data(using: .utf8),
              let value = try? JSONDecoder().decode(SiriPreferences.self, from: data)
        else { return .defaults }
        return value
    }
}

private enum SiriBackgroundError: Error {
    case unavailable
    case authentication
    case configuration
    case busy
    case timedOut
}

private enum SiriBackgroundOutcome {
    case answer(String)
    case continuing
    case needsInput
}

@MainActor
private final class MedousaSiriSpeechPlayer {
    static let shared = MedousaSiriSpeechPlayer()
    private let synthesizer = AVSpeechSynthesizer()

    func start(_ text: String) {
        let session = AVAudioSession.sharedInstance()
        try? session.setCategory(.playback, mode: .spokenAudio, options: [.duckOthers])
        try? session.setActive(true)
        let utterance = AVSpeechUtterance(string: text)
        utterance.rate = AVSpeechUtteranceDefaultSpeechRate
        synthesizer.speak(utterance)
    }

    var isSpeaking: Bool { synthesizer.isSpeaking }

    func finish() {
        try? AVAudioSession.sharedInstance().setActive(
            false,
            options: [.notifyOthersOnDeactivation]
        )
    }
}

private struct SiriPersonalTurnResponse: Decodable {
    let status: String
    let text: String?
    let error: String?
}

@_silgen_name("medousa_siri_execute_personal")
private func medousaSiriExecutePersonal(
    _ json: UnsafePointer<CChar>?
) -> UnsafeMutablePointer<CChar>?

@_silgen_name("medousa_siri_free_rust_string")
private func medousaSiriFreeRustString(_ value: UnsafeMutablePointer<CChar>?)

@available(iOS 18.0, *)
private struct MedousaSiriResultView: View {
    let answer: String
    let isContinuing: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Image(systemName: isContinuing ? "ellipsis.bubble.fill" : "waveform.circle.fill")
                    .foregroundStyle(.purple)
                Text(isContinuing ? "Working in Medousa" : "Medousa")
                    .font(.headline)
            }
            Text(answer)
                .font(.body)
                .lineLimit(8)
        }
        .padding()
    }
}

/// Runs the selected workshop directly in the background. A failed background
/// admission is reported to Siri instead of unexpectedly launching the app.
@available(iOS 18.0, *)
struct AskMedousaIntent: AppIntent {
    private static let daemonReadyWaitSeconds: TimeInterval = 8
    private static let resultWaitSeconds: TimeInterval = 20
    private static let longRunningResultWaitSeconds: TimeInterval = 120
    static let title: LocalizedStringResource = "Ask Medousa"
    static let description = IntentDescription(
        "Ask the currently selected Medousa workshop without opening the app."
    )
    static let openAppWhenRun = false

    @available(iOS 26.0, *)
    static var supportedModes: IntentModes {
        [.background]
    }

    @Parameter(
        title: "Request",
        requestValueDialog: IntentDialog("What would you like to ask Medousa?")
    )
    var prompt: String

    @Parameter(title: "Workshop")
    var workshop: WorkshopEntity?

    static var parameterSummary: some ParameterSummary {
        Summary("Ask Medousa \(\.$prompt) in \(\.$workshop)")
    }

    func perform() async throws -> some IntentResult & ProvidesDialog & ShowsSnippetView {
        guard let defaults = UserDefaults(suiteName: "group.com.entasislabs.medousa-home") else {
            throw SiriBackgroundError.unavailable
        }

        do {
            let preferences = SiriPreferences.load(from: defaults)
            let workshopId = workshop?.id
                ?? preferences.defaultWorkshopId
                ?? WorkshopEntitySnapshot.load().first(where: \.isActive)?.id
            let outcome: SiriBackgroundOutcome
            if #available(iOS 27.0, *) {
                outcome = try await performLongRunningTurn(
                    workshopId: workshopId,
                    prompt: prompt,
                    defaults: defaults,
                    preferences: preferences
                )
            } else {
                outcome = try await runBackgroundTurn(
                    workshopId: workshopId,
                    prompt: prompt,
                    defaults: defaults,
                    preferences: preferences,
                    resultWaitSeconds: Self.resultWaitSeconds
                )
            }
            switch outcome {
            case .answer(let answer):
                if await shouldSpeak(preferences.speechMode) {
                    await speakAnswer(
                        String(answer.prefix(max(80, preferences.maxSpokenCharacters)))
                    )
                }
                return .result(
                    dialog: IntentDialog(full: "\(answer)", supporting: "\(answer)"),
                    view: MedousaSiriResultView(answer: answer, isContinuing: false)
                )
            case .continuing:
                let continuing = "Your request is continuing in Medousa."
                return .result(
                    dialog: IntentDialog(full: "\(continuing)", supporting: "Open Medousa to check its progress."),
                    view: MedousaSiriResultView(answer: continuing, isContinuing: true)
                )
            case .needsInput:
                let handoff = "Medousa needs you to open the app to finish that request."
                return .result(
                    dialog: IntentDialog(full: "\(handoff)", supporting: "Your work is saved in the selected chat."),
                    view: MedousaSiriResultView(answer: handoff, isContinuing: false)
                )
            }
        } catch {
            let unavailable: String
            let recovery: String
            switch error {
            case SiriBackgroundError.authentication:
                unavailable = "Medousa needs you to reconnect this workshop."
                recovery = "Open Connection settings in Medousa."
            case SiriBackgroundError.configuration:
                unavailable = "This Siri request doesn't match the selected workshop settings."
                recovery = "Open Medousa and update the Siri defaults."
            case SiriBackgroundError.busy:
                unavailable = "Medousa is still working in that chat."
                recovery = "I'll notify you when the current request is ready."
            default:
                unavailable = "I couldn't reach your selected Medousa workshop."
                recovery = "Open Medousa once, then try again."
            }
            return .result(
                dialog: IntentDialog(full: "\(unavailable)", supporting: "\(recovery)"),
                view: MedousaSiriResultView(answer: unavailable, isContinuing: false)
            )
        }
    }

    private func speakAnswer(_ answer: String) async {
        let backgroundTask = await MainActor.run {
            UIApplication.shared.beginBackgroundTask(
                withName: "Medousa Siri speech",
                expirationHandler: nil
            )
        }
        await MedousaSiriSpeechPlayer.shared.start(answer)
        let deadline = Date().addingTimeInterval(20)
        while !Task.isCancelled && Date() < deadline {
            let speaking = await MedousaSiriSpeechPlayer.shared.isSpeaking
            if !speaking { break }
            try? await Task.sleep(for: .milliseconds(100))
        }
        await MedousaSiriSpeechPlayer.shared.finish()
        await MainActor.run {
            if backgroundTask != .invalid {
                UIApplication.shared.endBackgroundTask(backgroundTask)
            }
        }
    }

    private func shouldSpeak(_ mode: String) async -> Bool {
        if mode == "always" { return true }
        if mode == "never" { return false }
        return await MainActor.run {
            AVAudioSession.sharedInstance().currentRoute.outputs.contains {
                $0.portType == .builtInSpeaker || $0.portType == .builtInReceiver
            }
        }
    }

    private func runBackgroundTurn(
        workshopId: String?,
        prompt: String,
        defaults: UserDefaults,
        preferences: SiriPreferences,
        resultWaitSeconds: TimeInterval
    ) async throws -> SiriBackgroundOutcome {
        let pinnedContext = preferences.defaultSessionId.flatMap { sessionId in
            (defaults.dictionary(forKey: "siri.executionContexts.v1") as? [String: String])?[sessionId]
        }
        guard let encoded = pinnedContext ?? defaults.string(forKey: "siri.executionContext.v1"),
              let data = encoded.data(using: .utf8),
              let context = try? JSONDecoder().decode(SiriExecutionContext.self, from: data),
              context.version == 1,
              context.updatedAt <= Date().timeIntervalSince1970 + 60,
              Date().timeIntervalSince1970 - context.updatedAt < 86_400,
              workshopId == nil || workshopId == context.workshopId,
              let baseURL = URL(string: context.baseUrl),
              let turnURL = URL(string: "v1/turns", relativeTo: baseURL.appendingPathComponent(""))
        else {
            throw SiriBackgroundError.unavailable
        }

        if context.workshopId == "personal" {
            return try await runPersonalTurn(
                context: context,
                prompt: prompt,
                resultWaitSeconds: resultWaitSeconds
            )
        }

        let payload: [String: Any] = [
            "session_id": context.sessionId,
            "prompt": prompt,
            "mode": "interactive",
            "persist_user_turn": true,
            "response_depth_mode": context.responseDepthMode,
            "reasoning_effort": context.reasoningEffort,
            "provider": context.provider,
            "model": preferences.fastResponseModel ?? context.model,
            "surface": [
                "channel_surface": "home-ios-siri",
                "channel_id": context.sessionId,
                "supports_ui_artifacts": false,
                "supports_liquid_markdown": false,
                "supports_browser_host": false,
                "selected_worlds": [],
            ],
            "media_refs": [],
            "identity_user_id": context.identityUserId ?? NSNull(),
        ]
        try await waitForDaemon(baseURL: baseURL, bearer: loadSiriBearer())
        var request = URLRequest(url: turnURL)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        if let bearer = loadSiriBearer(), !bearer.isEmpty {
            request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
        }
        request.httpBody = try JSONSerialization.data(withJSONObject: payload)

        let (responseData, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse,
              (200..<300).contains(http.statusCode)
        else {
            if let http = response as? HTTPURLResponse,
               http.statusCode == 401 || http.statusCode == 403 {
                throw SiriBackgroundError.authentication
            }
            throw SiriBackgroundError.unavailable
        }
        guard
              let accepted = try? JSONDecoder().decode(SiriTurnAccepted.self, from: responseData),
              let streamURL = URL(string: accepted.streamUrl, relativeTo: baseURL)
        else {
            throw SiriBackgroundError.unavailable
        }
        do {
            let answer = try await withThrowingTaskGroup(of: String.self) { group in
                group.addTask {
                    try await readTurnResult(streamURL: streamURL, bearer: loadSiriBearer())
                }
                group.addTask {
                    try await Task.sleep(for: .seconds(resultWaitSeconds))
                    throw SiriBackgroundError.timedOut
                }
                guard let result = try await group.next() else {
                    throw SiriBackgroundError.unavailable
                }
                group.cancelAll()
                return result
            }
            return .answer(answer)
        } catch {
            return .continuing
        }
    }

    private func runPersonalTurn(
        context: SiriExecutionContext,
        prompt: String,
        resultWaitSeconds: TimeInterval
    ) async throws -> SiriBackgroundOutcome {
        let backgroundTask = await MainActor.run {
            if #available(iOS 27.0, *) {
                // LongRunningIntent owns the background assertion on iOS 27.
                // Starting a second UIKit assertion here can expire underneath
                // the system-managed task while a tool call is still running.
                return UIBackgroundTaskIdentifier.invalid
            }
            return UIApplication.shared.beginBackgroundTask(
                withName: "Medousa Siri turn",
                expirationHandler: nil
            )
        }
        var keepCompletionTailAlive = false
        defer {
            if !keepCompletionTailAlive {
                Task { @MainActor in
                    if backgroundTask != .invalid {
                        UIApplication.shared.endBackgroundTask(backgroundTask)
                    }
                }
            }
        }
        let payload: [String: Any] = [
            "prompt": prompt,
            "sessionId": context.sessionId,
            "provider": context.provider,
            "model": context.model,
            "responseDepthMode": context.responseDepthMode,
            "reasoningEffort": context.reasoningEffort,
            "identityUserId": context.identityUserId ?? NSNull(),
            "resultWaitSeconds": resultWaitSeconds,
        ]
        let data = try JSONSerialization.data(withJSONObject: payload)
        guard let encoded = String(data: data, encoding: .utf8) else {
            throw SiriBackgroundError.unavailable
        }
        let response = await Task.detached(priority: .userInitiated) {
            encoded.withCString { pointer -> SiriPersonalTurnResponse? in
                guard let raw = medousaSiriExecutePersonal(pointer) else { return nil }
                defer { medousaSiriFreeRustString(raw) }
                let result = Data(String(cString: raw).utf8)
                return try? JSONDecoder().decode(SiriPersonalTurnResponse.self, from: result)
            }
        }.value
        guard let response else { throw SiriBackgroundError.unavailable }
        switch response.status {
        case "answer":
            guard let text = response.text?.trimmingCharacters(in: .whitespacesAndNewlines),
                  !text.isEmpty
            else {
                throw SiriBackgroundError.unavailable
            }
            return .answer(String(text.prefix(600)))
        case "continuing":
            keepCompletionTailAlive = true
            Task {
                // Siri already has its continuing card. Preserve the remainder
                // of the OS background window for tools and final synthesis.
                try? await Task.sleep(for: .seconds(16))
                await MainActor.run {
                    if backgroundTask != .invalid {
                        UIApplication.shared.endBackgroundTask(backgroundTask)
                    }
                }
            }
            return .continuing
        case "needs_input":
            return .needsInput
        default:
            let detail = response.error?.lowercased() ?? ""
            if detail.contains("active interactive turn") {
                throw SiriBackgroundError.busy
            }
            if detail.contains("credential") || detail.contains("unauthorized") {
                throw SiriBackgroundError.authentication
            }
            if detail.contains("configured for") || detail.contains("model") {
                throw SiriBackgroundError.configuration
            }
            throw SiriBackgroundError.unavailable
        }
    }

    private func waitForDaemon(baseURL: URL, bearer: String?) async throws {
        let deadline = Date().addingTimeInterval(Self.daemonReadyWaitSeconds)
        let healthURL = baseURL.appendingPathComponent("health")
        repeat {
            var request = URLRequest(url: healthURL)
            request.timeoutInterval = 1
            if let bearer, !bearer.isEmpty {
                request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
            }
            if let (_, response) = try? await URLSession.shared.data(for: request),
               let http = response as? HTTPURLResponse,
               (200..<300).contains(http.statusCode)
            {
                return
            }
            try await Task.sleep(for: .milliseconds(250))
        } while !Task.isCancelled && Date() < deadline
        throw SiriBackgroundError.unavailable
    }
}

@available(iOS 27.0, *)
extension AskMedousaIntent: LongRunningIntent {
    private func performLongRunningTurn(
        workshopId: String?,
        prompt: String,
        defaults: UserDefaults,
        preferences: SiriPreferences
    ) async throws -> SiriBackgroundOutcome {
        let taskProgress = progress
        taskProgress.totalUnitCount = Int64(Self.longRunningResultWaitSeconds)
        taskProgress.completedUnitCount = 0
        taskProgress.localizedDescription = "Working in Medousa"
        taskProgress.localizedAdditionalDescription = "Starting your request"

        return try await performBackgroundTask {
            let heartbeat = Task {
                while !Task.isCancelled {
                    try? await Task.sleep(for: .seconds(2))
                    guard !Task.isCancelled else { break }
                    taskProgress.completedUnitCount = min(
                        taskProgress.completedUnitCount + 2,
                        taskProgress.totalUnitCount - 1
                    )
                    taskProgress.localizedAdditionalDescription = "Using your Medousa tools"
                }
            }
            defer { heartbeat.cancel() }

            let outcome = try await runBackgroundTurn(
                workshopId: workshopId,
                prompt: prompt,
                defaults: defaults,
                preferences: preferences,
                resultWaitSeconds: Self.longRunningResultWaitSeconds
            )
            taskProgress.completedUnitCount = taskProgress.totalUnitCount
            taskProgress.localizedAdditionalDescription = "Request complete"
            return outcome
        }
    }
}

private func loadSiriBearer() -> String? {
    let query: [String: Any] = [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: "com.entasislabs.medousa-home.siri",
        kSecAttrAccount as String: "active-workshop-bearer.v1",
        kSecReturnData as String: true,
        kSecMatchLimit as String: kSecMatchLimitOne,
    ]
    var item: CFTypeRef?
    guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
          let data = item as? Data
    else {
        return nil
    }
    return String(data: data, encoding: .utf8)
}

private func readTurnResult(streamURL: URL, bearer: String?) async throws -> String {
    var request = URLRequest(url: streamURL)
    request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
    if let bearer, !bearer.isEmpty {
        request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
    }
    let (bytes, response) = try await URLSession.shared.bytes(for: request)
    guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
        throw SiriBackgroundError.unavailable
    }
    for try await line in bytes.lines where line.hasPrefix("data:") {
        let json = line.dropFirst(5).trimmingCharacters(in: .whitespaces)
        guard let data = json.data(using: .utf8),
              let event = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              event["terminal"] as? Bool == true
        else {
            continue
        }
        let answer = (event["final_text"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
            ?? (event["message"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
            ?? ""
        guard !answer.isEmpty else { throw SiriBackgroundError.unavailable }
        return String(answer.prefix(600))
    }
    throw SiriBackgroundError.unavailable
}
