import AppIntents
import Foundation

/// S1 foreground gateway. Prompt contents remain in the shared App Group and
/// only a short-lived, one-time receipt is exposed to the trusted shell.
@available(iOS 18.0, *)
struct AskMedousaIntent: AppIntent, ForegroundContinuableIntent {
    private static let resultWaitSeconds: TimeInterval = 20
    static let title: LocalizedStringResource = "Ask Medousa"
    static let description = IntentDescription(
        "Bring a request into Medousa using the currently selected workshop."
    )
    static let openAppWhenRun = false

    @available(iOS 26.0, *)
    static var supportedModes: IntentModes {
        [.background, .foreground(.dynamic)]
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

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let requestId = UUID().uuidString.lowercased()
        let workshopId = workshop?.id
            ?? WorkshopEntitySnapshot.load().first(where: \.isActive)?.id
        let payload: [String: Any] = [
            "requestId": requestId,
            "prompt": prompt,
            "workshopId": workshopId ?? "",
            "createdAt": Date().timeIntervalSince1970,
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let encoded = String(data: data, encoding: .utf8),
              let defaults = UserDefaults(suiteName: "group.com.entasislabs.medousa-home")
        else {
            throw AskMedousaError.unavailable
        }
        defaults.set(encoded, forKey: "siri.pendingAsk.v1")

        try await requestToContinueInForeground(
            "Opening Medousa to start your request."
        )
        if let answer = await waitForResult(requestId: requestId, defaults: defaults) {
            return .result(dialog: "\(answer)")
        }
        return .result(dialog: "Your request is continuing in Medousa.")
    }

    private func waitForResult(
        requestId: String,
        defaults: UserDefaults
    ) async -> String? {
        let deadline = Date().addingTimeInterval(Self.resultWaitSeconds)
        while !Task.isCancelled && Date() < deadline {
            if let encoded = defaults.string(forKey: "siri.askResult.v1"),
               let data = encoded.data(using: .utf8),
               let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               payload["requestId"] as? String == requestId,
               let createdAt = payload["createdAt"] as? TimeInterval,
               Date().timeIntervalSince1970 - createdAt < 60,
               let text = payload["text"] as? String,
               !text.isEmpty
            {
                defaults.removeObject(forKey: "siri.askResult.v1")
                return text
            }
            try? await Task.sleep(for: .milliseconds(250))
        }
        return nil
    }
}

@available(iOS 18.0, *)
private enum AskMedousaError: Error {
    case unavailable
}
