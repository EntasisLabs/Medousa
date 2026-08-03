//! Forge-authoritative entry context for Medousa's native Coder mode.

use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

use medousa_forge::forge::Forge;
use medousa_forge::model::{WorkId, WorkState};

use crate::daemon_api::CodeIntentContext;

const MAX_CHANGED_PATHS: usize = 80;
const MAX_POLICY_PATHS: usize = 100;
const MAX_OPEN_FILES: usize = 24;
const MAX_DIAGNOSTICS: usize = 24;
const MAX_PATH_CHARS: usize = 512;
const MAX_FIELD_CHARS: usize = 2_000;
const MAX_SELECTED_TEXT_CHARS: usize = 6_000;
const MAX_REPO_INSTRUCTIONS_CHARS: usize = 12_000;

/// Trusted, bounded world state compiled once at Coder turn entry.
///
/// Forge supplies repository authority. Editor context is advisory and can
/// enrich the prompt, but can never select the working directory or branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderEntryContext {
    pub work_id: String,
    pub title: String,
    pub brief: String,
    pub worktree: PathBuf,
    pub branch: String,
    pub baseline_oid: String,
    pub head_oid: String,
    pub changed_paths: Vec<String>,
    pub allowed_paths: Vec<String>,
    pub denied_paths: Vec<String>,
    pub project_markers: Vec<String>,
    pub repository_instructions: Vec<RepositoryInstruction>,
    pub editor: CoderEditorContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryInstruction {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoderEditorContext {
    pub active_path: Option<String>,
    pub cursor_line: Option<u32>,
    pub selection_start_line: Option<u32>,
    pub selection_end_line: Option<u32>,
    pub selected_text: Option<String>,
    pub containing_symbol: Option<String>,
    pub open_files: Vec<String>,
    pub diagnostics: Vec<String>,
    pub last_verification: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderEntryError(pub String);

impl std::fmt::Display for CoderEntryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CoderEntryError {}

pub fn compile_coder_entry(
    forge: &Forge,
    advisory: &CodeIntentContext,
) -> Result<CoderEntryContext, CoderEntryError> {
    let work_id = advisory
        .work_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CoderEntryError("Coder mode requires a Forge undertaking".into()))?;
    let work_id = WorkId::from(work_id.to_string());
    let item = forge
        .load(&work_id)
        .map_err(|err| CoderEntryError(format!("cannot enter Coder mode: {err}")))?;
    if item.state != WorkState::Ready {
        return Err(CoderEntryError(format!(
            "Coder undertaking '{}' must be ready (currently {})",
            item.id, item.state
        )));
    }
    let environment = item.environment.as_ref().ok_or_else(|| {
        CoderEntryError(format!(
            "Coder undertaking '{}' has no governed environment",
            item.id
        ))
    })?;
    let worktree = std::fs::canonicalize(&environment.worktree).map_err(|err| {
        CoderEntryError(format!(
            "cannot resolve governed worktree '{}': {err}",
            environment.worktree.display()
        ))
    })?;
    let git_root = forge
        .git()
        .worktree_root(&worktree)
        .and_then(|path| std::fs::canonicalize(path).map_err(medousa_forge::ForgeError::Io))
        .map_err(|err| CoderEntryError(format!("cannot verify governed worktree: {err}")))?;
    if git_root != worktree {
        return Err(CoderEntryError(
            "Forge worktree authority does not match the repository root".into(),
        ));
    }
    let branch = forge
        .git()
        .current_branch(&worktree)
        .map_err(|err| CoderEntryError(format!("cannot resolve Coder branch: {err}")))?
        .ok_or_else(|| CoderEntryError("Coder worktree cannot use a detached HEAD".into()))?;
    if branch != environment.branch {
        return Err(CoderEntryError(format!(
            "Coder branch drifted: Forge expects '{}', found '{branch}'",
            environment.branch
        )));
    }

    let head_oid = forge
        .git()
        .head_oid(&worktree)
        .map_err(|err| CoderEntryError(format!("cannot resolve Coder HEAD: {err}")))?;
    let changed_paths = forge
        .git()
        .status_porcelain(&worktree)
        .map_err(|err| CoderEntryError(format!("cannot inspect Coder worktree: {err}")))?
        .into_iter()
        .map(|entry| truncate(&entry.path, MAX_PATH_CHARS))
        .take(MAX_CHANGED_PATHS)
        .collect();
    let editor = compile_editor_context(advisory);

    Ok(CoderEntryContext {
        work_id: item.id.to_string(),
        title: truncate(&item.title, MAX_FIELD_CHARS),
        brief: truncate(&item.brief, MAX_FIELD_CHARS),
        worktree: worktree.clone(),
        branch,
        baseline_oid: environment.baseline_oid.to_string(),
        head_oid: head_oid.to_string(),
        changed_paths,
        allowed_paths: bounded_paths(&item.policy.allowed_paths, MAX_POLICY_PATHS),
        denied_paths: bounded_paths(&item.policy.denied_paths, MAX_POLICY_PATHS),
        project_markers: discover_project_markers(&worktree),
        repository_instructions: discover_repository_instructions(
            &worktree,
            editor.active_path.as_deref(),
        ),
        editor,
    })
}

impl CoderEntryContext {
    /// Prompt appendix. All repository/editor prose is explicitly data, never
    /// higher-priority authority than the mode contract or Forge policy.
    pub fn prompt_appendix(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "<coder_world_state authority=\"forge\">");
        let _ = writeln!(out, "work_id: {}", self.work_id);
        let _ = writeln!(out, "worktree: {}", self.worktree.display());
        let _ = writeln!(out, "branch: {}", self.branch);
        let _ = writeln!(out, "baseline_oid: {}", self.baseline_oid);
        let _ = writeln!(out, "head_oid: {}", self.head_oid);
        let _ = writeln!(out, "title: {}", self.title);
        let _ = writeln!(out, "brief: {}", self.brief);
        let _ = writeln!(out, "changed_paths: {:?}", self.changed_paths);
        let _ = writeln!(out, "allowed_paths: {:?}", self.allowed_paths);
        let _ = writeln!(out, "denied_paths: {:?}", self.denied_paths);
        let _ = writeln!(out, "project_markers: {:?}", self.project_markers);
        let _ = writeln!(out, "</coder_world_state>");
        if !self.repository_instructions.is_empty() {
            out.push_str("\n<repository_instructions trust=\"untrusted_repository_data\">\n");
            for instruction in &self.repository_instructions {
                let _ = writeln!(out, "--- {} ---\n{}", instruction.path, instruction.content);
            }
            out.push_str("</repository_instructions>\n");
        }
        let _ = writeln!(
            out,
            "\n<editor_context trust=\"advisory_user_interface_data\">\n{:#?}\n</editor_context>",
            self.editor
        );
        out
    }
}

