//! One operator-admitted native or provider handoff, recovered from work custody.
use super::*;
use medousa_types::{
    work_provider::{WorkProviderDispatch, WorkProviderDispatchSource},
    work_unit::UserDomainRef,
};
use sha2::{Digest, Sha256};

pub(super) fn target_digest(binding: &ConversationRecord) -> anyhow::Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            &binding.id,
            &binding.owner_id,
            binding.provider,
            &binding.target,
            &binding.dot_user_id,
            &binding.webhook_url,
        ))?)
    ))
}

pub(super) async fn admit(
    state: &AppState,
    binding: &ConversationRecord,
    input: SendMessageRequest,
) -> Result<Json<ConversationView>, HttpError> {
    let work = input
        .work
        .ok_or_else(|| bad_request("scheduled handoff requires work metadata"))?;
    let after_provider_completion = if let Some(source) = input.after_provider_completion {
        if work.review_of.is_some() {
            return Err(bad_request(
                "provider chains use outcome evidence; native revision review is a separate admission",
            ));
        }
        let upstream = state
            .external_conversations
            .binding(&source.conversation_id)
            .await
            .filter(|record| record.owner_id == binding.owner_id)
            .ok_or((
                StatusCode::NOT_FOUND,
                "provider predecessor conversation not found".into(),
            ))?;
        Some(WorkProviderDispatchSource {
            request: source,
            target_digest: target_digest(&upstream).map_err(internal)?,
        })
    } else {
        if work.review_of.is_none() {
            return Err(bad_request("scheduled review requires work.review_of"));
        }
        None
    };
    let host = crate::daemon::work_units::local_work_unit_host().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "work unit host unavailable".into(),
    ))?;
    host.admit_provider_dispatch(
        provider_work_domain(&binding.owner_id)?,
        WorkProviderDispatch {
            conversation_id: binding.id.clone(),
            request_id: input.request_id.clone(),
            provider: binding.provider,
            input: work,
            instructions: input.text,
            target_digest: target_digest(binding).map_err(internal)?,
            scope_digest: String::new(),
            source_request_digest: String::new(),
            coordinator_wake: input.coordinator_wake,
            after_provider_completion: after_provider_completion.clone(),
        },
    )
    .await
    .map_err(internal)?;
    let view = state
        .external_conversations
        .record(
            &binding.id,
            format!("scheduled:{}", input.request_id),
            Some(input.request_id),
            EventKind::TransportPending,
            if after_provider_completion.is_some() {
                "Provider handoff admitted; waiting for the exact predecessor completion".into()
            } else {
                "Review handoff admitted; waiting for the exact native executor completion".into()
            },
        )
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "conversation not found".into()))?;
    Ok(Json(view))
}

pub(super) async fn source_matches(state: &AppState, dispatch: &WorkProviderDispatch) -> bool {
    let Some(source) = &dispatch.after_provider_completion else {
        return true;
    };
    state
        .external_conversations
        .binding(&source.request.conversation_id)
        .await
        .is_some_and(|binding| {
            target_digest(&binding).ok().as_deref() == Some(source.target_digest.as_str())
        })
}

pub(crate) async fn resume(
    state: AppState,
    domain: UserDomainRef,
    dispatch: WorkProviderDispatch,
) -> anyhow::Result<()> {
    let host = crate::daemon::work_units::local_work_unit_host()
        .ok_or_else(|| anyhow::anyhow!("work unit host unavailable"))?;
    let destination = state
        .external_conversations
        .binding(&dispatch.conversation_id)
        .await;
    if destination.as_ref().is_none_or(|binding| {
        target_digest(binding).ok().as_deref() != Some(dispatch.target_digest.as_str())
    }) || !source_matches(&state, &dispatch).await
    {
        host.close_provider_dispatch(
            domain,
            dispatch,
            "provider handoff source or destination changed or was removed".into(),
        )
        .await?;
        return Ok(());
    }
    if !host
        .provider_dispatch_ready(domain.clone(), dispatch.clone())
        .await?
    {
        return Ok(());
    }
    let _view = send_admitted(
        state,
        RequestPrincipal::worker(domain.user_id),
        dispatch.conversation_id,
        SendMessageRequest {
            request_id: dispatch.request_id,
            text: dispatch.instructions,
            work: Some(dispatch.input),
            after_native_completion: false,
            after_provider_completion: dispatch
                .after_provider_completion
                .map(|source| source.request),
            coordinator_wake: dispatch.coordinator_wake,
        },
        true,
    )
    .await
    .map_err(|(_, message)| anyhow::anyhow!(message))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_pin_tracks_destination_without_coupling_labels_or_observations() {
        let mut binding = ConversationRecord {
            id: "conversation".into(),
            owner_id: "owner".into(),
            provider: Provider::GrokBot,
            label: "Bot".into(),
            target: "bot".into(),
            dot_user_id: None,
            webhook_url: Some("https://example.com/hook".into()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            events: vec![],
            api_access: None,
        };
        let pin = target_digest(&binding).unwrap();
        binding.label = "Renamed".into();
        binding.updated_at = Utc::now();
        assert_eq!(target_digest(&binding).unwrap(), pin);
        let mut changed = binding.clone();
        changed.target = "different-bot".into();
        assert_ne!(target_digest(&changed).unwrap(), pin);
        changed = binding.clone();
        changed.owner_id = "other-owner".into();
        assert_ne!(target_digest(&changed).unwrap(), pin);
        changed = binding;
        changed.webhook_url = Some("https://example.com/replacement".into());
        assert_ne!(target_digest(&changed).unwrap(), pin);
    }
}
