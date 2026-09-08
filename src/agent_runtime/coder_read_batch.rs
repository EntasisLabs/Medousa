//! One purpose shared by a bounded set of independent workspace observations.
//! Children re-enter the caller's registry; this is not another authority path.

use futures_util::{StreamExt, stream};
use genai::chat::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::domain::errors::StasisError;
use stasis::prelude::Result;

pub const TOOL_NAME: &str = "cognition_coder_read_batch";
const MAX_OPERATIONS: usize = 4;
const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAX_ITEM_RESULT_BYTES: usize = 24 * 1024;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadBatch {
    /// Independent reads/searches with known arguments; results use zero-based indexes.
    #[schemars(length(min = 1, max = 4))]
    operations: Vec<ReadOperation>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "action", deny_unknown_fields)]
enum ReadOperation {
    #[serde(rename = "code.read")]
    Read {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 1))]
        line_start: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 1))]
        line_end: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        byte_start: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        byte_end: Option<u64>,
    },
    #[serde(rename = "code.search")]
    Search {
        query: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 1, max = 100))]
        max_results: Option<u64>,
    },
}

impl ReadOperation {
    fn validate(&self) -> Result<()> {
        let valid = match self {
            Self::Read {
                path,
                line_start,
                line_end,
                byte_start,
                byte_end,
            } => {
                !path.trim().is_empty()
                    && *line_start != Some(0)
                    && *line_end != Some(0)
                    && line_start
                        .zip(*line_end)
                        .is_none_or(|(start, end)| start <= end)
                    && byte_start
                        .zip(*byte_end)
                        .is_none_or(|(start, end)| start <= end)
                    && !((line_start.is_some() || line_end.is_some())
                        && (byte_start.is_some() || byte_end.is_some()))
            }
            Self::Search { query, max_results } => {
                !query.trim().is_empty()
                    && max_results.is_none_or(|limit| (1..=100).contains(&limit))
            }
        };
        if !valid {
            return Err(StasisError::PortFailure(
                "invalid batch read/search: require a nonempty path/query, ordered ranges of one kind, and search limit 1..=100".into(),
            ));
        }
        Ok(())
    }
}

pub fn tool_definition() -> Tool {
    Tool::new(TOOL_NAME)
        .with_description("Read/search the workspace with one shared intent: 1-4 independent operations. Results are indexed; oversized items require a narrower read. No writes or dependent operations.")
        .with_schema(crate::typed_tools::normalize_input_schema::<ReadBatch>()
            .expect("Coder read batch schema must normalize"))
}

pub async fn invoke(
    registry: &dyn ToolRegistry,
    input: Value,
    intent: &str,
    settings: &crate::execution_policy::ParallelExecutionSettings,
) -> Result<Value> {
    if input.to_string().len() > MAX_INPUT_BYTES {
        return Err(StasisError::PortFailure(
            "read batch exceeds 64 KiB input limit".into(),
        ));
    }
    let batch: ReadBatch = serde_json::from_value(input)
        .map_err(|error| StasisError::PortFailure(format!("invalid read batch: {error}")))?;
    if batch.operations.is_empty() || batch.operations.len() > MAX_OPERATIONS {
        return Err(StasisError::PortFailure(
            "read batch requires 1..=4 operations".into(),
        ));
    }
    // Validate the entire typed request before starting any child.
    for operation in &batch.operations {
        operation.validate()?;
    }
    let concurrency = if settings.parallel_tool_calls_enabled {
        settings.max_parallel_tool_calls.clamp(1, MAX_OPERATIONS)
    } else {
        1
    };
    // Futures remain in this task: cancellation drops active/queued children,
    // and execution-context task locals remain available to every child.
    let mut results = stream::iter(batch.operations.into_iter().enumerate())
        .map(|(index, operation)| async move {
            let mut input = serde_json::to_value(operation).expect("read operation serializes");
            input["intent"] = Value::String(intent.to_owned());
            let result = registry
                .invoke_tool(crate::public_api::COGNITION_STORE_READ, input)
                .await;
            bounded_result(index, result)
        })
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await;
    results.sort_unstable_by_key(|item| item["index"].as_u64().expect("batch index"));
    // Four <=24 KiB results plus bounded error/envelope fields stay below 128 KiB.
    Ok(json!({
        "ok": results.iter().all(|item| item["ok"] == true),
        "results": results,
    }))
}

