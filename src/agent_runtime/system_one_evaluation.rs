//! Durable, local evaluation records for System 1 shadow decisions.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use medousa_runtime::SystemOneEvaluationRecord;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

pub const SYSTEM_ONE_EVALUATION_ENV: &str = "MEDOUSA_SYSTEM_ONE_EVALUATION";
pub const SYSTEM_ONE_EVALUATION_DIR: &str = "system_one";
pub const SYSTEM_ONE_EVALUATION_FILE: &str = "evaluations.jsonl";

static EVALUATION_WRITE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub fn evaluation_enabled() -> bool {
    !matches!(
        std::env::var(SYSTEM_ONE_EVALUATION_ENV)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "disabled" | "off" | "false" | "0"
    )
}

pub fn default_evaluation_path() -> PathBuf {
    crate::paths::medousa_data_dir()
        .join(SYSTEM_ONE_EVALUATION_DIR)
        .join(SYSTEM_ONE_EVALUATION_FILE)
}

pub async fn persist_evaluation(record: &SystemOneEvaluationRecord) -> std::io::Result<PathBuf> {
    let path = default_evaluation_path();
    persist_evaluation_at(&path, record).await?;
    Ok(path)
}

async fn persist_evaluation_at(
    path: &Path,
    record: &SystemOneEvaluationRecord,
) -> std::io::Result<()> {
    let encoded = serde_json::to_vec(record).map_err(std::io::Error::other)?;
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "System 1 evaluation path has no parent",
        )
    })?;

    let _guard = EVALUATION_WRITE_LOCK.lock().await;
    tokio::fs::create_dir_all(parent).await?;
    let mut options = tokio::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    let mut file = options.open(path).await?;
    file.write_all(&encoded).await?;
    file.write_all(b"\n").await?;
    file.flush().await
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use medousa_runtime::{
        SystemOneDecision, SystemOneInput, SystemOneMode, SystemOneRecommendation, TurnIntent,
    };

    use super::*;

    fn record() -> SystemOneEvaluationRecord {
        SystemOneEvaluationRecord::new(
            Utc::now(),
            "session-1",
            7,
            SystemOneMode::Shadow,
            SystemOneInput {
                current_user_message: "look this up".to_string(),
                recent_context: String::new(),
            },
            SystemOneDecision::new(
                TurnIntent::ToolRequired,
                0.9,
                "requires retrieval",
                "test-engine",
            )
            .unwrap(),
            SystemOneRecommendation::PreferTools,
            TurnIntent::Conversational,
            "configured_default",
        )
    }

    #[tokio::test]
    async fn appends_newline_delimited_evaluation_records() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nested/evaluations.jsonl");
        let record = record();

        persist_evaluation_at(&path, &record).await.unwrap();
        persist_evaluation_at(&path, &record).await.unwrap();

        let content = tokio::fs::read_to_string(path).await.unwrap();
        let lines = content.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        let decoded: SystemOneEvaluationRecord = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(decoded.reference_source, "activation_heuristic");
        assert!(!decoded.agrees_with_reference);
    }
}
