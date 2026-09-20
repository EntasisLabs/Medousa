import Foundation

/// One-shot commands sent by WidgetKit to the active native Live owner.
public enum MedousaLiveControlCommand {
    public enum Action: String {
        case mute
        case unmute
        case stop
    }

    public static let notificationName = "com.entasislabs.medousa-home.live-control"
    private static let storageKey = "medousa.live.control.v1"

    public static func send(_ action: Action) {
        guard let defaults = UserDefaults(suiteName: MedousaWidgetSnapshot.appGroupId) else { return }
        defaults.set(
            [
                "id": UUID().uuidString,
                "action": action.rawValue,
                "createdAt": Date().timeIntervalSince1970,
            ],
            forKey: storageKey
        )
        CFNotificationCenterPostNotification(
            CFNotificationCenterGetDarwinNotifyCenter(),
            CFNotificationName(notificationName as CFString),
            nil,
            nil,
            true
        )
    }

    public static func consume() -> (id: String, action: Action)? {
        guard let defaults = UserDefaults(suiteName: MedousaWidgetSnapshot.appGroupId),
              let payload = defaults.dictionary(forKey: storageKey)
        else { return nil }
        defaults.removeObject(forKey: storageKey)

        let now = Date().timeIntervalSince1970
        guard let id = payload["id"] as? String,
              id.count <= 64,
              let rawAction = payload["action"] as? String,
              let action = Action(rawValue: rawAction),
              let createdAt = payload["createdAt"] as? TimeInterval,
              createdAt <= now + 5,
              now - createdAt < 30
        else { return nil }
        return (id, action)
    }
}
