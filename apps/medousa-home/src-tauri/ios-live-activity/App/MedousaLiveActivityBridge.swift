import Foundation
import Security
import UserNotifications

private let siriBearerService = "com.entasislabs.medousa-home.siri"
private let siriBearerAccount = "active-workshop-bearer.v1"

private func storeSiriBearer(_ bearer: String) -> Bool {
    let query: [String: Any] = [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: siriBearerService,
        kSecAttrAccount as String: siriBearerAccount,
    ]
    SecItemDelete(query as CFDictionary)
    guard !bearer.isEmpty else { return true }
    var insert = query
    insert[kSecValueData as String] = Data(bearer.utf8)
    insert[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    return SecItemAdd(insert as CFDictionary, nil) == errSecSuccess
}

/// Rust/Tauri invoke handlers run off the main thread; `@MainActor` types must hop to main first.
@available(iOS 16.1, *)
private func runOnMainActor<T>(_ work: @MainActor () -> T) -> T {
    if Thread.isMainThread {
        return MainActor.assumeIsolated(work)
    }
    return DispatchQueue.main.sync {
        MainActor.assumeIsolated(work)
    }
}

/// C ABI consumed by Rust on iOS. Returns a heap-allocated JSON string; caller must free via medousa_live_activity_free_string.
@_cdecl("medousa_live_activity_bridge_version")
public func medousa_live_activity_bridge_version() -> UInt32 {
    1
}

@_cdecl("medousa_live_activity_diagnostics")
public func medousa_live_activity_diagnostics() -> UnsafeMutablePointer<CChar>? {
    if #available(iOS 16.2, *) {
        let result = runOnMainActor {
            MedousaLiveActivityManager.shared.diagnosticsJson()
        }
        return strdup(result)
    }
    let fallback =
        "{\"bridgeLinked\":true,\"activitiesEnabled\":false,\"widgetExtensionInstalled\":false,\"supportsLiveActivities\":false,\"error\":\"iOS 16.2+ required\"}"
    return strdup(fallback)
}

@_cdecl("medousa_live_activity_is_available")
public func medousa_live_activity_is_available() -> Bool {
    if #available(iOS 16.2, *) {
        return runOnMainActor {
            MedousaLiveActivityManager.shared.isAvailable()
        }
    }
    return false
}

@_cdecl("medousa_live_activity_sync")
public func medousa_live_activity_sync(_ json: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
    guard let json else { return nil }
    let payload = String(cString: json)

    if #available(iOS 16.2, *) {
        let result = runOnMainActor {
            MedousaLiveActivityManager.shared.sync(json: payload)
        }
        return strdup(result)
    }

    let fallback = "{\"available\":false,\"active\":false,\"error\":\"iOS 16.2+ required\",\"pushToken\":null}"
    return strdup(fallback)
}

@_cdecl("medousa_live_activity_push_token")
public func medousa_live_activity_push_token() -> UnsafeMutablePointer<CChar>? {
    if #available(iOS 16.2, *) {
        let token = runOnMainActor {
            MedousaLiveActivityManager.shared.pushTokenHex()
        }
        guard let token, !token.isEmpty else { return nil }
        return strdup(token)
    }
    return nil
}

@_cdecl("medousa_live_voice_start")
public func medousa_live_voice_start(
    _ jsonPointer: UnsafePointer<CChar>?
) -> UnsafeMutablePointer<CChar>? {
    guard let jsonPointer else { return nil }
    if #available(iOS 17.0, *) {
        let json = String(cString: jsonPointer)
        return strdup(runOnMainActor {
            MedousaLiveVoiceSessionManager.shared.start(json: json)
        })
    }
    return strdup("{\"available\":false,\"active\":false,\"muted\":false,\"phase\":\"failed\",\"error\":\"iOS 17+ required\"}")
}

@_cdecl("medousa_live_voice_set_muted")
public func medousa_live_voice_set_muted(_ muted: Bool) -> UnsafeMutablePointer<CChar>? {
    if #available(iOS 17.0, *) {
        return strdup(runOnMainActor {
            MedousaLiveVoiceSessionManager.shared.setMuted(muted)
        })
    }
    return nil
}

@_cdecl("medousa_live_voice_stop")
public func medousa_live_voice_stop() -> UnsafeMutablePointer<CChar>? {
    if #available(iOS 17.0, *) {
        return strdup(runOnMainActor {
            MedousaLiveVoiceSessionManager.shared.stop()
        })
    }
    return nil
}

@_cdecl("medousa_live_voice_status")
public func medousa_live_voice_status() -> UnsafeMutablePointer<CChar>? {
    if #available(iOS 17.0, *) {
        return strdup(runOnMainActor {
            MedousaLiveVoiceSessionManager.shared.status()
        })
    }
    return strdup("{\"available\":false,\"active\":false,\"muted\":false,\"phase\":\"failed\",\"error\":\"iOS 17+ required\"}")
}

@_cdecl("medousa_carplay_live_exchange")
public func medousa_carplay_live_exchange(_ jsonPointer: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
    guard let jsonPointer else { return nil }
    if #available(iOS 26.4, *) {
        let json = String(cString: jsonPointer)
        return strdup(runOnMainActor { MedousaCarPlayLiveController.shared.exchange(json: json) })
    }
    return strdup("{\"enabled\":false,\"action\":null}")
}

private struct WidgetSyncResult: Encodable {
    let ok: Bool
    let error: String?
}

@_cdecl("medousa_home_widget_sync")
public func medousa_home_widget_sync(_ json: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
    guard let json else { return nil }
    let payload = String(cString: json)
    let error = MedousaWidgetSnapshotStore.save(json: payload)
    let result = WidgetSyncResult(ok: error == nil, error: error)
    guard let data = try? JSONEncoder().encode(result),
          let text = String(data: data, encoding: .utf8)
    else {
        return strdup("{\"ok\":false,\"error\":\"encode failed\"}")
    }
    return strdup(text)
}

