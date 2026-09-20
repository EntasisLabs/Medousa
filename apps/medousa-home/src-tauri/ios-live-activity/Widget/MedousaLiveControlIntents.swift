import AppIntents
import Foundation

@available(iOS 17.0, *)
struct SetMedousaLiveMutedIntent: LiveActivityIntent {
    static let title: LocalizedStringResource = "Mute Medousa Live"
    static let description = IntentDescription("Mute or unmute the active Medousa Live session.")

    @Parameter(title: "Muted")
    var muted: Bool

    init() {}

    init(muted: Bool) {
        self.muted = muted
    }

    func perform() async throws -> some IntentResult {
        MedousaLiveControlCommand.send(muted ? .mute : .unmute)
        return .result()
    }
}

@available(iOS 17.0, *)
struct StopMedousaLiveIntent: LiveActivityIntent {
    static let title: LocalizedStringResource = "End Medousa Live"
    static let description = IntentDescription("End the active Medousa Live session.")

    func perform() async throws -> some IntentResult {
        MedousaLiveControlCommand.send(.stop)
        return .result()
    }
}
