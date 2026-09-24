//! Canonical notification policy for Medousa Home.
//!
//! Domain producers emit one typed intent after their source fact is durable.
//! Local OS notifications and APNs are transports for that same intent; neither
//! transport is allowed to infer notification policy from cards or tool events.

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use chrono::Utc;
use medousa_types::{
    HomeNotificationIntent, HomeNotificationKind, TurnCompletionOutcomeV3,
    TurnStreamEnvelopeV3, TurnStreamEventV3,
};
use tokio::sync::broadcast;

use crate::runtime_composition_ext::RuntimeCompositionExt;

const INTENT_BUFFER: usize = 256;
const DEDUPE_LIMIT: usize = 1024;

static INTENTS: LazyLock<broadcast::Sender<HomeNotificationIntent>> =
    LazyLock::new(|| broadcast::channel(INTENT_BUFFER).0);
static PUBLISHED: LazyLock<Mutex<(HashSet<String>, Vec<String>)>> =
    LazyLock::new(|| Mutex::new((HashSet::new(), Vec::new())));

pub fn subscribe() -> broadcast::Receiver<HomeNotificationIntent> {
    INTENTS.subscribe()
}

/// Publish one logical alert to every delivery route. Duplicate ids are
/// discarded before either local delivery or APNs sees them.
pub fn publish(intent: HomeNotificationIntent) {
    let mut published = PUBLISHED.lock().expect("home notification dedupe lock");
    if published.0.contains(&intent.notification_id) {
        return;
    }
    published.0.insert(intent.notification_id.clone());
    published.1.push(intent.notification_id.clone());
    if published.1.len() > DEDUPE_LIMIT {
        let oldest = published.1.remove(0);
        published.0.remove(&oldest);
    }
    drop(published);

    let _ = INTENTS.send(intent.clone());
    crate::home_push::notify_intent(intent);
}

pub fn publish_turn_envelope(envelope: &TurnStreamEnvelopeV3) {
    if let Some(intent) = intent_for_turn_envelope(envelope) {
        publish(intent);
    }
}

pub fn intent_for_turn_envelope(
    envelope: &TurnStreamEnvelopeV3,
) -> Option<HomeNotificationIntent> {
    let turn_id = envelope.turn_id.trim();
    if turn_id.is_empty() {
        return None;
    }

    let (notification_id, kind, subject_id, title, body, card_id) = match &envelope.event {
        TurnStreamEventV3::TurnCompleted {
            outcome: TurnCompletionOutcomeV3::Fatal,
            operator_message,
            aggregate_text,
            ..
        } => (
            format!("fatal-turn:{turn_id}"),
            HomeNotificationKind::FatalTurn,
            turn_id.to_string(),
            "Medousa — turn stopped".to_string(),
            preview(operator_message.as_deref().unwrap_or(aggregate_text), "Turn stopped"),
            Some(turn_id.to_string()),
        ),
        TurnStreamEventV3::TurnCompleted {
            outcome: TurnCompletionOutcomeV3::Completed,
            aggregate_text,
            ..
        } => (
            format!("turn-update:{turn_id}"),
            HomeNotificationKind::TurnUpdate,
            turn_id.to_string(),
            "Medousa — turn ready".to_string(),
            preview(aggregate_text, "Turn finished"),
            Some(turn_id.to_string()),
        ),
        TurnStreamEventV3::TurnCompleted {
            outcome: TurnCompletionOutcomeV3::NeedsInput,
            aggregate_text,
            ..
        } => (
            format!("needs-input:{turn_id}"),
            HomeNotificationKind::NeedsInput,
            turn_id.to_string(),
            "Medousa — needs your input".to_string(),
            preview(aggregate_text, "Open Medousa to continue"),
            Some(turn_id.to_string()),
        ),
        TurnStreamEventV3::WorkerSynthesis {
            text,
            work_id: Some(work_id),
            ..
        } if !work_id.trim().is_empty() => (
            format!("worker-result:{}", work_id.trim()),
            HomeNotificationKind::TurnUpdate,
            work_id.trim().to_string(),
            "Medousa — background work ready".to_string(),
            preview(text, "Background work finished"),
            Some(work_id.trim().to_string()),
        ),
        TurnStreamEventV3::BudgetApprovalRequired {
            request_id,
            reason,
            progress_summary,
            ..
        } if !request_id.trim().is_empty() => (
            format!("needs-input:{}", request_id.trim()),
            HomeNotificationKind::NeedsInput,
            request_id.trim().to_string(),
            "Medousa — approve more rounds?".to_string(),
            preview(progress_summary.as_deref().unwrap_or(reason), "Turn needs approval"),
            Some(request_id.trim().to_string()),
        ),
        TurnStreamEventV3::PermissionRequest {
            request_id, message, ..
        } if !request_id.trim().is_empty() => (
            format!("needs-input:{}", request_id.trim()),
            HomeNotificationKind::NeedsInput,
            request_id.trim().to_string(),
            "Medousa — permission needed".to_string(),
            preview(message, "Open Medousa to review"),
            Some(turn_id.to_string()),
        ),
        TurnStreamEventV3::SecretRequest {
            request_id,
            label,
            reason,
            ..
        } if !request_id.trim().is_empty() => (
            format!("needs-input:{}", request_id.trim()),
            HomeNotificationKind::NeedsInput,
            request_id.trim().to_string(),
            format!("Medousa — {label}"),
            preview(reason, "Secure input needed"),
            Some(turn_id.to_string()),
        ),
        TurnStreamEventV3::BrowserChallenge { reason, .. } => (
            format!("needs-input:browser:{turn_id}"),
            HomeNotificationKind::NeedsInput,
            turn_id.to_string(),
            "Medousa — browser needs you".to_string(),
            preview(reason, "Open Medousa to continue"),
            Some(turn_id.to_string()),
        ),
        _ => return None,
    };

    Some(HomeNotificationIntent {
        notification_id,
        kind,
        subject_id,
        title,
        body,
        card_id,
        session_id: None,
        url: None,
        emitted_at_utc: Utc::now(),
    })
}

