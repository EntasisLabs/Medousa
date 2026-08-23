//! Daemon evidence adapter for the portable model-facing perception governor.

use std::sync::Arc;

pub use medousa_runtime::perception::*;

#[derive(Clone)]
pub struct DaemonPerceptionEvidencePort {
    undertaking_id: String,
    store: super::coder_evidence::CoderEvidenceStore,
    compact_receipt_sink: Option<Arc<dyn super::coder_evidence::CompactEvidenceReceiptSink>>,
}

impl DaemonPerceptionEvidencePort {
    pub fn for_coder_undertaking(
        undertaking_id: String,
        compact_receipt_sink: Option<Arc<dyn super::coder_evidence::CompactEvidenceReceiptSink>>,
    ) -> Self {
        Self::new(
            undertaking_id,
            super::coder_evidence::CoderEvidenceStore::for_data_root(
                &crate::paths::medousa_data_dir(),
            ),
            compact_receipt_sink,
        )
    }

    pub fn new(
        undertaking_id: String,
        store: super::coder_evidence::CoderEvidenceStore,
        compact_receipt_sink: Option<Arc<dyn super::coder_evidence::CompactEvidenceReceiptSink>>,
    ) -> Self {
        Self {
            undertaking_id,
            store,
            compact_receipt_sink,
        }
    }
}

impl medousa_runtime::PerceptionEvidencePort for DaemonPerceptionEvidencePort {
    fn persist(
        &self,
        request: medousa_runtime::PerceptionEvidenceRequest<'_>,
    ) -> Result<medousa_runtime::PersistedPerceptionEvidence, String> {
        let retention = if request.failed {
            super::coder_evidence::EvidenceRetention::FailedOrNonReproducible
        } else {
            super::coder_evidence::EvidenceRetention::SuccessfulOrReproducible
        };
        let receipt = self.store.put(
            &self.undertaking_id,
            request.output,
            retention,
            super::coder_tools::COGNITION_CODER_EVIDENCE_READ,
        )?;
        let logical_bytes = receipt.logical_bytes;
        let (durable_receipt_staged, receipt_stage_error) = match self.compact_receipt_sink.as_ref()
        {
            Some(sink) => match sink.stage_compact_receipt(
                request.tool_name,
                request.source_call_id,
                &receipt,
            ) {
                Ok(()) => (true, None),
                Err(error) => (false, Some(error)),
            },
            None => (false, None),
        };
        Ok(medousa_runtime::PersistedPerceptionEvidence {
            receipt: serde_json::to_value(receipt).map_err(|error| error.to_string())?,
            logical_bytes,
            durable_receipt_staged,
            receipt_stage_error,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Default)]
    struct RecordingReceiptSink {
        receipts: std::sync::Mutex<Vec<(String, String, String)>>,
    }

    impl super::super::coder_evidence::CompactEvidenceReceiptSink for RecordingReceiptSink {
        fn stage_compact_receipt(
            &self,
            source_tool: &str,
            source_call_id: Option<&str>,
            receipt: &super::super::coder_evidence::CoderEvidenceReceipt,
        ) -> Result<(), String> {
            self.receipts.lock().unwrap().push((
                source_tool.to_owned(),
                source_call_id.unwrap_or_default().to_owned(),
                receipt.digest.clone(),
            ));
            Ok(())
        }
    }

    #[test]
    fn coder_non_replayable_observation_gets_followable_ephemeral_receipt() {
        let temp = tempfile::TempDir::new().unwrap();
        let receipt_sink = Arc::new(RecordingReceiptSink::default());
        let store = super::super::coder_evidence::CoderEvidenceStore::new(
            temp.path().join("coder-evidence"),
            super::super::coder_evidence::EvidencePolicy::default(),
        );
        let evidence_port: Arc<dyn medousa_runtime::PerceptionEvidencePort> =
            Arc::new(DaemonPerceptionEvidencePort::new(
                "work-test".into(),
                store.clone(),
                Some(receipt_sink.clone()),
            ));
        let mut governor = ToolPerceptionGovernor::new(Some(evidence_port));
        let observed = governor.observe_for_call(
            "cognition_shell_session_run",
            Some("model-call-7"),
            &json!({
                "ok": false,
                "headers": {"authorization": "Bearer secret"},
                "stderr": "compile failure\n".repeat(4_000),
            }),
            4_096,
        );
        let receipt = &observed["ephemeral_evidence"]["receipt"];
        assert_eq!(observed["perception_status"], "bounded");
        assert_eq!(observed["ephemeral_evidence"]["status"], "stored");
        assert_eq!(
            observed["ephemeral_evidence"]["durable_receipt_staged"],
            true
        );
        assert_eq!(receipt["redacted"], true);
        assert_eq!(
            receipt["read_tool"],
            super::super::coder_tools::COGNITION_CODER_EVIDENCE_READ
        );
        assert!(
            receipt["reference"]
                .as_str()
                .is_some_and(|value| value.starts_with("coder-evidence:sha256:"))
        );
        let stored = store
            .read_range(
                "work-test",
                receipt["reference"].as_str().unwrap(),
                0,
                32 * 1024,
            )
            .unwrap();
        assert!(stored.content.contains("[REDACTED]"));
        assert!(!stored.content.contains("Bearer secret"));
        let staged = receipt_sink.receipts.lock().unwrap();
        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].0, "cognition_shell_session_run");
        assert_eq!(staged[0].1, "model-call-7");
        assert_eq!(staged[0].2, receipt["digest"].as_str().unwrap());
        drop(staged);
        let metrics = governor.take_round_metrics();
        assert_eq!(metrics.stored_evidence_results, 1);
        assert!(metrics.stored_evidence_logical_bytes > 20_000);
    }
}
