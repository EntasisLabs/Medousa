import Foundation

/// Pulse snapshot shared between the main app and the home-screen widget via App Group.
public struct MedousaWidgetSnapshot: Codable, Hashable {
    public let mood: String
    public let workshopName: String
    public let eyebrow: String
    public let headline: String
    public let subline: String?
    public let motionSummary: String?
    public let blockedCount: Int
    public let primaryCardId: String?
    public let updatedAt: Date

    public init(
        mood: String,
        workshopName: String,
        eyebrow: String,
        headline: String,
        subline: String? = nil,
        motionSummary: String? = nil,
        blockedCount: Int = 0,
        primaryCardId: String? = nil,
        updatedAt: Date = Date()
    ) {
        self.mood = mood
        self.workshopName = workshopName
        self.eyebrow = eyebrow
        self.headline = headline
        self.subline = subline
        self.motionSummary = motionSummary
        self.blockedCount = blockedCount
        self.primaryCardId = primaryCardId
        self.updatedAt = updatedAt
    }

    public static let appGroupId = "group.com.entasislabs.medousa-home"
    public static let widgetKind = "MedousaHomeGlance"
    private static let storageKey = "medousa.pulse.snapshot"

    public static func load() -> MedousaWidgetSnapshot? {
        guard let defaults = UserDefaults(suiteName: appGroupId),
              let data = defaults.data(forKey: storageKey)
        else {
            return nil
        }
        return try? JSONDecoder().decode(MedousaWidgetSnapshot.self, from: data)
    }

    public static func save(_ snapshot: MedousaWidgetSnapshot) {
        guard let defaults = UserDefaults(suiteName: appGroupId),
              let data = try? JSONEncoder().encode(snapshot)
        else {
            return
        }
        defaults.set(data, forKey: storageKey)
    }

    public static let placeholder = MedousaWidgetSnapshot(
        mood: "working",
        workshopName: "Workshop",
        eyebrow: "In motion",
        headline: "Medousa is running",
        subline: "Open the app to connect",
        motionSummary: "2 running",
        blockedCount: 0,
        primaryCardId: nil
    )
}