@_cdecl("medousa_siri_consume_pending_ask")
public func medousa_siri_consume_pending_ask(
    _ requestIdPointer: UnsafePointer<CChar>?
) -> UnsafeMutablePointer<CChar>? {
    guard let requestIdPointer else { return nil }
    let requestId = String(cString: requestIdPointer)
    guard let defaults = UserDefaults(suiteName: "group.com.entasislabs.medousa-home"),
          let encoded = defaults.string(forKey: "siri.pendingAsk.v1"),
          let data = encoded.data(using: .utf8),
          let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          payload["requestId"] as? String == requestId,
          let createdAt = payload["createdAt"] as? TimeInterval,
          createdAt <= Date().timeIntervalSince1970 + 60,
          Date().timeIntervalSince1970 - createdAt < 600
    else {
        return nil
    }
    defaults.removeObject(forKey: "siri.pendingAsk.v1")
    return strdup(encoded)
}

/// Returns only the receipt identifier for a just-created Siri request. This
/// covers iOS cold starts where OpenURLIntent launches the app but the URL is
/// not retained for the webview. The normal consume path remains authoritative.
@_cdecl("medousa_siri_recent_pending_ask_id")
public func medousa_siri_recent_pending_ask_id() -> UnsafeMutablePointer<CChar>? {
    guard let defaults = UserDefaults(suiteName: "group.com.entasislabs.medousa-home"),
          let encoded = defaults.string(forKey: "siri.pendingAsk.v1"),
          let data = encoded.data(using: .utf8),
          let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          let requestId = payload["requestId"] as? String,
          !requestId.isEmpty,
          requestId.count <= 64,
          let createdAt = payload["createdAt"] as? TimeInterval,
          createdAt <= Date().timeIntervalSince1970 + 60,
          Date().timeIntervalSince1970 - createdAt < 120
    else {
        return nil
    }
    return strdup(requestId)
}

@_cdecl("medousa_siri_consume_pending_live_url")
public func medousa_siri_consume_pending_live_url() -> UnsafeMutablePointer<CChar>? {
    guard let defaults = UserDefaults(suiteName: "group.com.entasislabs.medousa-home"),
          let payload = defaults.dictionary(forKey: "siri.pendingLive.v1") else { return nil }
    defaults.removeObject(forKey: "siri.pendingLive.v1")
    let now = Date().timeIntervalSince1970
    guard let createdAt = payload["createdAt"] as? TimeInterval,
          createdAt <= now + 5, now - createdAt < 120,
          let url = payload["url"] as? String, url.count < 256,
          url.hasPrefix("medousa://live?") else { return nil }
    return strdup(url)
}

@_cdecl("medousa_siri_publish_ask_result")
public func medousa_siri_publish_ask_result(
    _ jsonPointer: UnsafePointer<CChar>?
) -> Bool {
    guard let jsonPointer,
          let defaults = UserDefaults(suiteName: "group.com.entasislabs.medousa-home")
    else {
        return false
    }
    defaults.set(String(cString: jsonPointer), forKey: "siri.askResult.v1")
    return true
}

@_cdecl("medousa_siri_store_execution_context")
public func medousa_siri_store_execution_context(
    _ jsonPointer: UnsafePointer<CChar>?,
    _ bearerPointer: UnsafePointer<CChar>?
) -> Bool {
    guard let jsonPointer,
          let defaults = UserDefaults(suiteName: "group.com.entasislabs.medousa-home")
    else {
        return false
    }
    let bearer = bearerPointer.map(String.init(cString:)) ?? ""
    guard storeSiriBearer(bearer) else { return false }
    let encoded = String(cString: jsonPointer)
    defaults.set(encoded, forKey: "siri.executionContext.v1")
    if let data = encoded.data(using: .utf8),
       let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
       let sessionId = payload["sessionId"] as? String,
       !sessionId.isEmpty
    {
        var contexts = defaults.dictionary(forKey: "siri.executionContexts.v1") as? [String: String] ?? [:]
        contexts[sessionId] = encoded
        defaults.set(contexts, forKey: "siri.executionContexts.v1")
    }
    return true
}

@_cdecl("medousa_siri_store_preferences")
public func medousa_siri_store_preferences(
    _ jsonPointer: UnsafePointer<CChar>?
) -> Bool {
    guard let jsonPointer,
          let defaults = UserDefaults(suiteName: "group.com.entasislabs.medousa-home")
    else {
        return false
    }
    defaults.set(String(cString: jsonPointer), forKey: "siri.preferences.v1")
    return true
}

@_cdecl("medousa_siri_notify_completion")
public func medousa_siri_notify_completion(_ bodyPointer: UnsafePointer<CChar>?) -> Bool {
    guard let bodyPointer else { return false }
    let content = UNMutableNotificationContent()
    content.title = "Medousa — turn ready"
    content.body = String(cString: bodyPointer)
    content.sound = .default
    UNUserNotificationCenter.current().add(
        UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil)
    )
    return true
}

@_cdecl("medousa_siri_store_workshops")
public func medousa_siri_store_workshops(
    _ jsonPointer: UnsafePointer<CChar>?
) -> Bool {
    guard let jsonPointer,
          let defaults = UserDefaults(suiteName: "group.com.entasislabs.medousa-home")
    else {
        return false
    }
    defaults.set(String(cString: jsonPointer), forKey: "siri.workshops.v1")
    return true
}

@_cdecl("medousa_live_activity_free_string")
public func medousa_live_activity_free_string(_ ptr: UnsafeMutablePointer<CChar>?) {
    guard let ptr else { return }
    free(ptr)
}
