//! Clap front-end for `medousa_cli` (daemon-* helpers).

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "medousa_cli",
    about = "Medousa daemon HTTP helpers (health, ask, identity, watches).",
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Ask(AskArgs),
    Llm(AskArgs),
    #[command(name = "daemon-health")]
    DaemonHealth(DaemonUrlArgs),
    #[command(name = "daemon-stats")]
    DaemonStats(DaemonUrlArgs),
    #[command(name = "daemon-heartbeat-status")]
    DaemonHeartbeatStatus(DaemonUrlArgs),
    #[command(name = "daemon-first-run")]
    DaemonFirstRun(DaemonFirstRunArgs),
    #[command(name = "daemon-ask")]
    DaemonAsk(DaemonAskArgs),
    #[command(name = "daemon-report")]
    DaemonReport(DaemonReportArgs),
    #[command(name = "daemon-job-report")]
    DaemonJobReport(DaemonJobReportArgs),
    #[command(name = "daemon-watch-add")]
    DaemonWatchAdd(DaemonWatchAddArgs),
    #[command(name = "daemon-identity-context")]
    DaemonIdentityContext(IdentityCommonArgs),
    #[command(name = "daemon-identity-inspect")]
    DaemonIdentityInspect(IdentityInspectArgs),
    #[command(name = "daemon-identity-propose")]
    DaemonIdentityPropose(IdentityProposeArgs),
    #[command(name = "daemon-identity-update")]
    DaemonIdentityUpdate(IdentityUpdateArgs),
    #[command(name = "daemon-identity-commit")]
    DaemonIdentityCommit(IdentityCommitArgs),
    #[command(name = "daemon-identity-history")]
    DaemonIdentityHistory(IdentityHistoryArgs),
    #[command(name = "daemon-identity-review")]
    DaemonIdentityReview(IdentityHistoryArgs),
    #[command(name = "daemon-identity-explain")]
    DaemonIdentityExplain(IdentityHistoryArgs),
    #[command(name = "daemon-identity-rollback")]
    DaemonIdentityRollback(IdentityRollbackArgs),
}

#[derive(Debug, Args)]
pub struct AskArgs {
    pub prompt: String,
    #[arg(long)]
    pub backend: Option<String>,
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long = "base-url")]
    pub base_url: Option<String>,
}

#[derive(Debug, Args)]
pub struct DaemonUrlArgs {
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
}

#[derive(Debug, Args)]
pub struct DaemonFirstRunArgs {
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long = "report-query")]
    pub report_query: Option<String>,
}

#[derive(Debug, Args)]
pub struct DaemonAskArgs {
    pub prompt: String,
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long = "no-wait")]
    pub no_wait: bool,
    #[arg(long = "identity-user-id")]
    pub identity_user_id: Option<String>,
    #[arg(long = "identity-channel-id")]
    pub identity_channel_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct DaemonReportArgs {
    pub query: String,
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long = "policy-profile")]
    pub policy_profile: Option<String>,
    #[arg(long = "model-hint")]
    pub model_hint: Option<String>,
    #[arg(long = "max-turns")]
    pub max_turns: Option<String>,
    #[arg(long = "poll-timeout-ms")]
    pub poll_timeout_ms: Option<String>,
    #[arg(long = "poll-interval-ms")]
    pub poll_interval_ms: Option<String>,
    #[arg(long = "identity-user-id")]
    pub identity_user_id: Option<String>,
    #[arg(long = "identity-persona-id")]
    pub identity_persona_id: Option<String>,
    #[arg(long = "identity-channel-id")]
    pub identity_channel_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct DaemonJobReportArgs {
    pub job_id: String,
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
}

#[derive(Debug, Args)]
pub struct DaemonWatchAddArgs {
    pub cron_expr: String,
    pub prompt: Vec<String>,
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long = "tz", default_value = "UTC")]
    pub tz: String,
}

#[derive(Debug, Args)]
pub struct IdentityCommonArgs {
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long = "user-id")]
    pub user_id: Option<String>,
    #[arg(long = "persona-id")]
    pub persona_id: Option<String>,
    #[arg(long = "channel-id")]
    pub channel_id: Option<String>,
    #[arg(long = "policy-profile")]
    pub policy_profile: Option<String>,
    #[arg(long = "relationship-limit")]
    pub relationship_limit: Option<String>,
    #[arg(long)]
    pub mode: Option<String>,
}

#[derive(Debug, Args)]
pub struct IdentityInspectArgs {
    #[command(flatten)]
    pub common: IdentityCommonArgs,
    #[arg(long)]
    pub raw: bool,
}

#[derive(Debug, Args)]
pub struct IdentityProposeArgs {
    pub entity_type: String,
    pub entity_id: String,
    pub patch_json: String,
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long)]
    pub source: Option<String>,
    #[arg(long)]
    pub confidence: Option<String>,
    #[arg(long)]
    pub reason: Option<String>,
    #[arg(long)]
    pub actor: Option<String>,
    #[arg(long = "receipt-id")]
    pub receipt_id: Option<String>,
    #[arg(long = "expires-at")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Args)]
