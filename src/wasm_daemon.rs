//! Browser composition of `medousa_daemon`.
//!
//! Same schema order as embedded boot: identity schema, `RuntimeFactory::from_db`,
//! then `ensure_stasis_runtime_schema`, then workshop session and note tables.
//! IndexedDB is opened beside Stasis's wasm `ws://` guard. In-memory boot is a
//! hard error. Vault bytes live in origin-private OPFS. Inference is `fetch`.
//! Grapheme scripts run on grapheme-wasm. Pairing dials the Iroh relay client.

use std::sync::Mutex;

use grapheme_wasm::{execute, ExecuteRequest};
use serde::Serialize;
use serde_json::{json, Value};
use stasis::infrastructure::memory::locus_node_store_factory::LocusMemoryStore;
use stasis::infrastructure::memory::surreal_identity_memory_store::SurrealIdentityMemoryStore;
use stasis::prelude::{RuntimeComposition, RuntimeFactory};
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::stasis_surreal_schema::ensure_stasis_runtime_schema;
use crate::workshop_authority;

pub const WORKSHOP_ENDPOINT: &str = "indxdb://medousa";
const WORKSHOP_NS: &str = "medousa";
const WORKSHOP_DB: &str = "runtime";
const INSTALLATION_KEY: &str = "medousa.installation_id";

const WORKSHOP_TABLES: &[&str] = &[
    "DEFINE TABLE workshop_session SCHEMALESS",
    "DEFINE TABLE workshop_note SCHEMALESS",
    "DEFINE TABLE workshop_turn SCHEMALESS",
];

struct InferenceConfig {
    base_url: String,
    api_key: String,
    model: String,
}

struct Workshop {
    runtime: RuntimeComposition,
    _locus: std::sync::Arc<LocusMemoryStore>,
}

static WORKSHOP: Mutex<Option<Workshop>> = Mutex::new(None);
static INFERENCE: Mutex<InferenceConfig> = Mutex::new(InferenceConfig {
    base_url: String::new(),
    api_key: String::new(),
    model: String::new(),
});

/// Browser workshops never boot on Stasis in-memory storage.
pub fn reject_memory_boot(is_memory: bool) -> Result<(), &'static str> {
    if is_memory {
        Err("browser workshop refuses in-memory boot")
    } else {
        Ok(())
    }
}

/// Personal workshop endpoint. SurrealKV paths are not a browser store.
pub fn accept_workshop_endpoint(endpoint: &str) -> Result<(), &'static str> {
    let normalized = endpoint.trim().to_ascii_lowercase();
    if normalized == WORKSHOP_ENDPOINT {
        Ok(())
    } else if normalized.starts_with("mem://")
        || normalized.starts_with("memory")
        || normalized.starts_with("surrealkv://")
        || normalized.starts_with("file://")
    {
        Err("browser workshop refuses in-memory boot")
    } else {
        Err("browser workshop only opens indxdb://medousa")
    }
}

fn lock_workshop() -> Result<std::sync::MutexGuard<'static, Option<Workshop>>, String> {
    WORKSHOP
        .lock()
        .map_err(|_| "browser workshop lock was poisoned".to_string())
}

fn browser_window() -> Result<web_sys::Window, String> {
    web_sys::window().ok_or_else(|| "browser window is unavailable".to_string())
}

/// Wall clock from the page. Native `SystemTime` is not the workshop clock here.
pub fn browser_now_ms() -> u64 {
    js_sys::Date::now() as u64
}

/// Interval from the page timer, not a tokio runtime reactor.
pub async fn browser_sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let Ok(window) = browser_window() else {
            let _ = resolve.call0(&JsValue::NULL);
            return;
        };
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms.max(0));
    });
    let _ = JsFuture::from(promise).await;
}

fn sql_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn installation_id() -> Result<medousa_types::secrets::InstallationId, String> {
    let storage = browser_window()?
        .local_storage()
        .map_err(|_| "localStorage is unavailable".to_string())?
        .ok_or_else(|| "localStorage is unavailable".to_string())?;
    if let Some(existing) = storage
        .get_item(INSTALLATION_KEY)
        .map_err(|_| "localStorage read failed".to_string())?
    {
        if let Ok(parsed) = medousa_types::secrets::InstallationId::parse(&existing) {
            return Ok(parsed);
        }
    }
    let created = uuid::Uuid::new_v4().to_string();
    storage
        .set_item(INSTALLATION_KEY, &created)
        .map_err(|_| "localStorage write failed".to_string())?;
    medousa_types::secrets::InstallationId::parse(&created).map_err(|err| err.to_string())
}

