import AppIntents
import Foundation

/// S1 foreground gateway. Prompt contents remain in the shared App Group; the
/// deep link contains only a short-lived, one-time receipt consumed by Rust.
@available(iOS 18.0, *)
struct AskMedousaIntent: AppIntent {
    static let title: LocalizedStringResource = "Ask Medousa"
    static let description = IntentDescription(
        "Bring a request into Medousa using the currently selected workshop."
    )
    static let openAppWhenRun = true

    @Parameter(
        title: "Request",
        requestValueDialog: IntentDialog("What would you like to ask Medousa?")
    )
    var prompt: String

    static var parameterSummary: some ParameterSummary {
        Summary("Ask Medousa \(\.$prompt)")
    }

    func perform() async throws -> some IntentResult & ProvidesDialog & OpensIntent {
        let requestId = UUID().uuidString.lowercased()
        let payload: [String: Any] = [
            "requestId": requestId,
            "prompt": prompt,
            "createdAt": Date().timeIntervalSince1970,
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let encoded = String(data: data, encoding: .utf8),
              let defaults = UserDefaults(suiteName: "group.com.entasislabs.medousa-home")
        else {
            throw AskMedousaError.unavailable
        }
        defaults.set(encoded, forKey: "siri.pendingAsk.v1")

        var components = URLComponents()
        components.scheme = "medousa"
        components.host = "ask"
        components.queryItems = [URLQueryItem(name: "request", value: requestId)]

        guard let url = components.url else {
            throw AskMedousaError.invalidPrompt
        }
        return .result(
            opensIntent: OpenURLIntent(url),
            dialog: "Starting your request in Medousa."
        )
    }
}

@available(iOS 18.0, *)
private enum AskMedousaError: Error {
    case invalidPrompt
    case unavailable
}
