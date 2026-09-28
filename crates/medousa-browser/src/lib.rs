//! Page exports for the browser workshop. No Axum listener.
//!
//! Native workspace checks compile an empty library. The exports exist only on
//! `wasm32-unknown-unknown`.

#[cfg(not(target_arch = "wasm32"))]
pub fn browser_bridge_is_wasm32_only() {}

#[cfg(target_arch = "wasm32")]
mod page {

    use wasm_bindgen::prelude::*;

    fn js_error(error: String) -> JsValue {
        JsValue::from_str(&error)
    }

    #[wasm_bindgen]
    pub async fn boot() -> Result<(), JsValue> {
        medousa::wasm_daemon::boot().await.map_err(js_error)
    }

    #[wasm_bindgen]
    pub fn configure_inference(base_url: String, api_key: String, model: String) {
        medousa::wasm_daemon::configure_inference(&base_url, &api_key, &model);
    }

    #[wasm_bindgen]
    pub async fn list_sessions() -> Result<String, JsValue> {
        let rows = medousa::wasm_daemon::list_sessions()
            .await
            .map_err(js_error)?;
        serde_json_string(&rows)
    }

    #[wasm_bindgen]
    pub async fn create_session(title: String) -> Result<String, JsValue> {
        let row = medousa::wasm_daemon::create_session(&title)
            .await
            .map_err(js_error)?;
        serde_json_string(&row)
    }

    #[wasm_bindgen]
    pub async fn list_notes() -> Result<String, JsValue> {
        let rows = medousa::wasm_daemon::list_notes().await.map_err(js_error)?;
        serde_json_string(&rows)
    }

    #[wasm_bindgen]
    pub async fn save_note(
        note_id: String,
        title: String,
        body: String,
    ) -> Result<String, JsValue> {
        let row = medousa::wasm_daemon::save_note(&note_id, &title, &body)
            .await
            .map_err(js_error)?;
        serde_json_string(&row)
    }

    #[wasm_bindgen]
    pub async fn start_turn(
        session_id: String,
        text: String,
        on_event: &js_sys::Function,
    ) -> Result<String, JsValue> {
        let on_event = on_event.clone();
        medousa::wasm_daemon::start_turn(&session_id, &text, move |event| {
            let _ = on_event.call1(&JsValue::NULL, &JsValue::from_str(event));
        })
        .await
        .map_err(js_error)
    }

    #[wasm_bindgen]
    pub fn run_grapheme(source: String) -> String {
        medousa::wasm_daemon::run_grapheme(&source)
    }

    #[wasm_bindgen]
    pub async fn dial_iroh_ticket(ticket: String, path: String) -> Result<String, JsValue> {
        medousa::wasm_daemon::dial_iroh_ticket(&ticket, &path)
            .await
            .map_err(js_error)
    }

    #[wasm_bindgen]
    pub async fn read_vault(path: String) -> Result<String, JsValue> {
        medousa::wasm_daemon::read_vault(&path)
            .await
            .map_err(js_error)
    }

    #[wasm_bindgen]
    pub async fn write_vault(path: String, body: String) -> Result<(), JsValue> {
        medousa::wasm_daemon::write_vault(&path, &body)
            .await
            .map_err(js_error)
    }

    fn serde_json_string(value: &impl serde::Serialize) -> Result<String, JsValue> {
        serde_json::to_string(value).map_err(|err| js_error(err.to_string()))
    }
}
