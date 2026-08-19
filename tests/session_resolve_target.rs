use std::env;
use std::path::PathBuf;

use chrono::Utc;
use medousa::session::{
    ConversationTurn, append_turn, resolve_history_resume_target, session_turn_count,
};

fn make_temp_data_dir() -> PathBuf {
    let base = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    base.join(format!("medousa_session_resolve_test_{nanos}"))
}

fn user_turn(content: &str) -> ConversationTurn {
    ConversationTurn::plain("user", content.to_string(), Utc::now(), vec![], None)
}

#[tokio::test(flavor = "multi_thread")]
async fn resolve_history_resume_target_uses_catalog_prefix_without_full_list() {
    let tmp = make_temp_data_dir();
    unsafe { env::set_var("XDG_DATA_HOME", &tmp) };

    append_turn("abcdef-session-one", &user_turn("first"))
        .await
        .unwrap();
    append_turn("abcdef-session-two", &user_turn("second"))
        .await
        .unwrap();

    assert_eq!(
        resolve_history_resume_target("abcdef-session-one"),
        Some("abcdef-session-one".to_string())
    );
    assert_eq!(resolve_history_resume_target("abcdef"), None);
    assert_eq!(
        resolve_history_resume_target("abcdef-session-o"),
        Some("abcdef-session-one".to_string())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn session_turn_count_reads_catalog_not_full_history() {
    let tmp = make_temp_data_dir();
    unsafe { env::set_var("XDG_DATA_HOME", &tmp) };

    append_turn("count-session", &user_turn("one"))
        .await
        .unwrap();
    append_turn("count-session", &user_turn("two"))
        .await
        .unwrap();
    append_turn("count-session", &user_turn("three"))
        .await
        .unwrap();

    assert_eq!(session_turn_count("count-session"), 3);
}
