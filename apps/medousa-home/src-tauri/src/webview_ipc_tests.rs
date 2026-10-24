//! Exercise Tauri's actual IPC extraction with the embedded browser layout.
use tauri::{Manager, Runtime, Webview, WebviewWindow};

#[tauri::command]
fn invoking_webview<R: Runtime>(webview: Webview<R>) -> String {
    webview.label().to_string()
}

#[tauri::command]
fn legacy_window<R: Runtime>(window: WebviewWindow<R>) -> String {
    window.label().to_string()
}

fn invoke(
    webview: &impl AsRef<Webview<tauri::test::MockRuntime>>,
    command: &str,
) -> Result<String, serde_json::Value> {
    tauri::test::get_ipc_response(
        webview,
        tauri::webview::InvokeRequest {
            cmd: command.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            // An IPC payload cannot substitute another view's native identity.
            body: tauri::ipc::InvokeBody::Json(serde_json::json!({
                "webview":"desktop-work-forged", "window":"desktop-work-forged",
            })),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
    .map(|response| response.deserialize().unwrap())
}

#[test]
fn main_shell_ipc_survives_embedded_browser_children() {
    let app = tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![invoking_webview, legacy_window])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    assert_eq!(invoke(&main, "legacy_window"), Ok("main".into()));
    assert_eq!(invoke(&main, "invoking_webview"), Ok("main".into()));

    let child = app
        .get_window("main")
        .unwrap()
        .add_child(
            tauri::webview::WebviewBuilder::new("browser-content-embed-test", Default::default()),
            tauri::LogicalPosition::new(0.0, 0.0),
            tauri::LogicalSize::new(800.0, 600.0),
        )
        .unwrap();
    child.hide().unwrap();
    assert_eq!(
        invoke(&main, "legacy_window"),
        Err(serde_json::json!("current webview is not a WebviewWindow"))
    );
    assert_eq!(invoke(&main, "invoking_webview"), Ok("main".into()));

    let work = tauri::WebviewWindowBuilder::new(&app, "desktop-work-test", Default::default())
        .build()
        .unwrap();
    assert_eq!(
        invoke(&work, "invoking_webview"),
        Ok("desktop-work-test".into())
    );
    assert_eq!(invoke(&main, "invoking_webview"), Ok("main".into()));

    let spotlight = tauri::WebviewWindowBuilder::new(&app, "desktop-spotlight", Default::default())
        .build()
        .unwrap();
    assert_eq!(
        invoke(&spotlight, "invoking_webview"),
        Ok("desktop-spotlight".into())
    );
}
