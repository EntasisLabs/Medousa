import AppIntents
import Foundation

private func liveLaunchURL(mode: String) -> URL {
    let id = UUID().uuidString.lowercased()
    let url = URL(string: "medousa://live?mode=\(mode)&request=\(id)")!
    let defaults = UserDefaults(suiteName: "group.com.entasislabs.medousa-home")
    defaults?.set(["url": url.absoluteString, "createdAt": Date().timeIntervalSince1970],
                  forKey: "siri.pendingLive.v1")
    return url
}

// Siri launches the existing in-app Live controller. It does not hold an
// invocation open while voice or tool work runs.
@available(iOS 18.0, *)
struct StartMedousaLiveIntent: AppIntent {
    static let title: LocalizedStringResource = "Start New Medousa Live Session"
    static let description = IntentDescription("Open Medousa and start Live in a new chat in the selected workshop.")
    static let openAppWhenRun = true

    func perform() async throws -> some IntentResult & OpensIntent {
        return .result(opensIntent: OpenURLIntent(liveLaunchURL(mode: "new")))
    }
}

@available(iOS 18.0, *)
struct ResumeMedousaLiveIntent: AppIntent {
    static let title: LocalizedStringResource = "Resume Medousa Live"
    static let description = IntentDescription("Open Medousa and talk live in the current chat, creating a chat if needed.")
    static let openAppWhenRun = true

    func perform() async throws -> some IntentResult & OpensIntent {
        return .result(opensIntent: OpenURLIntent(liveLaunchURL(mode: "resume")))
    }
}
