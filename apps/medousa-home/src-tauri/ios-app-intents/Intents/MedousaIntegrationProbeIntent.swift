import AppIntents

/// S0 metadata-discovery probe. Keep this side-effect free until the native
/// gateway can bind an invocation to one captured workshop authority.
@available(iOS 16.0, *)
struct MedousaIntegrationProbeIntent: AppIntent {
    static let title: LocalizedStringResource = "Check Medousa Integration"
    static let description = IntentDescription(
        "Confirms that Medousa actions are available to Siri and Shortcuts."
    )

    func perform() async throws -> some IntentResult & ProvidesDialog {
        .result(dialog: "Medousa actions are ready.")
    }
}

@available(iOS 18.0, *)
struct MedousaAppShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: AskMedousaIntent(),
            phrases: [
                "Ask \(.applicationName)",
                "Talk to \(.applicationName)",
            ],
            shortTitle: "Ask Medousa",
            systemImageName: "sparkles"
        )
        AppShortcut(
            intent: MedousaIntegrationProbeIntent(),
            phrases: [
                "Check \(.applicationName)",
                "Check \(.applicationName) integration",
            ],
            shortTitle: "Check Medousa",
            systemImageName: "waveform.badge.magnifyingglass"
        )
    }
}
