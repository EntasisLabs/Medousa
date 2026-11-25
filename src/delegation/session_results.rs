//! The foreground loop reads the same correlated waits that the transport settles.
use super::*;
use crate::execution_context::TurnExecutionContext;
use medousa_runtime::ports::SessionEventsPort;

// Stasis owns the receipt. This notification only wakes the waiting loop.
pub(super) static RESULTS_CHANGED: tokio::sync::Notify = tokio::sync::Notify::const_new();

type PendingSessionWait = (String, Arc<dyn TurnWaitStore>);

#[derive(Default)]
pub(crate) struct SessionWaits {
    // A consumed ID stays in the same execution until it ends, so a send retry
    // cannot register the same terminal receipt twice.
    requests: Mutex<HashMap<String, Option<PendingSessionWait>>>,
}

impl std::fmt::Debug for SessionWaits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionWaits").finish_non_exhaustive()
    }
}

impl SessionWaits {
    pub(crate) fn track(&self, id: String, request: String, store: Arc<dyn TurnWaitStore>) {
        self.requests
            .lock()
            .expect("session waits")
            .entry(id)
            .or_insert(Some((request, store)));
    }
}

#[async_trait::async_trait]
impl SessionEventsPort for TurnExecutionContext {
    async fn receive(&self, yielding: bool) -> StasisResult<Vec<Value>> {
        let waits = &self.session_waits;
        loop {
            let changed = RESULTS_CHANGED.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.cancellation().is_cancelled()
                || self
                    .deadline()
                    .is_some_and(|at| std::time::Instant::now() >= at)
            {
                return Err(port_failure("original turn cancelled or expired"));
            }
            let requests: Vec<_> = waits
                .requests
                .lock()
                .expect("session waits")
                .iter()
                .filter_map(|(id, wait)| wait.clone().map(|wait| (id.clone(), wait)))
                .collect();
            let mut received = Vec::new();
            let mut events = Vec::new();
            for (id, (request, store)) in &requests {
                let Some(record) = store.get(id).await? else {
                    // Missing durable state is not a still-running worker.
                    // Surface the broken correlation once instead of polling forever.
                    received.push(id.clone());
                    events.push(json!({"request_id": request, "turn_id": id,
                        "status": "delivery_failed",
                        "error": "Registered worker wait is missing; inspect the assignment receipt before retrying."}));
                    continue;
                };
                if record.session_id != self.session_id().as_str() {
                    return Err(port_failure(
                        "terminal result belongs to a different session",
                    ));
                }
                if record.status == TurnWaitStatus::Pending {
                    continue;
                }
                let data = record.result_payload.as_ref();
                let result = data
                    .and_then(|p| p.get("text").or_else(|| p.get("error")))
                    .and_then(Value::as_str)
                    .or(record.error_message.as_deref())
                    .unwrap_or_default();
                received.push(id.clone());
                events.push(
                    json!({"request_id": request, "worker": record.participant_id,
                    "status": record.status, "result": result}),
                );
            }
            #[cfg(feature = "full-daemon")]
            events.extend(
                crate::agent_messaging::receive_for_turn(self)
                    .await
                    .map_err(port_failure)?,
            );
            if !events.is_empty() {
                let mut pending = waits.requests.lock().expect("session waits");
                for id in received {
                    pending.insert(id, None);
                }
                return Ok(events);
            }
            if !yielding || requests.is_empty() {
                return Ok(events);
            }
            tokio::select! {
                _ = self.cancellation().cancelled() => return Err(port_failure("original turn cancelled")),
                _ = changed => {},
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        }
    }
}

/// Native workers already have durable assignments. Stasis correlates their terminal receipt.
pub(crate) struct NativeSessionWait {
    pub id: String,
    pub store: Arc<dyn TurnWaitStore>,
    record: TurnWaitRecord,
}

impl NativeSessionWait {
    pub async fn open(
        runtime: &RuntimeComposition,
        scope: &str,
        session: &str,
        request: &str,
        worker: &str,
    ) -> StasisResult<Self> {
        let identity = deterministic_identity(
            "native-",
            &[
                scope.as_bytes(),
                session.as_bytes(),
                request.as_bytes(),
                worker.as_bytes(),
            ],
        );
        let id = format!("{DELEGATION_TURN_PREFIX}{identity}");
        let store: Arc<dyn TurnWaitStore> = Arc::new(RuntimeDelegationWaitStore::new(runtime));
        let now = Utc::now();
        let record = TurnWaitRecord {
            turn_id: id.clone(),
            job_id: format!("{DELEGATION_JOB_PREFIX}{identity}"),
            session_id: session.into(),
            correlation_id: request.into(),
            participant_id: worker.into(),
            status: TurnWaitStatus::Pending,
            deadline_at: DateTime::<Utc>::MAX_UTC,
            created_at: now,
            updated_at: now,
            result_payload: None,
            error_message: None,
        };
        if store.get(&id).await?.is_none() {
            if let Err(error) = store.insert(record.clone()).await
                && store.get(&id).await?.is_none()
            {
                return Err(error);
            }
        }
        Ok(Self { id, store, record })
    }

