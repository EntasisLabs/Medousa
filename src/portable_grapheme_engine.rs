//! Portable Grapheme execution without a timeout that detaches live work.

use std::sync::{Arc, OnceLock};

use grapheme_sdk::GraphemeEngine;
use medousa_forge::execution::{ExecutionClass, ForgeExecutionService};
use serde_json::Value;
use stasis::domain::errors::{Result, StasisError};
use stasis::infrastructure::runtime::grapheme_sdk_workflow_engine::GraphemeWorkflowGuardrails;
use stasis::ports::outbound::runtime::workflow_engine::{WorkflowEngine, WorkflowExecutionOutput};

pub(crate) fn workflow_engine() -> Arc<dyn WorkflowEngine> {
    Arc::new(PortableGraphemeEngine {
        guardrails: GraphemeWorkflowGuardrails::default(),
    })
}

struct PortableGraphemeEngine {
    guardrails: GraphemeWorkflowGuardrails,
}

fn execution_service() -> &'static ForgeExecutionService {
    static EXECUTION: OnceLock<ForgeExecutionService> = OnceLock::new();
    EXECUTION.get_or_init(ForgeExecutionService::new)
}

impl PortableGraphemeEngine {
    fn validate_source(&self, source: &str) -> Result<()> {
        if source.len() > self.guardrails.max_source_bytes {
            return Err(StasisError::PortFailure(format!(
                "grapheme policy violation: source size {} exceeds max {} bytes",
                source.len(),
                self.guardrails.max_source_bytes,
            )));
        }
        for line in source.lines().map(str::trim) {
            if !line.starts_with("import ") {
                continue;
            }
            let quote = if line.contains('"') { '"' } else { '\'' };
            let Some(start) = line.find(quote) else {
                continue;
            };
            let tail = &line[start + 1..];
            let Some(end) = tail.find(quote) else {
                continue;
            };
            let import = &tail[..end];
            if !self.guardrails.allowed_imports.iter().any(|pattern| {
                pattern
                    .strip_suffix('*')
                    .map_or(pattern == import, |prefix| import.starts_with(prefix))
            }) {
                return Err(StasisError::PortFailure(format!(
                    "grapheme policy violation: import '{import}' is not allowlisted"
                )));
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl WorkflowEngine for PortableGraphemeEngine {
    async fn execute_grapheme_source(
        &self,
        source: &str,
        state_current: Option<&Value>,
    ) -> Result<WorkflowExecutionOutput> {
        self.validate_source(source)?;
        let source = source.to_string();
        let state = state_current.cloned();
        let guardrails = self.guardrails.clone();
        let result = execution_service()
            .run(ExecutionClass::Compaction, source.len().max(1), move || {
                let engine = GraphemeEngine::builder()
                    .with_max_steps(guardrails.max_steps)
                    .with_max_call_depth(guardrails.max_call_depth)
                    .build();
                Ok(engine.execute_source_with_initial_state(&source, state))
            })
            .await
            .map_err(|error| {
                StasisError::PortFailure(format!("grapheme execution admission failed: {error}"))
            })?
            .map_err(|error| {
                StasisError::PortFailure(format!("grapheme sdk execution error: {error}"))
            })?;
        Ok(WorkflowExecutionOutput {
            run_id: format!("grapheme:{}", result.artifact_id),
            execution: serde_json::to_value(&result.execution).unwrap_or(Value::Null),
            final_state: result.final_state,
            lint_warnings: serde_json::to_value(&result.lint_warnings).unwrap_or(Value::Null),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn portable_execution_does_not_use_the_legacy_total_timeout() {
        let engine = PortableGraphemeEngine {
            guardrails: GraphemeWorkflowGuardrails {
                execution_timeout: std::time::Duration::ZERO,
                ..Default::default()
            },
        };
        engine
            .execute_grapheme_source(
                "import core from \"grapheme/core\"\nquery Echo { core.echo(message: \"ready\") { message } }",
                None,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn portable_execution_keeps_source_and_import_authority_limits() {
        let engine = PortableGraphemeEngine {
            guardrails: GraphemeWorkflowGuardrails {
                max_source_bytes: 32,
                ..Default::default()
            },
        };
        assert!(
            engine
                .execute_grapheme_source(&"x".repeat(33), None)
                .await
                .is_err()
        );
        assert!(
            engine
                .execute_grapheme_source("import \"foreign/io\";", None)
                .await
                .is_err()
        );
    }
}
