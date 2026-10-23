//! Native page history, independent of page JavaScript and shell URL history.

use serde::Serialize;
use tauri::Webview;
use tokio::sync::oneshot;

#[derive(Clone, Copy)]
pub(super) enum Action {
    Back,
    Forward,
    Stop,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct State {
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub url: Option<String>,
    pub loading: Option<bool>,
}

pub(super) async fn read(view: Webview, direction: Option<Action>) -> Result<State, String> {
    let (tx, rx) = oneshot::channel();
    view.with_webview(move |platform| {
        let result: Result<State, String> = {
            #[cfg(target_os = "macos")]
            unsafe {
                let page = &*platform.inner().cast::<objc2_web_kit::WKWebView>();
                match direction {
                    Some(Action::Back) => {
                        page.goBack();
                    }
                    Some(Action::Forward) => {
                        page.goForward();
                    }
                    Some(Action::Stop) => {
                        page.stopLoading();
                    }
                    None => {}
                }
                Ok(State {
                    can_go_back: page.canGoBack(),
                    can_go_forward: page.canGoForward(),
                    url: page
                        .URL()
                        .and_then(|url| url.absoluteString())
                        .map(|url| url.to_string()),
                    loading: Some(page.isLoading()),
                })
            }
            #[cfg(windows)]
            unsafe {
                let page = platform
                    .controller()
                    .CoreWebView2()
                    .map_err(|e| e.to_string())?;
                match direction {
                    Some(Action::Back) => page.GoBack().map_err(|e| e.to_string())?,
                    Some(Action::Forward) => page.GoForward().map_err(|e| e.to_string())?,
                    Some(Action::Stop) => page.Stop().map_err(|e| e.to_string())?,
                    None => {}
                }
                let mut back = Default::default();
                let mut forward = Default::default();
                page.CanGoBack(&mut back).map_err(|e| e.to_string())?;
                page.CanGoForward(&mut forward).map_err(|e| e.to_string())?;
                let mut source = Default::default();
                page.Source(&mut source).map_err(|e| e.to_string())?;
                Ok(State {
                    can_go_back: back.as_bool(),
                    can_go_forward: forward.as_bool(),
                    url: Some(webview2_com::take_pwstr(source)),
                    loading: None,
                })
            }
            #[cfg(any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            ))]
            {
                use webkit2gtk::WebViewExt;
                let page = platform.inner();
                match direction {
                    Some(Action::Back) => page.go_back(),
                    Some(Action::Forward) => page.go_forward(),
                    Some(Action::Stop) => page.stop_loading(),
                    None => {}
                }
                Ok(State {
                    can_go_back: page.can_go_back(),
                    can_go_forward: page.can_go_forward(),
                    url: page.uri().map(|url| url.to_string()),
                    loading: Some(page.is_loading()),
                })
            }
        };
        let _ = tx.send(result);
    })
    .map_err(|err| err.to_string())?;
    tokio::time::timeout(std::time::Duration::from_secs(2), rx)
        .await
        .map_err(|_| "native navigation state timed out".to_string())?
        .map_err(|_| "native navigation state channel closed".to_string())?
}
