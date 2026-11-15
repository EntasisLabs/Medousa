//! Read-only worker discovery; no UI state and no chat admission.
use medousa_types::{WorkCardDetail, WorkCardKind};

pub(super) async fn fetch(
    client: &medousa_sdk::MedousaClient,
    session: &str,
    known: &[(String, String)],
) -> Result<Vec<WorkCardDetail>, String> {
    use futures_util::{StreamExt, stream};
    let page = client
        .workspace()
        .list_cards(&medousa_types::WorkspaceCardsQuery {
            session_id: Some(session.into()),
            include_terminal: Some(true),
            limit: Some(100),
            ..Default::default()
        })
        .await
        .map_err(|_| {
            "Collaboration activity could not be refreshed. Recorded activity is still available."
                .to_string()
        })?;
    let changed = page.cards.into_iter().filter(|card| {
        !known
            .iter()
            .any(|(id, stamp)| id == &card.id.0 && stamp == &card.updated_at_utc.to_rfc3339())
    });
    let results = stream::iter(
        changed.map(|card| async move { client.workspace().get_card(&card.id.0).await }),
    )
    .buffer_unordered(4)
    .collect::<Vec<_>>()
    .await;
    let mut rows = vec![];
    for result in results {
        let row = result.map_err(|_| {
            "Some collaboration activity could not be loaded. Retry to reconcile.".to_string()
        })?;
        if row.kind == WorkCardKind::TurnWorker && row.session_id.as_deref() == Some(session) {
            if let Some(full) = &row.worker_transcript {
                if full.work_id != row.work_id.as_deref().unwrap_or(&row.card.id.0)
                    || full.session_id != session
                {
                    return Err(
                        "Worker transcript identity did not match its parent conversation.".into(),
                    );
                }
            }
            rows.push(row);
        }
    }
    rows.sort_by(|a, b| {
        a.card
            .created_at_utc
            .cmp(&b.card.created_at_utc)
            .then(a.card.id.0.cmp(&b.card.id.0))
    });
    Ok(rows)
}
