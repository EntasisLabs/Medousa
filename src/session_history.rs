pub use medousa_types::session::{ConversationTurn, SessionHistorySummary};

pub fn load_history(session_id: &str) -> Vec<ConversationTurn> {
    let Ok(session_id) = crate::session_storage::SessionId::parse(session_id) else {
        return Vec::new();
    };
    crate::session_store::get_session_store().load_history(&session_id)
}

pub fn list_sessions(limit: usize) -> Vec<SessionHistorySummary> {
    #[cfg(feature = "full-daemon")]
    {
        crate::session_catalog::list_sessions(limit)
    }
    #[cfg(not(feature = "full-daemon"))]
    {
        crate::session_store::build_backfill_summaries(limit)
    }
}

pub fn session_visible_to_profile(session_id: &str, profile_id: &str) -> bool {
    #[cfg(feature = "full-daemon")]
    {
        crate::session_catalog::session_visible_to_profile(session_id, profile_id)
    }
    #[cfg(not(feature = "full-daemon"))]
    {
        let _ = profile_id;
        list_sessions(usize::MAX)
            .iter()
            .any(|summary| summary.session_id == session_id)
    }
}

pub fn list_sessions_for_profile(profile_id: &str, limit: usize) -> Vec<SessionHistorySummary> {
    #[cfg(feature = "full-daemon")]
    {
        crate::session::list_history_sessions_page_for_profile(
            Some(profile_id),
            limit,
            None,
            None,
        )
        .sessions
    }
    #[cfg(not(feature = "full-daemon"))]
    {
        let _ = profile_id;
        list_sessions(limit)
    }
}

pub fn display_name(session_id: &str) -> Option<String> {
    #[cfg(feature = "full-daemon")]
    {
        crate::session_catalog::get_summary(session_id)
            .and_then(|summary| summary.display_name)
            .or_else(|| {
                crate::shared_session_catalog::get_shared_row(session_id)
                    .and_then(|row| row.display_name)
            })
    }
    #[cfg(not(feature = "full-daemon"))]
    {
        list_sessions(usize::MAX)
            .into_iter()
            .find(|summary| summary.session_id == session_id)
            .and_then(|summary| summary.display_name)
    }
}
