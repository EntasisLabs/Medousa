//! FeedSink router — recurring job terminal events → feed_bus publish.

use stasis::domain::runtime::job::JobState;
use stasis::ports::outbound::runtime::job_store::JobStore;
use stasis::prelude::RuntimeComposition;

use crate::channel_delivery;
use crate::daemon::ingest::get_job_attempts_graceful;
use crate::feed_adapters::{self, JobTerminalPhase, RecurringTickContext};
use crate::recurring_feed;

pub async fn maybe_publish_recurring_job_feed(runtime: &RuntimeComposition, job_id: &str) {
    let Some(job) = load_job(runtime, job_id).await else {
        return;
    };

    if !is_terminal(&job.state) {
        return;
    }

    let recurring_id = job.correlation_id.trim();
    if recurring_id.is_empty() {
        return;
    }

    let Some(binding) = recurring_feed::feed_binding_for_recurring(recurring_id).await else {
        return;
    };

    let phase = terminal_phase(&job.state);
    let output_excerpt = resolve_output_excerpt(runtime, job_id).await;
    let parsed_poll = output_excerpt
        .as_deref()
        .and_then(feed_adapters::parse_http_poll_output);

    let ctx = RecurringTickContext {
        recurring_id: recurring_id.to_string(),
        job_id: job_id.to_string(),
        job_type: job.job_type.clone(),
        phase,
        output_excerpt,
        parsed_poll,
        payload_mode: binding.payload_mode,
    };

    for feed_id in &binding.feed_ids {
        feed_adapters::publish_recurring_tick(feed_id, &ctx).await;
    }
}

async fn load_job(
    runtime: &RuntimeComposition,
    job_id: &str,
) -> Option<stasis::domain::runtime::job::Job> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_store.get(job_id).await.ok().flatten(),
        RuntimeComposition::Surreal(rt) => rt.job_store.get(job_id).await.ok().flatten(),
    }
}

fn is_terminal(state: &JobState) -> bool {
    matches!(
        state,
        JobState::Succeeded | JobState::Failed | JobState::DeadLetter | JobState::Canceled
    )
}

fn terminal_phase(state: &JobState) -> JobTerminalPhase {
    match state {
        JobState::Succeeded => JobTerminalPhase::TickSucceeded,
        JobState::Canceled => JobTerminalPhase::TickFailed,
        JobState::Failed | JobState::DeadLetter => JobTerminalPhase::TickFailed,
        _ => JobTerminalPhase::TickFailed,
    }
}

async fn resolve_output_excerpt(runtime: &RuntimeComposition, job_id: &str) -> Option<String> {
    let attempts = get_job_attempts_graceful(runtime, job_id)
        .await
        .unwrap_or_default();
    attempts.iter().rev().find_map(|attempt| {
        channel_delivery::extract_output_text_from_diagnostics(attempt.diagnostics.as_deref())
    })
}
