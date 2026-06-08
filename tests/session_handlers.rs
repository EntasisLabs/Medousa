use std::env;
use std::path::PathBuf;

use axum::extract::{Path as AxumPath, Query};
use axum::Json;
use chrono::Utc;

use medousa::daemon_handlers::{append_session_turn, get_session_history, list_session_history};
use medousa::daemon_api::{SessionAppendTurnRequest, SessionHistoryListRequest};
use medousa::session::ConversationTurn;

fn make_temp_data_dir() -> PathBuf {
    let base = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    base.join(format!("medousa_test_{}", nanos))
}

#[tokio::test]
async fn session_append_and_get_via_handlers() {
    let tmp = make_temp_data_dir();
    unsafe { std::env::set_var("XDG_DATA_HOME", &tmp) };

    let session_id = "test-session-handlers".to_string();

    let turn = ConversationTurn::plain(
        "user",
        "hello from test".to_string(),
        Utc::now(),
        vec![],
        None,
    );

    let req = SessionAppendTurnRequest { turn: turn.clone() };

    // call append handler
    let append_res = append_session_turn(AxumPath(session_id.clone()), Json(req)).await;
    assert!(append_res.is_ok());
    let Json(body) = append_res.unwrap();
    assert_eq!(body.session_id, session_id);
    assert!(body.stored);

    // call get handler
    let get_res = get_session_history(AxumPath(session_id.clone())).await;
    assert!(get_res.is_ok());
    let Json(history) = get_res.unwrap();
    assert_eq!(history.session_id, session_id);
    assert!(!history.turns.is_empty());
    assert_eq!(history.turns.last().unwrap().content, "hello from test");
}

#[tokio::test]
async fn list_history_sessions_handler() {
    let tmp = make_temp_data_dir();
    unsafe { std::env::set_var("XDG_DATA_HOME", &tmp) };

    // create two sessions
    for i in 0..2 {
        let sid = format!("session-{}", i);
        let turn = ConversationTurn::plain(
            "user",
            format!("content {}", i),
            Utc::now(),
            vec![],
            None,
        );
        medousa::session::append_turn(&sid, &turn);
    }

    let query = Query(SessionHistoryListRequest { limit: Some(10) });
    let res = list_session_history(query).await;
    assert!(res.is_ok());
    let Json(list) = res.unwrap();
    assert!(list.sessions.len() >= 2);
}
