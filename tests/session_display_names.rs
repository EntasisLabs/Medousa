use std::env;
use std::path::PathBuf;

use axum::extract::Path as AxumPath;
use axum::Json;

use medousa::daemon_handlers::{list_session_history, set_session_display_name};
use medousa::daemon_api::{
    SessionHistoryListRequest, SessionSetDisplayNameRequest,
};

fn make_temp_data_dir() -> PathBuf {
    let base = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    base.join(format!("medousa_session_name_test_{}", nanos))
}

#[tokio::test(flavor = "multi_thread")]
async fn set_and_list_session_display_name_via_handlers() {
    let tmp = make_temp_data_dir();
    unsafe { env::set_var("XDG_DATA_HOME", &tmp) };

    let session_id = "named-session-test".to_string();
    let request = SessionSetDisplayNameRequest {
        display_name: "Research Sprint".to_string(),
    };

    let set_res =
        set_session_display_name(AxumPath(session_id.clone()), Json(request)).await;
    assert!(set_res.is_ok());
    let Json(body) = set_res.unwrap();
    assert_eq!(body.session_id, session_id);
    assert_eq!(body.display_name, "Research Sprint");

    assert_eq!(
        medousa::session::get_session_display_name(&session_id).as_deref(),
        Some("Research Sprint")
    );

    let list_res = list_session_history(axum::extract::Query(SessionHistoryListRequest {
        limit: Some(50),
        include_verification: None,
    }))
    .await;
    assert!(list_res.is_ok());
    let Json(list) = list_res.unwrap();
    let named = list
        .sessions
        .iter()
        .find(|item| item.session_id == session_id);
    assert!(named.is_some());
    assert_eq!(
        named.unwrap().display_name.as_deref(),
        Some("Research Sprint")
    );

    assert_eq!(
        medousa::session::resolve_history_resume_target("Research Sprint"),
        Some(session_id)
    );
}
