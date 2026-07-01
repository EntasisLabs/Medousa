import Foundation

/// C ABI consumed by Rust on iOS. Returns a heap-allocated JSON string; caller must free via medousa_live_activity_free_string.
@_cdecl("medousa_live_activity_is_available")
public func medousa_live_activity_is_available() -> Bool {
    if #available(iOS 16.1, *) {
        return MainActor.assumeIsolated {
            MedousaLiveActivityManager.shared.isAvailable()
        }
    }
    return false
}

@_cdecl("medousa_live_activity_sync")
public func medousa_live_activity_sync(_ json: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
    guard let json else { return nil }
    let payload = String(cString: json)

    if #available(iOS 16.1, *) {
        let result = MainActor.assumeIsolated {
            MedousaLiveActivityManager.shared.sync(json: payload)
        }
        return strdup(result)
    }

    let fallback = "{\"available\":false,\"active\":false,\"error\":\"iOS 16.1+ required\"}"
    return strdup(fallback)
}

@_cdecl("medousa_live_activity_free_string")
public func medousa_live_activity_free_string(_ ptr: UnsafeMutablePointer<CChar>?) {
    guard let ptr else { return }
    free(ptr)
}
