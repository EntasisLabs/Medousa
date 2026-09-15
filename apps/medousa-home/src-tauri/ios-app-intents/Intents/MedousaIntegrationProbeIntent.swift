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
private func runFocusedMedousaPrompt(
    _ prompt: String
) async throws -> some IntentResult & ProvidesDialog & ShowsSnippetView {
    let ask = AskMedousaIntent()
    ask.prompt = prompt
    return try await ask.perform()
}

@available(iOS 18.0, *)
struct MedousaAttentionIntent: AppIntent {
    static let title: LocalizedStringResource = "What Needs My Attention"
    static let openAppWhenRun = false
    func perform() async throws -> some IntentResult & ProvidesDialog & ShowsSnippetView {
        try await runFocusedMedousaPrompt("What needs my attention right now? Give me the most important concise update.")
    }
}

@available(iOS 18.0, *)
struct MedousaSummarizeActiveIntent: AppIntent {
    static let title: LocalizedStringResource = "Summarize Active Work"
    static let openAppWhenRun = false
    func perform() async throws -> some IntentResult & ProvidesDialog & ShowsSnippetView {
        try await runFocusedMedousaPrompt("Summarize my active work and the next action in a concise spoken update.")
    }
}

@available(iOS 18.0, *)
struct MedousaCheckWorkIntent: AppIntent {
    static let title: LocalizedStringResource = "Check Running Work"
    static let openAppWhenRun = false
    func perform() async throws -> some IntentResult & ProvidesDialog & ShowsSnippetView {
        try await runFocusedMedousaPrompt("Check my running work. Tell me what finished, what is still running, and what needs me.")
    }
}

@available(iOS 18.0, *)
struct MedousaJournalIntent: AppIntent {
    static let title: LocalizedStringResource = "Capture in Medousa Journal"
    static let openAppWhenRun = false

    @Parameter(title: "Entry", requestValueDialog: "What should I capture?")
    var entry: String

    func perform() async throws -> some IntentResult & ProvidesDialog & ShowsSnippetView {
        try await runFocusedMedousaPrompt("Capture this in my journal, preserving my wording and confirming where it was saved: \(entry)")
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
        AppShortcut(
            intent: MedousaAttentionIntent(),
            phrases: ["What needs my attention in \(.applicationName)"],
            shortTitle: "Needs Attention",
            systemImageName: "exclamationmark.bubble"
        )
        AppShortcut(
            intent: MedousaSummarizeActiveIntent(),
            phrases: ["Summarize my work in \(.applicationName)"],
            shortTitle: "Summarize Work",
            systemImageName: "text.alignleft"
        )
        AppShortcut(
            intent: MedousaCheckWorkIntent(),
            phrases: ["Check my work in \(.applicationName)"],
            shortTitle: "Check Work",
            systemImageName: "checklist"
        )
        AppShortcut(
            intent: MedousaJournalIntent(),
            phrases: ["Capture this in \(.applicationName)"],
            shortTitle: "Capture Journal",
            systemImageName: "book.closed"
        )
    }
}