fn surreal_db(runtime: &RuntimeComposition) -> Result<Surreal<Any>, String> {
    match runtime {
        RuntimeComposition::Surreal(runtime) => Ok(runtime.job_store.db()),
        RuntimeComposition::InMemory(_) => Err(reject_memory_boot(true).unwrap_err().to_string()),
    }
}

async fn apply_statements(db: &Surreal<Any>, statements: &[&str]) -> Result<(), String> {
    for statement in statements {
        let result = match db.query(*statement).await {
            Ok(response) => response.check().map(|_| ()),
            Err(err) => Err(err),
        };
        if let Err(err) = result {
            let text = err.to_string();
            if text.contains("already exists") || text.contains("already defined") {
                continue;
            }
            return Err(format!("workshop schema `{statement}`: {text}"));
        }
    }
    Ok(())
}

fn json_u64(value: &Value) -> u64 {
    value
        .as_u64()
        .or_else(|| value.as_i64().map(|n| n.max(0) as u64))
        .or_else(|| value.as_f64().map(|n| n.max(0.0) as u64))
        .unwrap_or(0)
}

async fn query_checked(db: &Surreal<Any>, statement: &str) -> Result<(), String> {
    let response = db.query(statement).await.map_err(|err| err.to_string())?;
    response.check().map(|_| ()).map_err(|err| err.to_string())
}

async fn query_rows(
    db: &Surreal<Any>,
    statement: &str,
) -> Result<surrealdb::IndexedResults, String> {
    let response = db.query(statement).await.map_err(|err| err.to_string())?;
    response.check().map_err(|err| err.to_string())
}

/// Keep a current-thread Tokio runtime entered for the page.
///
/// Surreal's IndexedDB retry path calls `tokio::time::timeout`. That panics
/// with "there is no reactor running" unless a runtime context is entered.
/// The guard is leaked so later polls of boot, sessions, and portal dials
/// still see it. The browser event loop drives the futures; this does not
/// `block_on`.
pub fn install_browser_runtime() {
    use std::sync::OnceLock;
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("browser tokio runtime")
    });
    static ENTERED: OnceLock<()> = OnceLock::new();
    ENTERED.get_or_init(|| {
        let guard = runtime.enter();
        std::mem::forget(guard);
    });
}

/// Open `indxdb://medousa`, apply the shared Stasis schema, then workshop tables.
pub async fn boot() -> Result<(), String> {
    install_browser_runtime();
    if lock_workshop()?.is_some() {
        return Ok(());
    }
    accept_workshop_endpoint(WORKSHOP_ENDPOINT)?;
    let installation = installation_id()?;
    workshop_authority::initialize(&installation).map_err(|err| err)?;

    let db = Surreal::<Any>::init();
    db.connect(WORKSHOP_ENDPOINT)
        .await
        .map_err(|err| format!("connect {WORKSHOP_ENDPOINT}: {err}"))?;
    db.use_ns(WORKSHOP_NS)
        .use_db(WORKSHOP_DB)
        .await
        .map_err(|err| format!("select {WORKSHOP_NS}/{WORKSHOP_DB}: {err}"))?;
    SurrealIdentityMemoryStore::ensure_schema_for_db(&db)
        .await
        .map_err(|err| format!("identity schema: {err}"))?;
    let runtime = RuntimeFactory::from_db(db);
    reject_memory_boot(matches!(runtime, RuntimeComposition::InMemory(_)))
        .map_err(str::to_string)?;
    ensure_stasis_runtime_schema(&runtime)
        .await
        .map_err(|err| format!("stasis schema: {err}"))?;
    let db = surreal_db(&runtime)?;
    apply_statements(&db, WORKSHOP_TABLES).await?;
    let locus =
        LocusMemoryStore::from_surreal_endpoint(WORKSHOP_ENDPOINT, WORKSHOP_NS, WORKSHOP_DB)
            .await
            .map_err(|err| format!("locus memory: {err}"))?;
    // Yield once so the page can paint while IndexedDB finishes its first commit.
    browser_sleep_ms(0).await;
    *lock_workshop()? = Some(Workshop {
        runtime,
        _locus: locus,
    });
    Ok(())
}

fn with_db<T>(body: impl FnOnce(&Surreal<Any>) -> Result<T, String>) -> Result<T, String> {
    let guard = lock_workshop()?;
    let workshop = guard
        .as_ref()
        .ok_or_else(|| "browser workshop is not booted".to_string())?;
    let db = surreal_db(&workshop.runtime)?;
    body(&db)
}

