import Foundation
import WidgetKit

struct MedousaPulsePayload: Decodable {
    let mood: String
    let workshopName: String
    let eyebrow: String
    let headline: String
    let subline: String?
    let motionSummary: String?
    let blockedCount: Int
    let primaryCardId: String?
}

enum MedousaWidgetSnapshotStore {
    static func save(json: String) -> String? {
        guard let data = json.data(using: .utf8) else {
            return "invalid json"
        }

        let payload: MedousaPulsePayload
        do {
            payload = try JSONDecoder().decode(MedousaPulsePayload.self, from: data)
        } catch {
            return error.localizedDescription
        }

        let snapshot = MedousaWidgetSnapshot(
            mood: payload.mood,
            workshopName: payload.workshopName,
            eyebrow: payload.eyebrow,
            headline: payload.headline,
            subline: payload.subline,
            motionSummary: payload.motionSummary,
            blockedCount: payload.blockedCount,
            primaryCardId: payload.primaryCardId,
            updatedAt: Date()
        )
        MedousaWidgetSnapshot.save(snapshot)
        WidgetCenter.shared.reloadTimelines(ofKind: MedousaWidgetSnapshot.widgetKind)
        return nil
    }
}
