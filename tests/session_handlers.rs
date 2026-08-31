use std::env;
use std::path::PathBuf;

use axum::Json;
use axum::extract::{Extension, Path as AxumPath, Query};
use chrono::Utc;

use medousa::daemon_api::{
    SessionAppendTurnRequest, SessionHistoryListRequest, SessionHistoryRequest,
};
use medousa::daemon_handlers::{append_session_turn, get_session_history, list_session_history};
use medousa::request_principal::{RequestPrincipal, TransportClass};
use medousa::session::ConversationTurn;

fn test_principal() -> Extension<RequestPrincipal> {
    Extension(RequestPrincipal::anonymous(TransportClass::Loopback))
}

fn make_temp_data_dir() -> PathBuf {
    let base = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    base.join(format!("medousa_test_{}", nanos))
}

#[tokio::test(flavor = "multi_thread")]
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
    let get_res = get_session_history(
        AxumPath(session_id.clone()),
        Query(SessionHistoryRequest {
            limit: None,
            cursor: None,
        }),
    )
    .await;
    assert!(get_res.is_ok());
    let Json(history) = get_res.unwrap();
    assert_eq!(history.session_id, session_id);
    assert!(!history.turns.is_empty());
    assert_eq!(
        history.turns.last().unwrap().turn.content,
        "hello from test"
    );
    assert!(history.next_cursor.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn session_history_pages_walk_backward_via_handler() {
    let tmp = make_temp_data_dir();
    unsafe { std::env::set_var("XDG_DATA_HOME", &tmp) };

    let session_id = "test-session-handler-pages".to_string();
    for index in 1..=5 {
        let turn =
            ConversationTurn::plain("user", format!("turn {index}"), Utc::now(), vec![], None);
        medousa::session::append_turn(&session_id, &turn)
            .await
            .unwrap();
    }

    let Json(latest) = get_session_history(
        AxumPath(session_id.clone()),
        Query(SessionHistoryRequest {
            limit: Some(2),
            cursor: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        latest
            .turns
            .iter()
            .map(|turn| turn.entry_seq)
            .collect::<Vec<_>>(),
        vec![4, 5]
    );
    assert_eq!(latest.next_cursor.as_deref(), Some("4"));

    let Json(older) = get_session_history(
        AxumPath(session_id),
        Query(SessionHistoryRequest {
            limit: Some(2),
            cursor: latest.next_cursor,
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        older
            .turns
            .iter()
            .map(|turn| turn.entry_seq)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(older.next_cursor.as_deref(), Some("2"));
}

#[tokio::test(flavor = "multi_thread")]
async fn list_history_sessions_handler() {
    let tmp = make_temp_data_dir();
    unsafe { std::env::set_var("XDG_DATA_HOME", &tmp) };

    // create two sessions
    for i in 0..2 {
        let sid = format!("session-{}", i);
        let turn =
            ConversationTurn::plain("user", format!("content {}", i), Utc::now(), vec![], None);
        medousa::session::append_turn(&sid, &turn).await.unwrap();
    }

    let query = Query(SessionHistoryListRequest {
        limit: Some(10),
        include_verification: None,
        q: None,
        cursor: None,
    });
    let res = list_session_history(test_principal(), query).await;
    assert!(res.is_ok());
    let Json(list) = res.unwrap();
    assert!(list.sessions.len() >= 2);

    let slim_query = Query(SessionHistoryListRequest {
        limit: Some(10),
        include_verification: Some(false),
        q: None,
        cursor: None,
    });
    let slim_res = list_session_history(test_principal(), slim_query).await;
    assert!(slim_res.is_ok());
    let Json(slim_list) = slim_res.unwrap();
    assert!(slim_list.sessions.iter().all(|session| {
        session.verification_runs == 0
            && session.last_verification_timestamp.is_none()
            && session.last_verification_confidence.is_none()
            && session.last_verification_coverage.is_none()
            && session.last_verification_verified.is_none()
    }));
}

#[tokio::test(flavor = "multi_thread")]
async fn list_history_sessions_search_via_handlers() {
    let tmp = make_temp_data_dir();
    unsafe { std::env::set_var("XDG_DATA_HOME", &tmp) };

    medousa::session::set_session_display_name("alpha-session", "Budget planning").unwrap();
    medousa::session::set_session_display_name("beta-session", "Morning brief").unwrap();

    let turn = ConversationTurn::plain("user", "hello".to_string(), Utc::now(), vec![], None);
    medousa::session::append_turn("alpha-session", &turn)
        .await
        .unwrap();
    medousa::session::append_turn("beta-session", &turn)
        .await
        .unwrap();

    let search = Query(SessionHistoryListRequest {
        limit: Some(10),
        include_verification: None,
        q: Some("budget".to_string()),
        cursor: None,
    });
    let res = list_session_history(test_principal(), search).await;
    assert!(res.is_ok());
    let Json(list) = res.unwrap();
    assert_eq!(list.sessions.len(), 1);
    assert_eq!(list.sessions[0].session_id, "alpha-session");
}
