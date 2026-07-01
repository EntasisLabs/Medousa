import ActivityKit
import Foundation

/// Shared between the main app (Activity.request) and the Widget Extension (ActivityConfiguration).
public struct MedousaWorkAttributes: ActivityAttributes {
    public struct ContentState: Codable, Hashable {
        public var mood: String
        public var eyebrow: String
        public var headline: String
        public var subline: String?
        public var motionSummary: String?
        public var blockedCount: Int
        public var primaryCardId: String?

        public init(
            mood: String,
            eyebrow: String,
            headline: String,
            subline: String? = nil,
            motionSummary: String? = nil,
            blockedCount: Int = 0,
            primaryCardId: String? = nil
        ) {
            self.mood = mood
            self.eyebrow = eyebrow
            self.headline = headline
            self.subline = subline
            self.motionSummary = motionSummary
            self.blockedCount = blockedCount
            self.primaryCardId = primaryCardId
        }
    }

    public var workshopName: String

    public init(workshopName: String) {
        self.workshopName = workshopName
    }
}
