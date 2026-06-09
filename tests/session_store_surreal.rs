//! Surreal in-memory integration tests for `SurrealSessionStore`.
//!
//! Validates session_turn CRUD and the GROUP BY list query (`time::max`, not `math::max`).

use chrono::{Duration, TimeZone, Utc};
use medousa::session::ConversationTurn;
use medousa::session_store::{SessionStore, SurrealSessionStore};
use medousa::turn_parts::user_conversation_turn;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;

const SESSION_TURN_TABLE: &str = "session_turn";

async fn setup_store() -> SurrealSessionStore {
    let db = Surreal::<Any>::init();
    db.connect("mem://")
        .await
        .expect("mem:// connect should succeed");
    db.use_ns("test")
        .use_db("test")
        .await
        .expect("use_ns/use_db should succeed");
    SurrealSessionStore::ensure_schema_for_db(&db)
        .await
        .expect("session_turn schema should apply");
    SurrealSessionStore::new(db)
}

fn user_turn(content: &str, at: chrono::DateTime<Utc>) -> ConversationTurn {
    ConversationTurn::plain("user", content.to_string(), at, vec![], None)
}

#[tokio::test(flavor = "multi_thread")]
async fn surreal_session_store_append_and_load_history() {
    let store = setup_store().await;
    let base = Utc.with_ymd_and_hms(2026, 6, 8, 12, 0, 0).unwrap();

    store.append_turn("medousa-home", &user_turn("first", base));
    store.append_turn(
        "medousa-home",
        &user_turn("second", base + Duration::minutes(1)),
    );

    let turns = store.load_history("medousa-home");
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].content, "first");
    assert_eq!(turns[1].content, "second");
}

#[tokio::test(flavor = "multi_thread")]
async fn surreal_session_store_persists_turns_with_parts_timeline() {
    let store = setup_store().await;
    store.append_turn(
        "medousa-home-parts",
        &user_conversation_turn("hello with structured parts"),
    );

    let turns = store.load_history("medousa-home-parts");
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].content, "hello with structured parts");
    assert!(
        turns[0].parts.as_ref().is_some_and(|parts| !parts.is_empty()),
        "user turns should round-trip timeline parts"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn surreal_list_history_sessions_includes_named_workshop_sessions() {
    let store = setup_store().await;
    let base = Utc.with_ymd_and_hms(2026, 6, 8, 15, 0, 0).unwrap();

    store.append_turn(
        "medousa-home",
        &user_turn("workshop default", base),
    );
    store.append_turn(
        "medousa-home-30ddc8bf-e469-40f0-8b5d-ca1c0397c8a4",
        &user_turn("ios new chat", base + Duration::minutes(5)),
    );
    store.append_turn(
        "2e326df0bb3f42219f51aa4d776efe2c",
        &user_turn("tui uuid session", base + Duration::minutes(10)),
    );

    let sessions = store.list_history_sessions(50);
    let ids: Vec<_> = sessions.iter().map(|s| s.session_id.as_str()).collect();

    assert!(ids.contains(&"medousa-home"));
    assert!(ids.contains(&"medousa-home-30ddc8bf-e469-40f0-8b5d-ca1c0397c8a4"));
    assert!(ids.contains(&"2e326df0bb3f42219f51aa4d776efe2c"));
    assert_eq!(sessions.len(), 3);

    for summary in &sessions {
        assert!(
            summary.last_timestamp.is_some(),
            "session {} should have a valid last_timestamp",
            summary.session_id
        );
        assert!(summary.turns >= 1);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn surreal_list_history_sessions_ordered_by_recency() {
    let store = setup_store().await;
    let base = Utc.with_ymd_and_hms(2026, 6, 8, 10, 0, 0).unwrap();

    store.append_turn("older-session", &user_turn("old", base));
    store.append_turn(
        "newer-session",
        &user_turn("new", base + Duration::hours(2)),
    );

    let sessions = store.list_history_sessions(10);
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].session_id, "newer-session");
    assert_eq!(sessions[1].session_id, "older-session");
}

#[tokio::test(flavor = "multi_thread")]
async fn surreal_list_history_sessions_respects_limit() {
    let store = setup_store().await;
    let base = Utc.with_ymd_and_hms(2026, 6, 8, 8, 0, 0).unwrap();

    for i in 0..5 {
        store.append_turn(
            &format!("session-{i}"),
            &user_turn(
                &format!("turn {i}"),
                base + Duration::minutes(i as i64),
            ),
        );
    }

    let sessions = store.list_history_sessions(3);
    assert_eq!(sessions.len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn surreal_group_by_uses_time_max_not_math_max() {
    let db = Surreal::<Any>::init();
    db.connect("mem://").await.unwrap();
    db.use_ns("test").use_db("test").await.unwrap();
    SurrealSessionStore::ensure_schema_for_db(&db)
        .await
        .unwrap();

    let base = Utc.with_ymd_and_hms(2026, 6, 8, 14, 0, 0).unwrap();
    let store = SurrealSessionStore::new(db.clone());
    store.append_turn("medousa-home", &user_turn("probe", base));

    #[derive(serde::Deserialize, SurrealValue)]
    struct Agg {
        session_id: String,
        turns: usize,
        last_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    }

    let math_sql = "SELECT session_id, count() AS turns, math::max(timestamp) AS last_timestamp \
                    FROM type::table($table) GROUP BY session_id";
    let mut math_resp = db
        .query(math_sql)
        .bind(("table", SESSION_TURN_TABLE))
        .await
        .expect("math::max query should run");
    let math_rows: Result<Vec<Agg>, _> = math_resp.take(0);
    assert!(
        math_rows.is_err(),
        "math::max(timestamp) on GROUP BY should not deserialize to DateTime"
    );

    let time_sql = "SELECT session_id, count() AS turns, time::max(timestamp) AS last_timestamp \
                    FROM type::table($table) GROUP BY session_id ORDER BY last_timestamp DESC";
    let mut time_resp = db
        .query(time_sql)
        .bind(("table", SESSION_TURN_TABLE))
        .await
        .expect("time::max query should run");
    let time_rows: Vec<Agg> = time_resp.take(0).expect("time::max rows should deserialize");
    assert_eq!(time_rows.len(), 1);
    assert_eq!(time_rows[0].session_id, "medousa-home");
    assert_eq!(time_rows[0].turns, 1);
    assert_eq!(time_rows[0].last_timestamp, Some(base));
}