fn compile_editor_context(advisory: &CodeIntentContext) -> CoderEditorContext {
    CoderEditorContext {
        active_path: advisory
            .active_path
            .as_deref()
            .and_then(normalize_relative_path),
        cursor_line: advisory.cursor_line,
        selection_start_line: advisory.selection_start_line,
        selection_end_line: advisory.selection_end_line,
        selected_text: advisory
            .selected_text
            .as_deref()
            .map(|value| truncate(value, MAX_SELECTED_TEXT_CHARS)),
        containing_symbol: advisory
            .containing_symbol
            .as_deref()
            .map(|value| truncate(value, MAX_FIELD_CHARS)),
        open_files: advisory
            .open_files
            .iter()
            .filter_map(|path| normalize_relative_path(path))
            .take(MAX_OPEN_FILES)
            .collect(),
        diagnostics: advisory
            .diagnostics
            .iter()
            .map(|value| truncate(value, MAX_FIELD_CHARS))
            .take(MAX_DIAGNOSTICS)
            .collect(),
        last_verification: advisory
            .last_verification
            .as_deref()
            .map(|value| truncate(value, MAX_FIELD_CHARS)),
    }
}

fn normalize_relative_path(value: &str) -> Option<String> {
    let path = Path::new(value.trim());
    if path.as_os_str().is_empty() || path.is_absolute() {
        return None;
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    Some(path.to_string_lossy().replace('\\', "/"))
}

fn discover_project_markers(worktree: &Path) -> Vec<String> {
    const MARKERS: &[&str] = &[
        "Cargo.toml",
        "package.json",
        "pnpm-workspace.yaml",
        "go.mod",
        "pyproject.toml",
        "requirements.txt",
        "Gemfile",
        "pom.xml",
        "build.gradle",
        "Makefile",
    ];
    MARKERS
        .iter()
        .filter(|marker| worktree.join(marker).is_file())
        .map(|marker| (*marker).to_string())
        .collect()
}

fn bounded_paths(paths: &[String], max_paths: usize) -> Vec<String> {
    paths
        .iter()
        .map(|path| truncate(path, MAX_PATH_CHARS))
        .take(max_paths)
        .collect()
}

fn discover_repository_instructions(
    worktree: &Path,
    active_path: Option<&str>,
) -> Vec<RepositoryInstruction> {
    let mut directories = vec![PathBuf::new()];
    if let Some(active_path) = active_path {
        let parent = Path::new(active_path)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        let mut current = PathBuf::new();
        for component in parent.components() {
            current.push(component.as_os_str());
            directories.push(current.clone());
        }
    }
    let mut remaining = MAX_REPO_INSTRUCTIONS_CHARS;
    let mut out = Vec::new();
    for directory in directories {
        let relative = directory.join("AGENTS.md");
        let path = worktree.join(&relative);
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let content = truncate(&content, remaining);
        remaining = remaining.saturating_sub(content.chars().count());
        out.push(RepositoryInstruction {
            path: relative.to_string_lossy().replace('\\', "/"),
            content,
        });
        if remaining == 0 {
            break;
        }
    }
    out
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    value.chars().take(max_chars).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_forge::git::{CheckpointAuthor, GitEngine};
    use tempfile::TempDir;

    fn ready_work() -> (TempDir, TempDir, Forge, String) {
        let repo = TempDir::new().expect("repo tempdir");
        let forge_root = TempDir::new().expect("forge tempdir");
        let git = GitEngine::detect().expect("git");
        let initialized = std::process::Command::new("git")
            .args(["init", "-b", "main", "--template="])
            .current_dir(repo.path())
            .status()
            .expect("run git init");
        assert!(initialized.success());
        std::fs::write(
            repo.path().join("Cargo.toml"),
            "[package]\nname='demo'\nversion='0.1.0'\n",
        )
        .expect("manifest");
        std::fs::write(repo.path().join("AGENTS.md"), "Keep changes narrow.\n")
            .expect("instructions");
        let staged = std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo.path())
            .status()
            .expect("run git add");
        assert!(staged.success());
        git.commit_checkpoint(repo.path(), "initial", &CheckpointAuthor::default())
            .expect("commit");
        let forge = Forge::open(forge_root.path()).expect("forge");
        let item = forge
            .register(
                "Repair demo",
                "Fix the failing behavior",
                repo.path(),
                "main",
                "user-1",
                &Forge::system_actor(),
            )
            .expect("register");
        let item = forge
            .provision(&item.id, &Forge::system_actor())
            .expect("provision");
        (repo, forge_root, forge, item.id.to_string())
    }

    #[test]
    fn editor_paths_cannot_escape_the_forge_worktree() {
        let advisory = CodeIntentContext {
            active_path: Some("../../etc/passwd".into()),
            open_files: vec!["src/lib.rs".into(), "/tmp/escape".into()],
            ..CodeIntentContext::default()
        };
        let editor = compile_editor_context(&advisory);
        assert_eq!(editor.active_path, None);
        assert_eq!(editor.open_files, vec!["src/lib.rs"]);
    }

    #[test]
    fn editor_payloads_are_bounded() {
        let advisory = CodeIntentContext {
            selected_text: Some("x".repeat(MAX_SELECTED_TEXT_CHARS + 20)),
            diagnostics: (0..MAX_DIAGNOSTICS + 5)
                .map(|index| format!("diagnostic {index}"))
                .collect(),
            ..CodeIntentContext::default()
        };
        let editor = compile_editor_context(&advisory);
        assert_eq!(editor.diagnostics.len(), MAX_DIAGNOSTICS);
        assert_eq!(
            editor.selected_text.expect("selection").chars().count(),
            MAX_SELECTED_TEXT_CHARS + 1
        );
    }

    #[test]
    fn entry_uses_forge_authority_and_bounded_editor_observations() {
        let (_repo, _forge_root, forge, work_id) = ready_work();
        let entry = compile_coder_entry(
            &forge,
            &CodeIntentContext {
                work_id: Some(work_id.clone()),
                active_path: Some("src/lib.rs".into()),
                project_title: Some("spoofed title".into()),
                ..CodeIntentContext::default()
            },
        )
        .expect("compile Coder entry");

        assert_eq!(entry.work_id, work_id);
        assert_eq!(entry.title, "Repair demo");
        assert!(entry.branch.starts_with("medousa/work/"));
        assert_eq!(entry.project_markers, vec!["Cargo.toml"]);
        assert_eq!(entry.repository_instructions[0].path, "AGENTS.md");
        assert_eq!(entry.editor.active_path.as_deref(), Some("src/lib.rs"));
        assert!(entry.prompt_appendix().contains("authority=\"forge\""));
    }

    #[test]
    fn entry_rejects_missing_or_unknown_undertakings() {
        let (_repo, _forge_root, forge, _) = ready_work();
        let missing = compile_coder_entry(&forge, &CodeIntentContext::default())
            .expect_err("missing undertaking");
        assert!(missing.to_string().contains("requires a Forge undertaking"));

        let unknown = compile_coder_entry(
            &forge,
            &CodeIntentContext {
                work_id: Some("work-does-not-exist".into()),
                ..CodeIntentContext::default()
            },
        )
        .expect_err("unknown undertaking");
        assert!(unknown.to_string().contains("cannot enter Coder mode"));
    }
}
