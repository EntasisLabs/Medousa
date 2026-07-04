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
    let pushToken: String?
}

@available(iOS 16.2, *)
@MainActor
final class MedousaLiveActivityManager {
    static let shared = MedousaLiveActivityManager()

    private var current: Activity<MedousaWorkAttributes>?
    private var currentPushToken: String?
    private var pushTokenTask: Task<Void, Never>?

    private init() {
        reconcileExistingActivities()
    }

    func isAvailable() -> Bool {
        ActivityAuthorizationInfo().areActivitiesEnabled
    }

    func pushTokenHex() -> String? {
        currentPushToken
    }

    func diagnosticsJson() -> String {
        let info = Bundle.main.infoDictionary
        let supports = (info?["NSSupportsLiveActivities"] as? Bool) ?? false
        let widgetInstalled =
            Bundle.main.path(
                forResource: "MedousaWorkWidget",
                ofType: "appex",
                inDirectory: "PlugIns"
            ) != nil
        let auth = ActivityAuthorizationInfo()
        let payload: [String: Any] = [
            "bridgeLinked": true,
            "activitiesEnabled": auth.areActivitiesEnabled,
            "widgetExtensionInstalled": widgetInstalled,
            "supportsLiveActivities": supports,
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let text = String(data: data, encoding: .utf8)
        else {
            return "{\"bridgeLinked\":true,\"error\":\"encode diagnostics failed\"}"
        }
        return text
    }

    func sync(json: String) -> String {
        guard let data = json.data(using: .utf8) else {
            return encodeResult(SyncResult(
                available: isAvailable(),
                active: current != nil,
                error: "invalid json",
                pushToken: currentPushToken
            ))
        }

        let payload: SyncPayload
        do {
            payload = try JSONDecoder().decode(SyncPayload.self, from: data)
        } catch {
            return encodeResult(SyncResult(
                available: isAvailable(),
                active: current != nil,
                error: error.localizedDescription,
                pushToken: currentPushToken
            ))
        }

        _ = MedousaWidgetSnapshotStore.save(json: json)

        let shouldRun = payload.mood == "working" || payload.mood == "waiting"
        if shouldRun {
            return encodeResult(startOrUpdate(payload))
        }
        return encodeResult(endActivity())
    }

    /// Re-attach to a Live Activity already on the Lock Screen after process restart.
    @discardableResult
    private func reconcileExistingActivities() -> Activity<MedousaWorkAttributes>? {
        let activities = Activity<MedousaWorkAttributes>.activities
        guard !activities.isEmpty else {
            current = nil
            return nil
        }

        let keeper = activities.max(by: { $0.content.staleDate ?? .distantPast < $1.content.staleDate ?? .distantPast })
            ?? activities[activities.count - 1]

        if activities.count > 1 {
            NSLog("[live-activity] reconciling %d duplicate activities", activities.count)
            for activity in activities where activity.id != keeper.id {
                Task {
                    await activity.end(nil, dismissalPolicy: .immediate)
                }
            }
        }

        current = keeper
        observePushToken(for: keeper)
        return keeper
    }

    private func startOrUpdate(_ payload: SyncPayload) -> SyncResult {
        guard isAvailable() else {
            return SyncResult(
                available: false,
                active: false,
                error: "Live Activities disabled in Settings",
                pushToken: nil
            )
        }

        if current == nil {
            _ = reconcileExistingActivities()
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
        let content = ActivityContent(state: state, staleDate: Date().addingTimeInterval(60 * 15))

        if let activity = current {
            Task {
                await activity.update(content)
            }
            return SyncResult(available: true, active: true, error: nil, pushToken: currentPushToken)
        }

        let attributes = MedousaWorkAttributes(workshopName: payload.workshopName)

        do {
            let activity = try Activity.request(
                attributes: attributes,
                content: content,
                pushType: .token
            )
            current = activity
            observePushToken(for: activity)
            return SyncResult(available: true, active: true, error: nil, pushToken: currentPushToken)
        } catch let pushError {
            NSLog(
                "[live-activity] push token start failed (%@) — falling back to local-only",
                pushError.localizedDescription
            )
            do {
                let activity = try Activity.request(
                    attributes: attributes,
                    content: content,
                    pushType: nil
                )
                current = activity
                currentPushToken = nil
                pushTokenTask?.cancel()
                pushTokenTask = nil
                return SyncResult(available: true, active: true, error: nil, pushToken: nil)
            } catch {
                return SyncResult(
                    available: true,
                    active: false,
                    error: error.localizedDescription,
                    pushToken: nil
                )
            }
        }
    }

    private func endActivity() -> SyncResult {
        pushTokenTask?.cancel()
        pushTokenTask = nil
        currentPushToken = nil
        current = nil

        let activities = Activity<MedousaWorkAttributes>.activities
        guard !activities.isEmpty else {
            return SyncResult(available: isAvailable(), active: false, error: nil, pushToken: nil)
        }

        // Immediate dismissal when work is done — default policy left the LA stuck
        // on the Lock Screen after workshop turns finished in the background.
        for activity in activities {
            Task {
                await activity.end(nil, dismissalPolicy: .immediate)
            }
        }
        return SyncResult(available: isAvailable(), active: false, error: nil, pushToken: nil)
    }

    private func observePushToken(for activity: Activity<MedousaWorkAttributes>) {
        pushTokenTask?.cancel()
        pushTokenTask = Task { [weak self] in
            for await tokenData in activity.pushTokenUpdates {
                guard !Task.isCancelled else { break }
                let hex = tokenData.map { String(format: "%02x", $0) }.joined()
                await MainActor.run {
                    self?.currentPushToken = hex
                }
            }
        }
    }

    private func encodeResult(_ result: SyncResult) -> String {
        guard let data = try? JSONEncoder().encode(result),
              let text = String(data: data, encoding: .utf8) else {
            return "{\"available\":false,\"active\":false,\"error\":\"encode failed\",\"pushToken\":null}"
        }
        return text
    }
}
