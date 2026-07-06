import Foundation
import UIKit
import WidgetKit

enum MedousaPushBackgroundHandler {
    private static var installed = false

    static func installIfNeeded() {
        guard !installed else { return }
        guard let delegate = UIApplication.shared.delegate else {
            NSLog("[medousa-push] no UIApplication.delegate for background handler")
            return
        }

        let cls: AnyClass = type(of: delegate)
        let selector = sel_registerName("application:didReceiveRemoteNotification:fetchCompletionHandler:")
        let block: @convention(block) (AnyObject, UIApplication, [AnyHashable: Any], @escaping (UIBackgroundFetchResult) -> Void) -> Void =
            { _, _, userInfo, completion in
                let handled = MedousaWidgetSnapshotStore.applyRemoteNotification(userInfo)
                completion(handled ? .newData : .noData)
            }
        let imp = imp_implementationWithBlock(block as Any)
        if class_addMethod(cls, selector, imp, "v@:@@?") {
            NSLog("[medousa-push] installed didReceiveRemoteNotification handler")
        }
        installed = true
    }
}

@_cdecl("medousa_ios_push_setup")
public func medousa_ios_push_setup() {
    if Thread.isMainThread {
        MedousaPushBackgroundHandler.installIfNeeded()
    } else {
        DispatchQueue.main.sync {
            MedousaPushBackgroundHandler.installIfNeeded()
        }
    }
}