pub struct IdentityUpdateArgs {
    #[command(flatten)]
    pub propose: IdentityProposeArgs,
    #[arg(long = "auto-commit")]
    pub auto_commit: bool,
    #[arg(long = "expected-version")]
    pub expected_version: Option<String>,
    #[arg(long)]
    pub approver: Option<String>,
    #[arg(long)]
    pub raw: bool,
}

#[derive(Debug, Args)]
pub struct IdentityCommitArgs {
    pub proposal_id: String,
    pub expected_version: String,
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long)]
    pub approver: Option<String>,
    #[arg(long)]
    pub raw: bool,
}

#[derive(Debug, Args)]
pub struct IdentityHistoryArgs {
    pub entity_type: String,
    pub entity_id: String,
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long)]
    pub limit: Option<String>,
    #[arg(long)]
    pub raw: bool,
}

#[derive(Debug, Args)]
pub struct IdentityRollbackArgs {
    pub entity_type: String,
    pub entity_id: String,
    pub target_version: String,
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long)]
    pub reason: Option<String>,
    #[arg(long)]
    pub approver: Option<String>,
    #[arg(long)]
    pub raw: bool,
}

fn push_flag(out: &mut Vec<String>, flag: &str, on: bool) {
    if on {
        out.push(flag.to_string());
    }
}

fn push_opt(out: &mut Vec<String>, flag: &str, value: Option<&str>) {
    if let Some(v) = value {
        out.push(flag.to_string());
        out.push(v.to_string());
    }
}

impl DaemonFirstRunArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_opt(&mut out, "--report-query", self.report_query.as_deref());
        out
    }
}

impl DaemonAskArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = vec![self.prompt.clone()];
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_flag(&mut out, "--no-wait", self.no_wait);
        push_opt(
            &mut out,
            "--identity-user-id",
            self.identity_user_id.as_deref(),
        );
        push_opt(
            &mut out,
            "--identity-channel-id",
            self.identity_channel_id.as_deref(),
        );
        out
    }
}

impl DaemonReportArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = vec![self.query.clone()];
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_opt(&mut out, "--policy-profile", self.policy_profile.as_deref());
        push_opt(&mut out, "--model-hint", self.model_hint.as_deref());
        push_opt(&mut out, "--max-turns", self.max_turns.as_deref());
        push_opt(
            &mut out,
            "--poll-timeout-ms",
            self.poll_timeout_ms.as_deref(),
        );
        push_opt(
            &mut out,
            "--poll-interval-ms",
            self.poll_interval_ms.as_deref(),
        );
        push_opt(
            &mut out,
            "--identity-user-id",
            self.identity_user_id.as_deref(),
        );
        push_opt(
            &mut out,
            "--identity-persona-id",
            self.identity_persona_id.as_deref(),
        );
        push_opt(
            &mut out,
            "--identity-channel-id",
            self.identity_channel_id.as_deref(),
        );
        out
    }
}

impl IdentityCommonArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_opt(&mut out, "--user-id", self.user_id.as_deref());
        push_opt(&mut out, "--persona-id", self.persona_id.as_deref());
        push_opt(&mut out, "--channel-id", self.channel_id.as_deref());
        push_opt(&mut out, "--policy-profile", self.policy_profile.as_deref());
        push_opt(
            &mut out,
            "--relationship-limit",
            self.relationship_limit.as_deref(),
        );
        push_opt(&mut out, "--mode", self.mode.as_deref());
        out
    }
}

impl IdentityInspectArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = self.common.to_legacy();
        push_flag(&mut out, "--raw", self.raw);
        out
    }
}

impl IdentityProposeArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = vec![
            self.entity_type.clone(),
            self.entity_id.clone(),
            self.patch_json.clone(),
        ];
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_opt(&mut out, "--source", self.source.as_deref());
        push_opt(&mut out, "--confidence", self.confidence.as_deref());
        push_opt(&mut out, "--reason", self.reason.as_deref());
        push_opt(&mut out, "--actor", self.actor.as_deref());
        push_opt(&mut out, "--receipt-id", self.receipt_id.as_deref());
        push_opt(&mut out, "--expires-at", self.expires_at.as_deref());
        out
    }
}

impl IdentityUpdateArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = self.propose.to_legacy();
        push_flag(&mut out, "--auto-commit", self.auto_commit);
        push_opt(
            &mut out,
            "--expected-version",
            self.expected_version.as_deref(),
        );
        push_opt(&mut out, "--approver", self.approver.as_deref());
        push_flag(&mut out, "--raw", self.raw);
        out
    }
}

impl IdentityCommitArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = vec![self.proposal_id.clone(), self.expected_version.clone()];
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_opt(&mut out, "--approver", self.approver.as_deref());
        push_flag(&mut out, "--raw", self.raw);
        out
    }
}

impl IdentityHistoryArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = vec![self.entity_type.clone(), self.entity_id.clone()];
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_opt(&mut out, "--limit", self.limit.as_deref());
        push_flag(&mut out, "--raw", self.raw);
        out
    }
}

impl IdentityRollbackArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = vec![
            self.entity_type.clone(),
            self.entity_id.clone(),
            self.target_version.clone(),
        ];
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_opt(&mut out, "--reason", self.reason.as_deref());
        push_opt(&mut out, "--approver", self.approver.as_deref());
        push_flag(&mut out, "--raw", self.raw);
        out
    }
}
