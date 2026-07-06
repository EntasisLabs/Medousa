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

    @discardableResult
    static func applyRemoteNotification(_ userInfo: [AnyHashable: Any]) -> Bool {
        let type = stringValue(userInfo["medousaType"])
        guard type == "pulse_snapshot" else {
            return false
        }

        if let pulseJson = stringValue(userInfo["medousaPulse"]), save(json: pulseJson) == nil {
            return true
        }

        let snapshot = MedousaWidgetSnapshot(
            mood: stringValue(userInfo["mood"]) ?? "quiet",
            workshopName: stringValue(userInfo["workshopName"]) ?? "Workshop",
            eyebrow: stringValue(userInfo["eyebrow"]) ?? "Quiet",
            headline: stringValue(userInfo["headline"]) ?? "Medousa",
            subline: stringValue(userInfo["subline"]),
            motionSummary: stringValue(userInfo["motionSummary"]),
            blockedCount: intValue(userInfo["blockedCount"]) ?? 0,
            primaryCardId: stringValue(userInfo["primaryCardId"]),
            updatedAt: Date()
        )
        MedousaWidgetSnapshot.save(snapshot)
        WidgetCenter.shared.reloadTimelines(ofKind: MedousaWidgetSnapshot.widgetKind)
        return true
    }

    private static func stringValue(_ value: Any?) -> String? {
        if let text = value as? String {
            let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
            return trimmed.isEmpty ? nil : trimmed
        }
        if let number = value as? NSNumber {
            return number.stringValue
        }
        return nil
    }

    private static func intValue(_ value: Any?) -> Int? {
        if let number = value as? NSNumber {
            return number.intValue
        }
        if let text = value as? String {
            return Int(text)
        }
        return nil
    }
}