pub fn scheduled_delivery_intent(
    job_id: &str,
    title: &str,
    output_text: &str,
) -> Option<HomeNotificationIntent> {
    let job_id = job_id.trim();
    if job_id.is_empty() {
        return None;
    }
    Some(HomeNotificationIntent {
        notification_id: format!("scheduled-delivery:{job_id}"),
        kind: HomeNotificationKind::ScheduledDelivery,
        subject_id: job_id.to_string(),
        title: if title.trim().is_empty() {
            "Medousa — scheduled result ready".to_string()
        } else {
            format!("Medousa — {}", title.trim())
        },
        body: preview(output_text, "Scheduled result delivered in Medousa"),
        card_id: Some(job_id.to_string()),
        session_id: None,
        url: None,
        emitted_at_utc: Utc::now(),
    })
}

/// Emit the in-app delivery notification only after the processed recurring
/// job and its successful attempt are queryable from the runtime store.
pub async fn maybe_publish_processed_recurring_job(
    runtime: &stasis::prelude::RuntimeComposition,
    job_id: &str,
) {
    let Ok(Some(job)) = runtime.get_job(job_id).await else {
        return;
    };
    let recurring_id = job.correlation_id.trim();
    if recurring_id.is_empty() {
        return;
    }
    let Ok(Some(definition)) = crate::recurring_handlers::get_recurring_definition(
        runtime,
        recurring_id,
    )
    .await
    else {
        return;
    };
    let Ok(attempts) = runtime.list_job_attempts(job_id).await else {
        return;
    };
    let Some(attempt) = attempts.last() else {
        return;
    };
    let output = crate::channel_delivery::extract_output_text_from_diagnostics(
        attempt.diagnostics.as_deref(),
    )
    .unwrap_or_default();
    let diagnostics = attempt
        .diagnostics
        .as_deref()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok());
    let turn_outcome = diagnostics
        .as_ref()
        .and_then(|value| value.get("turn_outcome"))
        .and_then(serde_json::Value::as_str);
    let title = crate::recurring_handlers::display_name_from_payload(
        &definition.payload_template_ref,
    )
    .unwrap_or_else(|| "scheduled result ready".to_string());

    if job.state != stasis::domain::runtime::job::JobState::Succeeded {
        if turn_outcome == Some("fatal") {
            publish(HomeNotificationIntent {
                notification_id: format!("fatal-turn:scheduled:{job_id}"),
                kind: HomeNotificationKind::FatalTurn,
                subject_id: job_id.to_string(),
                title: format!("Medousa — {title} stopped"),
                body: preview(&output, "Scheduled turn stopped"),
                card_id: Some(job_id.to_string()),
                session_id: None,
                url: None,
                emitted_at_utc: Utc::now(),
            });
        }
        return;
    }
    if !crate::recurring_handlers::notify_on_delivery_from_payload(
        &definition.payload_template_ref,
    ) {
        return;
    }
    // A linked external channel owns presentation for that delivery choice.
    if crate::recurring_delivery::delivery_binding_for_recurring(recurring_id)
        .await
        .is_some()
    {
        return;
    }

    if turn_outcome == Some("needs_input") {
        publish(HomeNotificationIntent {
            notification_id: format!("needs-input:{job_id}"),
            kind: HomeNotificationKind::NeedsInput,
            subject_id: job_id.to_string(),
            title: format!("Medousa — {title} needs your input"),
            body: preview(&output, "Open Medousa to continue"),
            card_id: Some(job_id.to_string()),
            session_id: None,
            url: None,
            emitted_at_utc: Utc::now(),
        });
        return;
    }
    if turn_outcome.is_some_and(|outcome| outcome != "completed") {
        return;
    }
    if let Some(intent) = scheduled_delivery_intent(job_id, &title, &output) {
        publish(intent);
    }
}

