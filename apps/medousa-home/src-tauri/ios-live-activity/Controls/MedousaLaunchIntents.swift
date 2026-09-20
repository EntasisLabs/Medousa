import AppIntents
import Foundation

func medousaLaunchURL(host: String, query: [URLQueryItem]) -> URL {
    var components = URLComponents()
    components.scheme = "medousa"
    components.host = host
    components.queryItems = query + [URLQueryItem(name: "request", value: UUID().uuidString.lowercased())]
    return components.url!
}

@available(iOS 18.0, *)
// AudioRecordingIntent grants the recording affordance; LiveActivityIntent makes
// WidgetKit execute this action in the app process, waking a backgrounded Medousa
// instead of leaving a notification queued in the widget extension.
struct OpenMedousaLiveIntent: AudioRecordingIntent, LiveActivityIntent {
    static let title: LocalizedStringResource = "Talk Live with Medousa"
    static let description = IntentDescription("Begin a Medousa Live conversation in the background.")
    static let openAppWhenRun = false

    @available(iOS 26.0, *)
    static var supportedModes: IntentModes { [.background] }

    func perform() async throws -> some IntentResult {
        CFNotificationCenterPostNotification(
            CFNotificationCenterGetDarwinNotifyCenter(),
            CFNotificationName("com.entasislabs.medousa-home.live.start" as CFString),
            nil,
            nil,
            true
        )
        return .result()
    }
}

@available(iOS 18.0, *)
struct OpenMedousaAskIntent: AppIntent {
    static let title: LocalizedStringResource = "Ask Medousa"
    static let description = IntentDescription("Open a new Medousa chat and focus the composer.")
    static let openAppWhenRun = true

    func perform() async throws -> some IntentResult & OpensIntent {
        .result(opensIntent: OpenURLIntent(medousaLaunchURL(
            host: "compose",
            query: [URLQueryItem(name: "action", value: "new")]
        )))
    }
}

@available(iOS 18.0, *)
struct OpenMedousaCameraIntent: AppIntent {
    static let title: LocalizedStringResource = "Take a Photo for Medousa"
    static let openAppWhenRun = true

    func perform() async throws -> some IntentResult & OpensIntent {
        .result(opensIntent: OpenURLIntent(medousaLaunchURL(
            host: "compose",
            query: [URLQueryItem(name: "action", value: "camera")]
        )))
    }
}

@available(iOS 18.0, *)
struct OpenMedousaPhotosIntent: AppIntent {
    static let title: LocalizedStringResource = "Choose Photos for Medousa"
    static let openAppWhenRun = true

    func perform() async throws -> some IntentResult & OpensIntent {
        .result(opensIntent: OpenURLIntent(medousaLaunchURL(
            host: "compose",
            query: [URLQueryItem(name: "action", value: "photos")]
        )))
    }
}