pub async fn list_sessions() -> Result<Vec<Value>, String> {
    let db = with_db(|db| Ok(db.clone()))?;
    let mut response = query_rows(
        &db,
        "SELECT session_id, title, updated_ms, preview FROM workshop_session ORDER BY updated_ms DESC",
    )
    .await?;
    let records: Vec<Value> = response.take(0).map_err(|err| err.to_string())?;
    let rows = records
        .into_iter()
        .map(|row| {
            json!({
                "session_id": row.get("session_id").and_then(Value::as_str).unwrap_or_default(),
                "title": row.get("title").and_then(Value::as_str).unwrap_or_default(),
                "updated_ms": row.get("updated_ms").map(json_u64).unwrap_or(0),
                "preview": row.get("preview").and_then(Value::as_str).unwrap_or_default(),
            })
        })
        .collect();
    Ok(rows)
}

pub async fn create_session(title: &str) -> Result<Value, String> {
    let db = with_db(|db| Ok(db.clone()))?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let title = if title.trim().is_empty() {
        "New chat"
    } else {
        title.trim()
    };
    let now = browser_now_ms();
    let statement = format!(
        "CREATE workshop_session SET session_id = {}, title = {}, preview = '', updated_ms = {now}",
        sql_quote(&session_id),
        sql_quote(title),
    );
    query_checked(&db, &statement).await?;
    let authority_id = workshop_authority::current()
        .map_err(|err| err)?
        .as_str()
        .to_string();
    Ok(json!({
        "session_id": session_id,
        "title": title,
        "authority_id": authority_id,
        "updated_ms": now,
    }))
}

pub async fn list_notes() -> Result<Vec<Value>, String> {
    let db = with_db(|db| Ok(db.clone()))?;
    let mut response = query_rows(
        &db,
        "SELECT note_id, title, body, updated_ms FROM workshop_note ORDER BY updated_ms DESC",
    )
    .await?;
    let records: Vec<Value> = response.take(0).map_err(|err| err.to_string())?;
    let rows = records
        .into_iter()
        .map(|row| {
            json!({
                "note_id": row.get("note_id").and_then(Value::as_str).unwrap_or_default(),
                "title": row.get("title").and_then(Value::as_str).unwrap_or_default(),
                "body": row.get("body").and_then(Value::as_str).unwrap_or_default(),
                "updated_ms": row.get("updated_ms").map(json_u64).unwrap_or(0),
            })
        })
        .collect();
    Ok(rows)
}

pub async fn save_note(note_id: &str, title: &str, body: &str) -> Result<Value, String> {
    let db = with_db(|db| Ok(db.clone()))?;
    let note_id = if note_id.trim().is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        note_id.trim().to_string()
    };
    let now = browser_now_ms();
    let statement = format!(
        "DELETE workshop_note WHERE note_id = {id}; CREATE workshop_note SET note_id = {id}, title = {title}, body = {body}, updated_ms = {now}",
        id = sql_quote(&note_id),
        title = sql_quote(title),
        body = sql_quote(body),
        now = now,
    );
    query_checked(&db, &statement).await?;
    Ok(json!({
        "note_id": note_id,
        "title": title,
        "body": body,
        "updated_ms": now,
    }))
}

pub fn configure_inference(base_url: &str, api_key: &str, model: &str) {
    if let Ok(mut config) = INFERENCE.lock() {
        config.base_url = base_url.trim().trim_end_matches('/').to_string();
        config.api_key = api_key.trim().to_string();
        config.model = if model.trim().is_empty() {
            "gpt-4o-mini".to_string()
        } else {
            model.trim().to_string()
        };
    }
}

fn inference_snapshot() -> Result<InferenceConfig, String> {
    let config = INFERENCE
        .lock()
        .map_err(|_| "inference lock was poisoned".to_string())?;
    if config.base_url.is_empty() {
        return Err("browser inference is not configured".to_string());
    }
    Ok(InferenceConfig {
        base_url: config.base_url.clone(),
        api_key: config.api_key.clone(),
        model: config.model.clone(),
    })
}

