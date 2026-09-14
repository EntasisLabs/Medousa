import AppIntents
import Foundation

/// S1 foreground continuation. The durable native gateway will replace this
/// route once it can capture and authenticate one workshop authority without a
/// running webview.
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

    func perform() async throws -> some IntentResult & OpensIntent {
        var components = URLComponents()
        components.scheme = "medousa"
        components.host = "ask"
        components.queryItems = [URLQueryItem(name: "prompt", value: prompt)]

        guard let url = components.url else {
            throw AskMedousaError.invalidPrompt
        }
        return .result(opensIntent: OpenURLIntent(url))
    }
}

@available(iOS 18.0, *)
private enum AskMedousaError: Error {
    case invalidPrompt
}
