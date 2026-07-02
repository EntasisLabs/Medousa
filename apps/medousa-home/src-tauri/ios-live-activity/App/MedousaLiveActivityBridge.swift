import Foundation

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

@_cdecl("medousa_live_activity_free_string")
public func medousa_live_activity_free_string(_ ptr: UnsafeMutablePointer<CChar>?) {
    guard let ptr else { return }
    free(ptr)
}
