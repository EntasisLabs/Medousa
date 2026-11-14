//! Project the daemon's connected-conversation event ledger into the same typed
//! timeline as native chat. No second renderer or local inference state.
use medousa_app_core::ChatSnapshot;
use medousa_types::{ExternalConversationView, ExternalEventKind};
use std::hash::{Hash, Hasher};

pub fn project(view: &ExternalConversationView) -> ChatSnapshot {
    let mut revision = std::collections::hash_map::DefaultHasher::new();
    let items = view.events.iter().map(|event| {
        serde_json::to_vec(event).expect("serializable provider event").hash(&mut revision);
        let (role, content) = match event.kind {
            ExternalEventKind::UserMessage => ("user", event.text.clone()),
            ExternalEventKind::ProviderMessage => ("assistant", event.text.clone()),
            kind => ("system", format!("{kind:?}: {}", event.text)),
        };
        serde_json::json!({
            "id": format!("provider:{}:{}",view.id,event.event_id),
            "role":role,"content":content,"createdAt":event.created_at.to_rfc3339(),
            "failed":matches!(event.kind,ExternalEventKind::TransportFailed|ExternalEventKind::Failed),
            "reactions":event.reaction.clone().into_iter().collect::<Vec<_>>()
        })
    }).collect::<Vec<_>>();
    serde_json::from_value(serde_json::json!({
        "sessionId":format!("provider:{}",view.id),"authorityId":format!("provider:{:?}",view.provider),
        "revision":revision.finish(),"items":items
    })).expect("connected-conversation projection follows app-core ChatSnapshot schema")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_event_identity_and_changed_text_revision() {
        let mut view: ExternalConversationView = serde_json::from_value(serde_json::json!({
            "id":"provider-a","provider":"muse","label":"Team","target":"channel",
            "created_at":"2026-11-14T00:00:00Z","updated_at":"2026-11-14T00:00:00Z",
            "events":[{"sequence":1,"event_id":"one","request_id":null,"kind":"user_message","text":"**literal**","created_at":"2026-11-14T00:00:00Z"}]
        })).unwrap();
        let before = project(&view);
        assert_eq!(before.items[0].content, "**literal**");
        assert_eq!(before.items[0].role, medousa_app_core::ChatItemRole::User);
        view.events[0].text = "changed".into();
        let after = project(&view);
        assert_eq!(before.items[0].id, after.items[0].id);
        assert_ne!(before.revision, after.revision);
    }
}
