import ActivityKit
import Foundation

private struct SyncPayload: Decodable {
    let mood: String
    let workshopName: String
    let eyebrow: String
    let headline: String
    let subline: String?
    let motionSummary: String?
    let blockedCount: Int
    let primaryCardId: String?
}

private struct SyncResult: Encodable {
    let available: Bool
    let active: Bool
    let error: String?
}

@MainActor
final class MedousaLiveActivityManager {
    static let shared = MedousaLiveActivityManager()

    private var current: Activity<MedousaWorkAttributes>?

    private init() {}

    func isAvailable() -> Bool {
        ActivityAuthorizationInfo().areActivitiesEnabled
    }

    func sync(json: String) -> String {
        guard let data = json.data(using: .utf8) else {
            return encodeResult(SyncResult(available: isAvailable(), active: current != nil, error: "invalid json"))
        }

        let payload: SyncPayload
        do {
            payload = try JSONDecoder().decode(SyncPayload.self, from: data)
        } catch {
            return encodeResult(SyncResult(available: isAvailable(), active: current != nil, error: error.localizedDescription))
        }

        let shouldRun = payload.mood == "working" || payload.mood == "waiting"
        if shouldRun {
            return encodeResult(startOrUpdate(payload))
        }
        return encodeResult(endActivity())
    }

    private func startOrUpdate(_ payload: SyncPayload) -> SyncResult {
        guard isAvailable() else {
            return SyncResult(available: false, active: false, error: "Live Activities disabled in Settings")
        }

        let state = MedousaWorkAttributes.ContentState(
            mood: payload.mood,
            eyebrow: payload.eyebrow,
            headline: payload.headline,
            subline: payload.subline,
            motionSummary: payload.motionSummary,
            blockedCount: payload.blockedCount,
            primaryCardId: payload.primaryCardId
        )

        if let activity = current {
            Task {
                await activity.update(ActivityContent(state: state, staleDate: Date().addingTimeInterval(60 * 15)))
            }
            return SyncResult(available: true, active: true, error: nil)
        }

        let attributes = MedousaWorkAttributes(workshopName: payload.workshopName)
        let content = ActivityContent(state: state, staleDate: Date().addingTimeInterval(60 * 15))

        do {
            let activity = try Activity.request(
                attributes: attributes,
                content: content,
                pushType: nil
            )
            current = activity
            return SyncResult(available: true, active: true, error: nil)
        } catch {
            return SyncResult(available: true, active: false, error: error.localizedDescription)
        }
    }

    private func endActivity() -> SyncResult {
        guard let activity = current else {
            return SyncResult(available: isAvailable(), active: false, error: nil)
        }

        current = nil
        Task {
            await activity.end(nil, dismissalPolicy: .default)
        }
        return SyncResult(available: isAvailable(), active: false, error: nil)
    }

    private func encodeResult(_ result: SyncResult) -> String {
        guard let data = try? JSONEncoder().encode(result),
              let text = String(data: data, encoding: .utf8) else {
            return "{\"available\":false,\"active\":false,\"error\":\"encode failed\"}"
        }
        return text
    }
}
