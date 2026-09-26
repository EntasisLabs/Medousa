import SwiftRs
import Tauri
import WebKit

class IosWebviewInsetsPlugin: Plugin {
  override func load(webview: WKWebView) {
    webview.scrollView.contentInsetAdjustmentBehavior = .never

    if #available(iOS 26.0, *) {
      // UIKit's automatic hard top-edge effect paints a dividing line at the
      // status-bar safe-area boundary once the app uses the UIScene lifecycle.
      webview.scrollView.topEdgeEffect.isHidden = true
    }
  }
}

@_cdecl("init_plugin_ios_webview_insets")
func initPlugin() -> Plugin {
  return IosWebviewInsetsPlugin()
}