async fn fetch_completion(config: &InferenceConfig, prompt: &str) -> Result<String, String> {
    let window = browser_window()?;
    let url = format!("{}/chat/completions", config.base_url);
    let payload = json!({
        "model": config.model,
        "stream": true,
        "messages": [{"role": "user", "content": prompt}],
    });
    let headers = web_sys::Headers::new().map_err(|_| "failed to build request headers")?;
    headers
        .set("content-type", "application/json")
        .map_err(|_| "failed to set content-type")?;
    if !config.api_key.is_empty() {
        headers
            .set("authorization", &format!("Bearer {}", config.api_key))
            .map_err(|_| "failed to set authorization")?;
    }
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_mode(web_sys::RequestMode::Cors);
    init.set_headers(&headers);
    init.set_body(&JsValue::from_str(&payload.to_string()));
    let request = web_sys::Request::new_with_str_and_init(&url, &init)
        .map_err(|_| format!("invalid inference URL {url}"))?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| format!("inference request to {url} failed"))?;
    let response: web_sys::Response = response
        .dyn_into()
        .map_err(|_| "inference response was not a Response")?;
    if !response.ok() {
        return Err(format!("inference HTTP {}", response.status()));
    }
    let text = JsFuture::from(
        response
            .text()
            .map_err(|_| "inference body was unreadable")?,
    )
    .await
    .map_err(|_| "inference body read failed")?;
    let text = text.as_string().unwrap_or_default();
    Ok(assemble_completion(&text))
}

fn assemble_completion(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.starts_with('{') {
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            if let Some(text) = value
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
            {
                return text.to_string();
            }
        }
    }
    let mut assembled = String::new();
    for line in body.lines() {
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if let Some(text) = value
            .pointer("/choices/0/delta/content")
            .and_then(Value::as_str)
        {
            assembled.push_str(text);
        }
    }
    if assembled.is_empty() {
        trimmed.to_string()
    } else {
        assembled
    }
}

#[derive(Serialize)]
struct TurnEvent<'a> {
    kind: &'a str,
    turn_id: &'a str,
    text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
}

fn emit(on_event: &impl Fn(&str), event: &TurnEvent<'_>) {
    if let Ok(encoded) = serde_json::to_string(event) {
        on_event(&encoded);
    }
}

/// Start a turn and stream model text through `on_event`.
pub async fn start_turn(
    session_id: &str,
    prompt: &str,
    on_event: impl Fn(&str),
) -> Result<String, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session id is required".to_string());
    }
    let turn_id = uuid::Uuid::new_v4().to_string();
    let now = browser_now_ms();
    let db = with_db(|db| Ok(db.clone()))?;
    let user_statement = format!(
        "CREATE workshop_turn SET turn_id = {turn}, session_id = {session}, role = 'user', body = {body}, created_ms = {now}",
        turn = sql_quote(&turn_id),
        session = sql_quote(session_id),
        body = sql_quote(prompt),
        now = now,
    );
    query_checked(&db, &user_statement).await?;
    emit(
        &on_event,
        &TurnEvent {
            kind: "accepted",
            turn_id: &turn_id,
            text: "",
            message: None,
        },
    );
    let config = inference_snapshot()?;
    let completion = match fetch_completion(&config, prompt).await {
        Ok(text) => text,
        Err(err) => {
            emit(
                &on_event,
                &TurnEvent {
                    kind: "error",
                    turn_id: &turn_id,
                    text: "",
                    message: Some(&err),
                },
            );
            return Err(err);
        }
    };
    let mut sent = 0;
    while sent < completion.len() {
        let next = completion
            .char_indices()
            .map(|(index, _)| index)
            .find(|index| *index >= sent + 48)
            .unwrap_or(completion.len());
        let chunk = &completion[sent..next];
        emit(
            &on_event,
            &TurnEvent {
                kind: "delta",
                turn_id: &turn_id,
                text: chunk,
                message: None,
            },
        );
        sent = next;
        browser_sleep_ms(0).await;
    }
    let assistant_statement = format!(
        "CREATE workshop_turn SET turn_id = {turn}, session_id = {session}, role = 'assistant', body = {body}, created_ms = {now}",
        turn = sql_quote(&turn_id),
        session = sql_quote(session_id),
        body = sql_quote(&completion),
        now = browser_now_ms(),
    );
    query_checked(&db, &assistant_statement).await?;
    let preview: String = completion.chars().take(180).collect();
    let touch = format!(
        "UPDATE workshop_session SET preview = {preview}, updated_ms = {now} WHERE session_id = {session}",
        preview = sql_quote(&preview),
        now = browser_now_ms(),
        session = sql_quote(session_id),
    );
    query_checked(&db, &touch).await?;
    emit(
        &on_event,
        &TurnEvent {
            kind: "done",
            turn_id: &turn_id,
            text: &completion,
            message: None,
        },
    );
    Ok(turn_id)
}