fn bounded_result(index: usize, result: Result<Value>) -> Value {
    match result {
        Ok(result) => {
            let bytes = result.to_string().len();
            if bytes > MAX_ITEM_RESULT_BYTES {
                json!({"index": index, "ok": false, "code": "batch_result_too_large",
                    "result_bytes": bytes,
                    "error": "Result omitted: narrow the line/byte range or search limit, or use a single code.read/code.search call."})
            } else {
                json!({"index": index, "ok": result.get("ok").and_then(Value::as_bool).unwrap_or(true), "result": result})
            }
        }
        Err(error) => json!({"index": index, "ok": false, "code": "batch_item_failed",
            "error": super::coder_evidence::redact_evidence_text(&error.to_string()).chars().take(1000).collect::<String>()}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::sync::{Notify, Semaphore};

    #[derive(Default)]
    struct Reads {
        inputs: Mutex<Vec<Value>>,
        active: AtomicUsize,
        peak: AtomicUsize,
        started: Notify,
        gate: Option<Semaphore>,
    }

    struct Active<'a>(&'a AtomicUsize);
    impl Drop for Active<'_> {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[async_trait::async_trait]
    impl ToolRegistry for Reads {
        async fn list_tools(&self) -> Result<Vec<Tool>> {
            Ok(vec![])
        }
        async fn invoke_tool(&self, name: &str, input: Value) -> Result<Value> {
            assert_eq!(name, crate::public_api::COGNITION_STORE_READ);
            self.inputs.lock().unwrap().push(input.clone());
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            let _active = Active(&self.active);
            self.peak.fetch_max(active, Ordering::SeqCst);
            self.started.notify_one();
            if let Some(gate) = &self.gate {
                let _permit = gate.acquire().await.unwrap();
            }
            tokio::task::yield_now().await;
            if input["path"] == "a" {
                tokio::task::yield_now().await;
            }
            match input["path"].as_str() {
                Some("fail") => Err(StasisError::PortFailure("read denied".into())),
                Some("large") => Ok(json!({"content": "x".repeat(MAX_ITEM_RESULT_BYTES)})),
                _ => Ok(json!({"ok": true, "path": input["path"], "query": input["query"]})),
            }
        }
    }

    fn settings() -> crate::execution_policy::ParallelExecutionSettings {
        crate::execution_policy::ParallelExecutionSettings::default()
    }

    #[tokio::test]
    async fn rejects_invalid_entire_batch_before_any_child() {
        let reads = Reads::default();
        for input in [
            json!({"operations": []}),
            json!({"operations": vec![json!({"action":"code.read", "path":"a"}); 5]}),
            json!({"operations": [{"action":"code.read", "path":"a"}, {"action":"code.write", "path":"b"}]}),
            json!({"operations": [{"action":"code.read", "path":"a", "intent":"override"}]}),
            json!({"operations": [{"action":"code.read", "path":"a", "root":"/escape"}]}),
            json!({"operations": [{"action":"code.read", "path":"a", "line_start":0}]}),
            json!({"operations": [{"action":"code.read", "path":"a", "line_start":3, "line_end":2}]}),
            json!({"operations": [{"action":"code.read", "path":"a", "line_end":3, "byte_start":0}]}),
            json!({"operations": [{"action":"code.search", "query":"a", "max_results":101}]}),
            json!({"operations": [{"action":"code.search", "query":"x".repeat(MAX_INPUT_BYTES)}]}),
            json!({"operations": [{"action":TOOL_NAME, "operations":[]}]}),
        ] {
            assert!(
                invoke(&reads, input, "Inspect callers", &settings())
                    .await
                    .is_err()
            );
        }
        assert!(reads.inputs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn bounds_results_preserves_indexes_and_isolates_item_errors() {
        let reads = Reads::default();
        let result = invoke(
            &reads,
            json!({"operations": [
                {"action":"code.read", "path":"a"},
                {"action":"code.read", "path":"fail"},
                {"action":"code.read", "path":"large"},
                {"action":"code.search", "query":"caller", "max_results":5}
            ]}),
            "Locate affected callers",
            &settings(),
        )
        .await
        .unwrap();
        assert_eq!(result["ok"], false);
        for (index, item) in result["results"].as_array().unwrap().iter().enumerate() {
            assert_eq!(item["index"], index);
        }
        assert_eq!(result["results"][0]["result"]["path"], "a");
        assert_eq!(result["results"][1]["code"], "batch_item_failed");
        assert_eq!(result["results"][2]["code"], "batch_result_too_large");
        assert!(result["results"][2].get("result").is_none());
        assert_eq!(result["results"][3]["result"]["query"], "caller");
        assert!(result.to_string().len() < 128 * 1024);
        assert!(
            reads
                .inputs
                .lock()
                .unwrap()
                .iter()
                .all(|input| input["intent"] == "Locate affected callers")
        );
    }

    #[tokio::test]
    async fn concurrency_respects_settings_and_cancellation_drops_children() {
        for parallel in [false, true] {
            let reads = Reads {
                gate: Some(Semaphore::new(0)),
                ..Reads::default()
            };
            let settings = crate::execution_policy::ParallelExecutionSettings {
                parallel_tool_calls_enabled: parallel,
                max_parallel_tool_calls: 2,
                ..settings()
            };
            let expected = if parallel { 2 } else { 1 };
            let input = json!({"operations": vec![json!({"action":"code.read", "path":"a"}); 4]});
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                tokio::select! {
                    _ = invoke(&reads, input, "Inspect callers", &settings) => panic!("children should block"),
                    _ = async {
                        loop {
                            let changed = reads.started.notified();
                            if reads.active.load(Ordering::SeqCst) == expected { break; }
                            changed.await;
                        }
                    } => {}
                }
            }).await.unwrap();
            assert_eq!(reads.active.load(Ordering::SeqCst), 0);
            assert_eq!(reads.peak.load(Ordering::SeqCst), expected);
            assert_eq!(reads.inputs.lock().unwrap().len(), expected);
        }
    }
}
