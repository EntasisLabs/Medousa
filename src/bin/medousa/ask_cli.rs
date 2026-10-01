//! Operator entry point for explicitly placed external-agent Bot work.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use medousa::daemon_api::resolve_daemon_url;
use serde_json::{Value, json};

use super::cli::AskArgs;

pub fn run_ask(args: AskArgs) -> Result<()> {
    let daemon_url = args
        .daemon_url
        .as_deref()
        .map(str::to_owned)
        .unwrap_or_else(|| resolve_daemon_url(None));
    let client = medousa::local_daemon_auth::blocking_client_with_timeout(
        &daemon_url,
        medousa_local_credential::CLI_LOCAL_NAME,
        Duration::from_secs(15),
    )?;
    if let Some(job_id) = args.cancel.as_deref() {
        let response = client
            .post(format!("{daemon_url}/v1/bots/ask/{job_id}/cancel"))
            .send()?;
        let status = response.status();
        let body: Value = response.json().context("read Bot cancellation response")?;
        if !status.is_success() {
            bail!(
                "{}: {}",
                body.pointer("/error/code")
                    .and_then(Value::as_str)
                    .unwrap_or("cancel_failed"),
                body.pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("Bot job could not be cancelled")
            );
        }
        println!("cancelled: {job_id}");
        return Ok(());
    }
    let job_id = if let Some(job_id) = args.resume.as_deref() {
        job_id.to_owned()
    } else {
        let prompt = args
            .prompt
            .as_deref()
            .context("usage: medousa ask \"prompt\" --bot NAME")?;
        if args.bot.is_none() == args.agent.is_none() {
            bail!("choose --bot NAME or --agent codex [--workshop NAME]");
        }
        let request_id = args
            .request_id
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        eprintln!("request_id: {request_id}");
        let request = json!({
            "prompt": prompt,
            "request_id": request_id,
            "bot": args.bot,
            "agent": args.agent,
            "workshop": args.workshop,
        });
        let response = client
            .post(format!("{daemon_url}/v1/bots/ask"))
            .json(&request)
            .send()
            .with_context(|| {
                format!(
                    "submission outcome uncertain; retry with the same --request-id {request_id}"
                )
            })?;
        let status = response.status();
        let body: Value = response.json().context("read Bot submission response")?;
        if !status.is_success() {
            bail!(
                "{}: {}",
                body.pointer("/error/code")
                    .and_then(Value::as_str)
                    .unwrap_or("submission_failed"),
                body.pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("Bot request failed")
            );
        }
        let job_id = body
            .pointer("/ticket/jobId")
            .and_then(Value::as_str)
            .context("daemon did not return a job handle")?
            .to_owned();
        let destination = body
            .pointer("/ticket/executionPlacement/resolvedRuntimeId")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        eprintln!("job: {job_id} destination: {destination}");
        job_id
    };
    if args.detach {
        println!("{job_id}");
        return Ok(());
    }
    let mut previous = String::new();
    let observing_since = std::time::Instant::now();
    loop {
        let response = client
            .get(format!("{daemon_url}/v1/bots/ask/{job_id}"))
            .send()
            .with_context(|| {
                format!("observation interrupted; resume with medousa ask --resume {job_id}")
            })?;
        let status = response.status();
        let body: Value = response.json().context("read Bot job status")?;
        if !status.is_success() {
            bail!(
                "{}: {}",
                body.pointer("/error/code")
                    .and_then(Value::as_str)
                    .unwrap_or("observation_failed"),
                body.pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("Bot job unavailable")
            );
        }
        let phase = body
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if phase != previous {
            eprintln!("{phase}: {job_id}");
            previous = phase.to_owned();
        }
        if matches!(phase, "completed" | "succeeded") {
            let destination = body
                .get("destination_runtime_id")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let runtime = body
                .get("runtime")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let workdir = body
                .pointer("/terminal_payload/external_agent_workdir")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let session = body
                .pointer("/terminal_payload/external_agent_session_id")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            eprintln!(
                "job: {job_id} destination: {destination} runtime: {runtime} workdir: {workdir} agent_session: {session}"
            );
            if let Some(text) = body.get("result").and_then(Value::as_str) {
                println!("{text}");
            }
            return Ok(());
        }
        if matches!(phase, "failed" | "cancelled" | "canceled") {
            bail!(
                "Bot job {phase}: {}",
                body.get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("see job status")
            );
        }
        if phase == "timed_out" {
            bail!("Bot job timed out: {job_id}");
        }
        if phase == "transport_pending" && observing_since.elapsed() > Duration::from_secs(30) {
            bail!(
                "destination admission is still uncertain; job {job_id} remains active. Resume with medousa ask --resume {job_id}"
            );
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}
