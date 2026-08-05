//! Runtime-inferred coordination claims for concurrent Coder agents.

use std::collections::HashSet;
use std::path::Path;

use medousa_forge::model::ExecutionLease;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoderClaimMode {
    Read,
    Write,
    Verify,
}

impl CoderClaimMode {
    pub fn conflicts_with(self, other: Self) -> bool {
        matches!(self, Self::Write) || matches!(other, Self::Write)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoderClaimScope {
    pub target: String,
    pub mode: CoderClaimMode,
    #[serde(default)]
    pub hazardous: bool,
    pub reason: String,
}

pub fn infer_tool_claims(
    tool_name: &str,
    input: &Value,
    lease: &ExecutionLease,
    worktree: &Path,
) -> Vec<CoderClaimScope> {
    let mut claims = Vec::new();
    let mode = tool_mode(tool_name, input);

    if let Some(path) = input.get("path").and_then(Value::as_str) {
        let path = logical_path(path, worktree);
        if !path.is_empty() {
            claims.push(CoderClaimScope {
                target: format!("file://{path}"),
                mode,
                hazardous: hazardous_path(&path),
                reason: "tool path".into(),
            });
            add_path_resource_claims(&mut claims, &path, mode);
        }
    }
    if let Some(uri) = input.get("uri").and_then(Value::as_str) {
        let path = logical_path(uri, worktree);
        claims.push(CoderClaimScope {
            target: format!("file://{path}"),
            mode,
            hazardous: false,
            reason: "code-intelligence document".into(),
        });
    }
    if tool_name == crate::coding_tools::COGNITION_CODE_SEARCH {
        claims.push(CoderClaimScope {
            target: format!("tree://{}", lease.work_id),
            mode: CoderClaimMode::Read,
            hazardous: false,
            reason: "repository search".into(),
        });
    }
    if tool_name == crate::coding_tools::COGNITION_SHELL_SESSION_RUN
        || tool_name == crate::coding_tools::COGNITION_CODER_SHELL_RUN
    {
        infer_shell_claims(input, lease, &mut claims);
    } else if tool_name == crate::coding_tools::COGNITION_SHELL_SESSION_STATUS
        || tool_name == crate::coding_tools::COGNITION_SHELL_SESSION_INTERRUPT
        || tool_name == crate::coding_tools::COGNITION_CODER_SHELL_STATUS
    {
        let session = input
            .get("session_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("attempt-{}", lease.attempt_id));
        claims.push(CoderClaimScope {
            target: format!("shell://{session}"),
            mode,
            hazardous: false,
            reason: "governed shell session".into(),
        });
    }
    if tool_name.starts_with("cognition_detamu_") {
        claims.push(CoderClaimScope {
            target: format!("world://{}", lease.work_id),
            mode: CoderClaimMode::Read,
            hazardous: false,
            reason: "undertaking world model".into(),
        });
    }
    if tool_name.starts_with("cognition_engineering_")
        || tool_name == super::coder_tools::COGNITION_CODER_EVIDENCE_READ
    {
        claims.push(CoderClaimScope {
            target: format!("ledger://{}", lease.work_id),
            mode: CoderClaimMode::Read,
            hazardous: false,
            reason: "engineering activity ledger".into(),
        });
    }

    deduplicate_claims(claims)
}

fn tool_mode(tool_name: &str, input: &Value) -> CoderClaimMode {
    if tool_name == crate::coding_tools::COGNITION_CODE_APPLY_PATCH
        || tool_name == crate::coding_tools::COGNITION_SHELL_SESSION_INTERRUPT
    {
        CoderClaimMode::Write
    } else if tool_name == crate::coding_tools::COGNITION_SHELL_SESSION_RUN
        || tool_name == crate::coding_tools::COGNITION_CODER_SHELL_RUN
    {
        let command = shell_input(input).to_ascii_lowercase();
        if is_verification_command(&command) {
            CoderClaimMode::Verify
        } else {
            CoderClaimMode::Write
        }
    } else {
        CoderClaimMode::Read
    }
}

fn add_path_resource_claims(claims: &mut Vec<CoderClaimScope>, path: &str, mode: CoderClaimMode) {
    let lower = path.to_ascii_lowercase();
    let file_name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    if is_lockfile(file_name) {
        claims.push(hazardous(
            format!("resource://lockfile/{file_name}"),
            mode,
            "dependency lockfile serialization",
        ));
    }
    if lower
        .split('/')
        .any(|part| matches!(part, "migration" | "migrations" | "migrate" | "schema"))
    {
        claims.push(hazardous(
            "resource://database/migrations".into(),
            mode,
            "ordered database migration sequence",
        ));
    }
    if lower.contains("generated/")
        || lower.contains("/generated_")
        || lower.contains(".generated.")
    {
        claims.push(hazardous(
            "resource://generated/artifacts".into(),
            mode,
            "generated artifact set",
        ));
    }
}

fn infer_shell_claims(input: &Value, lease: &ExecutionLease, claims: &mut Vec<CoderClaimScope>) {
    let command = shell_input(input);
    let lower = command.to_ascii_lowercase();
    let mode = tool_mode(crate::coding_tools::COGNITION_SHELL_SESSION_RUN, input);
    claims.push(CoderClaimScope {
        target: format!("shell://attempt/{}", lease.attempt_id),
        mode,
        hazardous: false,
        reason: "attempt-scoped shell execution".into(),
    });

    if contains_any(
        &lower,
        &[
            "git gc",
            "git worktree",
            "git branch",
            "git tag",
            "git push",
            "git fetch",
            "git pull",
            "git merge",
            "git rebase",
            "git cherry-pick",
            "git reset",
            "git stash",
            "git update-ref",
        ],
    ) {
        claims.push(hazardous(
            format!("resource://git/refs/{}", lease.work_id),
            CoderClaimMode::Write,
            "shared Git reference mutation",
        ));
    }
    if contains_any(
        &lower,
        &["psql ", "mysql ", "sqlite3 ", "redis-cli ", "mongosh "],
    ) {
        claims.push(hazardous(
            "resource://database/default".into(),
            CoderClaimMode::Write,
            "shared database mutation",
        ));
    }
    if contains_any(&lower, &["git add", "git commit", "git rm", "git mv"]) {
        claims.push(hazardous(
            format!("resource://git/index/{}", lease.attempt_id),
            CoderClaimMode::Write,
            "attempt Git index mutation",
        ));
    }
    for (needle, lockfile) in [
        ("npm install", "package-lock.json"),
        ("npm update", "package-lock.json"),
        ("pnpm install", "pnpm-lock.yaml"),
        ("pnpm update", "pnpm-lock.yaml"),
        ("yarn install", "yarn.lock"),
        ("yarn upgrade", "yarn.lock"),
        ("bun install", "bun.lock"),
        ("cargo update", "cargo.lock"),
        ("bundle install", "gemfile.lock"),
        ("poetry lock", "poetry.lock"),
    ] {
        if lower.contains(needle) {
            claims.push(hazardous(
                format!("resource://lockfile/{lockfile}"),
                CoderClaimMode::Write,
                "dependency resolver lockfile mutation",
            ));
        }
    }
    if contains_any(
        &lower,
        &[
            " migrate",
            "migration",
            "prisma db",
            "diesel ",
            "sqlx ",
            "alembic ",
            "db:push",
            "db push",
        ],
    ) {
        claims.push(hazardous(
            "resource://database/migrations".into(),
            CoderClaimMode::Write,
            "database schema or migration mutation",
        ));
    }
    if contains_any(
        &lower,
        &[
            " deploy",
            "publish",
            "release",
            "terraform apply",
            "pulumi up",
        ],
    ) {
        claims.push(hazardous(
            "resource://deployment/default".into(),
            CoderClaimMode::Write,
            "external deployment or publication",
        ));
    }
    if contains_any(
        &lower,
        &[" generate", "codegen", "gen-schema", "prisma generate"],
    ) {
        claims.push(hazardous(
            "resource://generated/artifacts".into(),
            CoderClaimMode::Write,
            "generated artifact set",
        ));
    }
    if starts_shared_service(&lower) {
        let port = infer_port(&lower)
            .map(|port| format!("port/{port}"))
            .unwrap_or_else(|| "service/default".into());
        claims.push(hazardous(
            format!("resource://{port}"),
            CoderClaimMode::Write,
            "shared runtime service or port",
        ));
    }
}

fn hazardous(target: String, mode: CoderClaimMode, reason: &str) -> CoderClaimScope {
    CoderClaimScope {
        target,
        mode,
        hazardous: true,
        reason: reason.into(),
    }
}

fn hazardous_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let file_name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    is_lockfile(file_name)
        || lower
            .split('/')
            .any(|part| matches!(part, "migration" | "migrations" | "migrate" | "schema"))
        || lower.contains("generated/")
        || lower.contains(".generated.")
}

fn is_lockfile(file_name: &str) -> bool {
    matches!(
        file_name,
        "cargo.lock"
            | "package-lock.json"
            | "npm-shrinkwrap.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "bun.lock"
            | "bun.lockb"
            | "gemfile.lock"
            | "poetry.lock"
            | "composer.lock"
            | "go.sum"
    )
}

fn is_verification_command(command: &str) -> bool {
    contains_any(
        command,
        &[
            " test",
            "test ",
            "cargo test",
            "cargo check",
            "cargo clippy",
            " lint",
            "typecheck",
            "tsc ",
            "git diff",
            "git status",
            "git log",
            "git show",
            "pytest",
            "vitest",
            "jest",
        ],
    )
}

fn starts_shared_service(command: &str) -> bool {
    contains_any(
        command,
        &[
            "npm run dev",
            "pnpm dev",
            "yarn dev",
            "bun dev",
            "cargo run",
            " runserver",
            " serve",
            "http.server",
            "docker compose up",
        ],
    )
}

fn shell_input(input: &Value) -> &str {
    input
        .get("command")
        .or_else(|| input.get("input"))
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn infer_port(command: &str) -> Option<u16> {
    let tokens = command
        .split(|character: char| character.is_whitespace() || character == '=')
        .collect::<Vec<_>>();
    for pair in tokens.windows(2) {
        if matches!(pair[0], "--port" | "-p")
            && let Ok(port) = pair[1].trim_matches(|c: char| !c.is_ascii_digit()).parse()
        {
            return Some(port);
        }
    }
    tokens.iter().find_map(|token| {
        token.rsplit_once(':').and_then(|(_, port)| {
            port.trim_matches(|c: char| !c.is_ascii_digit())
                .parse()
                .ok()
        })
    })
}

fn logical_path(path: &str, worktree: &Path) -> String {
    let raw = path.trim().trim_start_matches("file://");
    let path = Path::new(raw);
    path.strip_prefix(worktree)
        .unwrap_or(path)
        .to_string_lossy()
        .trim_start_matches("./")
        .replace('\\', "/")
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn deduplicate_claims(claims: Vec<CoderClaimScope>) -> Vec<CoderClaimScope> {
    let mut seen = HashSet::new();
    claims
        .into_iter()
        .filter(|claim| seen.insert((claim.target.clone(), claim.mode)))
        .take(16)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use medousa_forge::model::{AttemptId, LeaseId, WorkId};

    fn lease() -> ExecutionLease {
        ExecutionLease {
            lease_id: LeaseId::new(),
            generation: 1,
            work_id: WorkId::from("work-1".to_string()),
            attempt_id: AttemptId::from("attempt-1".to_string()),
            owner_instance_id: "instance".into(),
            acquired_at: Utc::now(),
            heartbeat_at: Utc::now(),
            pid: None,
            process_start_marker: None,
        }
    }

    #[test]
    fn infers_file_modes_and_hazardous_lockfile_resource() {
        let read = infer_tool_claims(
            crate::coding_tools::COGNITION_CODE_READ,
            &serde_json::json!({ "path": "src/lib.rs" }),
            &lease(),
            Path::new("/tmp/worktree"),
        );
        assert_eq!(read[0].mode, CoderClaimMode::Read);
        assert!(!read[0].hazardous);

        let write = infer_tool_claims(
            crate::coding_tools::COGNITION_CODE_APPLY_PATCH,
            &serde_json::json!({ "path": "Cargo.lock" }),
            &lease(),
            Path::new("/tmp/worktree"),
        );
        assert!(write.iter().any(|claim| {
            claim.target == "resource://lockfile/cargo.lock"
                && claim.mode == CoderClaimMode::Write
                && claim.hazardous
        }));
    }

    #[test]
    fn infers_shared_shell_resources_without_treating_tests_as_writes() {
        let server = infer_tool_claims(
            crate::coding_tools::COGNITION_SHELL_SESSION_RUN,
            &serde_json::json!({ "command": "npm run dev -- --port 4310" }),
            &lease(),
            Path::new("/tmp/worktree"),
        );
        assert!(
            server
                .iter()
                .any(|claim| { claim.target == "resource://port/4310" && claim.hazardous })
        );

        let verify = infer_tool_claims(
            crate::coding_tools::COGNITION_SHELL_SESSION_RUN,
            &serde_json::json!({ "command": "cargo test -p medousa" }),
            &lease(),
            Path::new("/tmp/worktree"),
        );
        assert_eq!(verify[0].mode, CoderClaimMode::Verify);
        assert!(!verify.iter().any(|claim| claim.hazardous));

        let database = infer_tool_claims(
            crate::coding_tools::COGNITION_SHELL_SESSION_RUN,
            &serde_json::json!({ "input": "psql app -c 'delete from jobs'" }),
            &lease(),
            Path::new("/tmp/worktree"),
        );
        assert!(
            database
                .iter()
                .any(|claim| { claim.target == "resource://database/default" && claim.hazardous })
        );
    }

    #[test]
    fn absolute_attempt_paths_canonicalize_to_the_same_logical_file() {
        let worktree = Path::new("/forge/attempt-a");
        let claims = infer_tool_claims(
            crate::code_intelligence_tools::COGNITION_CODE_DIAGNOSTICS,
            &serde_json::json!({ "uri": "file:///forge/attempt-a/src/lib.rs" }),
            &lease(),
            worktree,
        );
        assert_eq!(claims[0].target, "file://src/lib.rs");
    }
}