fn preview(value: &str, fallback: &str) -> String {
    let line = value.lines().find(|line| !line.trim().is_empty()).map(str::trim);
    let value = line.unwrap_or(fallback);
    if value.chars().count() <= 120 {
        return value.to_string();
    }
    format!("{}…", value.chars().take(117).collect::<String>())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use medousa_types::{TurnStreamEnvelopeV3, TurnStreamEventV3};

    use super::*;

    fn envelope(event: TurnStreamEventV3) -> TurnStreamEnvelopeV3 {
        TurnStreamEnvelopeV3::new("turn-1", 1, Utc::now(), event).unwrap()
    }

    #[test]
    fn failed_is_silent_but_explicit_fatal_alerts() {
        let failed = envelope(TurnStreamEventV3::TurnCompleted {
            outcome: TurnCompletionOutcomeV3::Failed,
            aggregate_text: String::new(),
            tool_names: Vec::new(),
            operator_message: Some("failed".into()),
            debug_message: None,
        });
        assert!(intent_for_turn_envelope(&failed).is_none());

        let fatal = envelope(TurnStreamEventV3::TurnCompleted {
            outcome: TurnCompletionOutcomeV3::Fatal,
            aggregate_text: String::new(),
            tool_names: Vec::new(),
            operator_message: Some("loop stopped".into()),
            debug_message: None,
        });
        assert_eq!(
            intent_for_turn_envelope(&fatal).unwrap().kind,
            HomeNotificationKind::FatalTurn
        );
    }

    #[test]
    fn tool_events_and_worker_starts_are_silent() {
        let tool = envelope(TurnStreamEventV3::ToolFinished {
            tool_run_id: "tool-1".into(),
            tool_name: "search".into(),
            status: "failed".into(),
            input_summary: String::new(),
            input_params: Vec::new(),
            output_summary: Some("nope".into()),
            artifact_refs: Vec::new(),
            tool_round: 1,
        });
        assert!(intent_for_turn_envelope(&tool).is_none());
        let worker = envelope(TurnStreamEventV3::WorkerAck {
            ack_kind: medousa_types::WorkerAckKind::Worker,
            text: "started".into(),
            tool_names: Vec::new(),
            work_id: Some("work-1".into()),
        });
        assert!(intent_for_turn_envelope(&worker).is_none());
    }

    #[test]
    fn final_input_and_scheduled_results_have_stable_categories() {
        let completed = envelope(TurnStreamEventV3::TurnCompleted {
            outcome: TurnCompletionOutcomeV3::Completed,
            aggregate_text: "done".into(),
            tool_names: Vec::new(),
            operator_message: None,
            debug_message: None,
        });
        let completed = intent_for_turn_envelope(&completed).expect("completed intent");
        assert_eq!(completed.notification_id, "turn-update:turn-1");
        assert_eq!(completed.kind, HomeNotificationKind::TurnUpdate);

        let needs_input = envelope(TurnStreamEventV3::TurnCompleted {
            outcome: TurnCompletionOutcomeV3::NeedsInput,
            aggregate_text: "choose a target".into(),
            tool_names: Vec::new(),
            operator_message: None,
            debug_message: None,
        });
        assert_eq!(
            intent_for_turn_envelope(&needs_input).unwrap().kind,
            HomeNotificationKind::NeedsInput
        );

        let scheduled = scheduled_delivery_intent("job-1", "Daily brief", "ready")
            .expect("scheduled intent");
        assert_eq!(scheduled.notification_id, "scheduled-delivery:job-1");
        assert_eq!(scheduled.kind, HomeNotificationKind::ScheduledDelivery);
    }
}
