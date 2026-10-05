use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TargetConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Absolute path on the workshop, never on the Home device. None uses ssh-agent.
    pub identity_file: Option<String>,
    pub agent_access: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SaveTarget {
    pub config: TargetConfig,
    /// Public host key lines returned by inspect. Saving explicitly trusts these keys.
    pub host_keys: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Target {
    pub id: String,
    pub owner: String,
    pub config: TargetConfig,
    pub host_keys: Vec<String>,
}

impl Target {
    pub fn summary(&self) -> serde_json::Value {
        serde_json::json!({"target_id": self.id, "name": self.config.name,
            "host": self.config.host, "port": self.config.port,
            "username": self.config.username, "agent_access": self.config.agent_access})
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetsQuery {}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionQuery {
    pub execution_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunInput {
    pub target_id: String,
    /// Stable per operation. Reuse it on transport retries; never resend under a new key.
    pub request_key: String,
    /// A remote shell command; arguments are interpreted by the server's shell.
    pub command: String,
    /// Optional remote-command observation deadline. A timeout does not prove remote cancellation.
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TerminalInput {
    pub target_id: String,
    pub request_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TerminalWriteInput {
    pub execution_id: String,
    /// Raw terminal input, including a newline when submitting a command. Omit to observe.
    pub input: Option<String>,
    pub after_sequence: Option<u64>,
    pub wait_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Execution {
    pub execution_id: String,
    pub owner: String,
    pub target_id: String,
    pub request_key: String,
    pub command: Option<String>,
    pub timeout_ms: Option<u64>,
    pub status: String,
    pub stdout: String,
    pub stderr: String,
    pub output_truncated: bool,
    pub exit_code: Option<i32>,
    pub session_id: Option<String>,
    pub error: Option<String>,
}

impl Execution {
    pub fn summary(&self) -> serde_json::Value {
        serde_json::json!({"execution_id": self.execution_id, "target_id": self.target_id,
            "status": self.status, "stdout": self.stdout, "stderr": self.stderr,
            "output_truncated": self.output_truncated, "exit_code": self.exit_code,
            "session_id": self.session_id, "error": self.error,
            "remote_outcome_unknown": self.status == "unknown"})
    }
}