/// Compile and run a Grapheme script on the in-page engine. Wasmer is not linked.
pub fn run_grapheme(source: &str) -> String {
    let response = execute(&ExecuteRequest {
        source: Some(source.to_string()),
        artifact: None,
        initial_current: None,
        args: None,
        entrypoint: None,
    });
    serde_json::to_string(&response).unwrap_or_else(|_| {
        json!({"ok": false, "error": {"code": "ENCODE", "message": "response encode failed"}})
            .to_string()
    })
}

/// Dial a paired daemon ticket over the Iroh relay (`medousa-http/1`).
pub async fn dial_iroh_ticket(ticket: &str, path: &str) -> Result<String, String> {
    let path = if path.trim().is_empty() { "/" } else { path };
    medousa_iroh_http::iroh_http_get_text(ticket, path)
        .await
        .map_err(|err| err.to_string())
}

async fn vault_dir() -> Result<web_sys::FileSystemDirectoryHandle, String> {
    let window = browser_window()?;
    let root = JsFuture::from(window.navigator().storage().get_directory())
        .await
        .map_err(|_| "OPFS root is unavailable".to_string())?;
    let root: web_sys::FileSystemDirectoryHandle = root
        .dyn_into()
        .map_err(|_| "OPFS root handle was invalid".to_string())?;
    let options = web_sys::FileSystemGetDirectoryOptions::new();
    options.set_create(true);
    let vault = JsFuture::from(root.get_directory_handle_with_options("vault", &options))
        .await
        .map_err(|_| "OPFS vault directory is unavailable".to_string())?;
    vault
        .dyn_into()
        .map_err(|_| "OPFS vault handle was invalid".to_string())
}

pub async fn read_vault(path: &str) -> Result<String, String> {
    let vault = vault_dir().await?;
    let file = JsFuture::from(vault.get_file_handle(path))
        .await
        .map_err(|_| format!("OPFS file {path} is missing"))?;
    let file: web_sys::FileSystemFileHandle = file
        .dyn_into()
        .map_err(|_| format!("OPFS file {path} handle was invalid"))?;
    let blob = JsFuture::from(file.get_file())
        .await
        .map_err(|_| format!("OPFS read {path} failed"))?;
    let blob: web_sys::File = blob
        .dyn_into()
        .map_err(|_| format!("OPFS read {path} was not a file"))?;
    let text = JsFuture::from(blob.text())
        .await
        .map_err(|_| format!("OPFS decode {path} failed"))?;
    text.as_string()
        .ok_or_else(|| format!("OPFS decode {path} failed"))
}

pub async fn write_vault(path: &str, body: &str) -> Result<(), String> {
    let vault = vault_dir().await?;
    let options = web_sys::FileSystemGetFileOptions::new();
    options.set_create(true);
    let file = JsFuture::from(vault.get_file_handle_with_options(path, &options))
        .await
        .map_err(|_| format!("OPFS file {path} could not be created"))?;
    let file: web_sys::FileSystemFileHandle = file
        .dyn_into()
        .map_err(|_| format!("OPFS file {path} handle was invalid"))?;
    let writable = JsFuture::from(file.create_writable())
        .await
        .map_err(|_| format!("OPFS writer {path} failed"))?;
    let writable: web_sys::FileSystemWritableFileStream = writable
        .dyn_into()
        .map_err(|_| format!("OPFS writer {path} was invalid"))?;
    JsFuture::from(
        writable
            .write_with_str(body)
            .map_err(|_| format!("OPFS write {path} failed"))?,
    )
    .await
    .map_err(|_| format!("OPFS write {path} failed"))?;
    JsFuture::from(writable.close())
        .await
        .map_err(|_| format!("OPFS close {path} failed"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{accept_workshop_endpoint, reject_memory_boot, WORKSHOP_ENDPOINT};

    #[test]
    fn in_memory_boot_is_a_hard_error() {
        assert_eq!(
            reject_memory_boot(true),
            Err("browser workshop refuses in-memory boot")
        );
        assert!(reject_memory_boot(false).is_ok());
    }

    #[test]
    fn only_indexeddb_workshop_endpoint_is_accepted() {
        assert!(accept_workshop_endpoint(WORKSHOP_ENDPOINT).is_ok());
        assert!(accept_workshop_endpoint("mem://").is_err());
        assert!(accept_workshop_endpoint("surrealkv://runtime.surrealkv").is_err());
        assert!(accept_workshop_endpoint("ws://127.0.0.1:8000").is_err());
    }
}
