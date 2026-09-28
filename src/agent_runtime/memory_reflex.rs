//! Locus memory reflex attached to Medousa's System 1 layer.
//!
//! The reflex returns a bus envelope. It does not read or write the store.
//! When `MEDOUSA_SYSTEM_ONE_ENGINE=laya`, the same `/v1/systemone` client used
//! for turn perception posts the locus-sdk question catalog and the raw body
//! is applied as `system1Response`. The host engine, and a failed forward
//! pass, leave that field unset so Stasis uses the offline heuristic.

use std::sync::Arc;

use locus_sdk::prelude::{MemoryReflexService, MemoryScope, MemoryStimulus};
use serde_json::{Map, Value};
use stasis::application::runtime::in_memory_runtime::{JobExecutionOutcome, JobHandler};
use stasis::application::runtime::memory_reflex_job_handler::MemoryReflexJobHandler;
use stasis::domain::runtime::job::Job;
use stasis::ports::outbound::memory::memory_operations::MemoryOperations;

use super::laya_system_one::LayaSystemOneEngine;

pub(crate) fn reflex_system1_request_body(
    text: &str,
    role: Option<&str>,
    session_ids: Option<Vec<String>>,
    metadata: Map<String, Value>,
    model: &str,
) -> Value {
    let stimulus = MemoryStimulus {
        text: text.to_string(),
        role: role.map(str::to_string),
        scope: MemoryScope {
            session_ids,
            ..Default::default()
        },
        metadata,
        ..Default::default()
    };
    let mut body = serde_json::to_value(MemoryReflexService::heuristic().request_for(&stimulus))
        .unwrap_or_else(|_| Value::Object(Map::new()));
    if let Some(object) = body.as_object_mut() {
        object.insert("model".to_string(), Value::String(model.to_string()));
    }
    body
}

/// Ask the configured System 1 engine for a memory-reflex forward pass.
///
/// `None` means the caller should let Stasis use the offline heuristic.
pub(crate) async fn configured_system1_response(
    text: &str,
    role: Option<&str>,
    session_ids: Option<Vec<String>>,
    metadata: Map<String, Value>,
) -> Option<Value> {
    if text.trim().is_empty() {
        return None;
    }
    let engine = match LayaSystemOneEngine::from_env_if_selected() {
        Ok(Some(engine)) => engine,
        Ok(None) => return None,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "memory reflex kept the offline heuristic after System 1 setup failed"
            );
            return None;
        }
    };
    let body = reflex_system1_request_body(text, role, session_ids, metadata, engine.model());
    match engine.post_systemone(&body).await {
        Ok(response) => Some(response),
        Err(error) => {
            tracing::warn!(
                error = %error,
                "memory reflex kept the offline heuristic after the System 1 forward pass failed"
            );
            None
        }
    }
}

pub(crate) async fn enrich_reflex_job_payload(payload_ref: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(payload_ref) else {
        return payload_ref.to_string();
    };
    let Some(object) = value.as_object() else {
        return payload_ref.to_string();
    };
    if field_present(object, "system1Response") || field_present(object, "system1Endpoint") {
        return payload_ref.to_string();
    }
    let text = object
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let role = object
        .get("role")
        .and_then(Value::as_str)
        .map(str::to_string);
    let session_ids = object
        .get("sessionIds")
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok());
    let metadata = object
        .get("metadata")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let Some(response) =
        configured_system1_response(&text, role.as_deref(), session_ids, metadata).await
    else {
        return payload_ref.to_string();
    };
    if let Some(object) = value.as_object_mut() {
        object.insert("system1Response".to_string(), response);
    }
    serde_json::to_string(&value).unwrap_or_else(|_| payload_ref.to_string())
}

fn field_present(object: &Map<String, Value>, key: &str) -> bool {
    object.get(key).is_some_and(|value| !value.is_null())
}

pub(crate) struct SystemOneMemoryReflexJobHandler {
    inner: MemoryReflexJobHandler,
}

impl SystemOneMemoryReflexJobHandler {
    pub(crate) fn new(operations: Arc<dyn MemoryOperations>) -> Self {
        Self {
            inner: MemoryReflexJobHandler::new(operations),
        }
    }
}

#[async_trait::async_trait]
impl JobHandler for SystemOneMemoryReflexJobHandler {
    fn job_type(&self) -> &'static str {
        "workflow.stasis.memory.reflex"
    }

    async fn execute(&self, job: &Job) -> stasis::prelude::Result<JobExecutionOutcome> {
        let payload_ref = enrich_reflex_job_payload(&job.payload_ref).await;
        let enriched = Job {
            payload_ref,
            ..job.clone()
        };
        self.inner.execute(&enriched).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stasis::prelude::MemoryReflexRequest;
    use stasis::prelude_ext::{LocusMemoryOperations, LocusNodeStoreFactory};

    #[test]
    fn reflex_catalog_asks_the_five_system1_questions() {
        let body = reflex_system1_request_body(
            "please remember that I prefer aisle seats",
            Some("user"),
            Some(vec!["session-a".to_string()]),
            Map::new(),
            "typed-decisions",
        );
        let questions = body["questions"].as_object().expect("questions");
        for name in [
            "action",
            "salience",
            "references_prior",
            "should_persist",
            "needs_system2",
        ] {
            assert!(questions.contains_key(name), "missing {name}");
        }
        assert_eq!(body["model"], "typed-decisions");
        assert_eq!(
            body["state"]["text"],
            "please remember that I prefer aisle seats"
        );
        assert_eq!(body["state"]["role"], "user");
    }

    #[tokio::test]
    async fn supplied_system1_body_is_left_on_the_job() {
        let payload = r#"{"text":"thanks","system1Response":{"answers":{}}}"#;
        let enriched = enrich_reflex_job_payload(payload).await;
        assert_eq!(enriched, payload);
    }

    #[tokio::test]
    async fn host_engine_leaves_the_job_on_the_heuristic() {
        let payload = r#"{"text":"do you remember the refund thread"}"#;
        let enriched = enrich_reflex_job_payload(payload).await;
        let value: Value = serde_json::from_str(&enriched).expect("payload");
        assert!(value.get("system1Response").is_none());
        assert!(value.get("system1Endpoint").is_none());
    }

    #[tokio::test]
    async fn system1_wire_body_dispatches_persist_through_stasis() {
        let memory = LocusNodeStoreFactory::in_memory()
            .await
            .expect("in-memory locus store");
        let operations = LocusMemoryOperations::new(memory, None);
        let wire = serde_json::json!({
            "routing": {"model": "typed-decisions"},
            "answers": {
                "action": {"choice": "persist", "confidence": 0.93},
                "salience": {"score": 2.4, "max": 3.0, "confidence": 0.9},
                "references_prior": {"noul": 0.1},
                "should_persist": {"noul": 0.88},
                "needs_system2": {"noul": 0.05}
            }
        });

        let reflex = operations
            .reflex(&MemoryReflexRequest {
                text: "please remember that I prefer aisle seats".to_string(),
                role: Some("user".to_string()),
                system1_response: Some(wire),
                ..Default::default()
            })
            .await
            .expect("apply system 1 body");

        assert_eq!(reflex.kind, "dispatch");
        assert_eq!(reflex.action, "persist");
        assert_eq!(reflex.topic, "locus.memory.persist");
        assert_eq!(reflex.decider_id, "wire");
        assert_eq!(reflex.checkpoint.as_deref(), Some("typed-decisions"));
    }
}