    pub async fn settle(
        &self,
        kind: AgentEnvelopeKind,
        receipt: &str,
        payload: Value,
    ) -> StasisResult<()> {
        let record = &self.record;
        let expected = match kind {
            AgentEnvelopeKind::TurnCompleted => TurnWaitStatus::Completed,
            AgentEnvelopeKind::Failed => TurnWaitStatus::Failed,
            AgentEnvelopeKind::Cancelled => TurnWaitStatus::Cancelled,
            _ => return Err(port_failure("invalid terminal envelope kind")),
        };
        let matches_receipt = |saved: &TurnWaitRecord| {
            saved.status == expected && saved.result_payload.as_ref() == Some(&payload)
        };
        if let Some(saved) = self.store.get(&self.id).await?
            && saved.status != TurnWaitStatus::Pending
        {
            return if matches_receipt(&saved) {
                Ok(())
            } else {
                Err(port_failure("conflicting terminal receipt"))
            };
        }
        let envelope = AgentEnvelope {
            schema_version: AGENT_ENVELOPE_SCHEMA_VERSION_V1,
            kind,
            envelope_id: receipt.into(),
            session_id: record.session_id.clone(),
            thread_id: None,
            turn_id: Some(self.id.clone()),
            job_id: Some(record.job_id.clone()),
            correlation_id: record.correlation_id.clone(),
            causation_id: record.turn_id.clone(),
            participant_id: Some(record.participant_id.clone()),
            occurred_at: Utc::now(),
            payload: payload.clone(),
        };
        let ingress = WaitCorrelatingAgentEventIngress::new(
            Arc::new(InMemoryAgentEventIngress::new()),
            self.store.clone(),
        );
        let ack = ingress.accept(envelope).await?;
        // Concurrent ingress can observe the same pending wait before one wins the durable write.
        if self
            .store
            .get(&self.id)
            .await?
            .as_ref()
            .is_some_and(matches_receipt)
        {
            return Ok(());
        }
        if ack.disposition != IngressDisposition::Accepted {
            return Err(port_failure("Stasis rejected terminal receipt"));
        }
        Err(port_failure("conflicting terminal receipt"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stasis::application::runtime::in_memory_runtime::InMemoryRuntime;

    #[tokio::test]
    async fn missing_wait_reports_once_without_hanging_at_yield() {
        let runtime = RuntimeComposition::InMemory(InMemoryRuntime::new());
        let wait = NativeSessionWait::open(&runtime, "test", "ses_source", "request", "worker")
            .await
            .unwrap();
        let scope = crate::turn_continuation::TurnContinuationScope {
            turn_correlation_id: "source-turn".into(),
            session_id: "ses_source".into(),
            identity_user_id: Some("profile-source".into()),
            original_prompt: "test".into(),
            delivery_target: None,
            provider: "test".into(),
            model: "test".into(),
            response_depth_mode: "normal".into(),
            supports_ui_artifacts: false,
            supports_liquid_markdown: false,
            supports_browser_host: false,
            browser_driver_id: None,
            selected_worlds: vec![],
            channel_surface: None,
        };
        let turn = TurnExecutionContext::from_scope(
            "source-turn",
            crate::request_principal::RequestPrincipal::continuation("profile-source"),
            tokio_util::sync::CancellationToken::new(),
            std::time::Instant::now() + Duration::from_secs(10),
            scope,
        )
        .unwrap();
        turn.session_waits.track(
            "missing-wait-id".into(),
            "missing-request".into(),
            wait.store,
        );
        let events = tokio::time::timeout(Duration::from_secs(1), turn.receive(true))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["status"], "delivery_failed");
        assert_eq!(events[0]["request_id"], "missing-request");
        assert!(turn.receive(true).await.unwrap().is_empty());
    }
}
