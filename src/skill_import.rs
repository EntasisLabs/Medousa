//! Import AgentSkills-format `SKILL.md` files (Cursor, Hermes, OpenClaw) as Medousa manuscripts.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::identity_manuscript::{
    IdentityManuscriptFile, ManuscriptMetadata, ManuscriptOpenshellSpec, ManuscriptPersonaSpec,
    ManuscriptPromptsSpec, ManuscriptScope, ManuscriptSpec, ManuscriptToolsSpec,
    MANUSCRIPT_API_VERSION, MANUSCRIPT_KIND, build_manuscript_context, project_manuscripts_dir,
    user_manuscripts_dir, validate_manuscript,
};
use crate::skill_execution::skill_has_runnable_scripts;

const MAX_SKILL_ID_LEN: usize = 64;

#[derive(Debug, Clone, Deserialize)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SkillImportResult {
    pub id: String,
    pub name: String,
    pub yaml_path: PathBuf,
    pub skill_assets_dir: PathBuf,
    pub source: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillImportPreset {
    Hermes,
    OpenClaw,
    Cursor,
}

pub fn preset_skill_roots(preset: SkillImportPreset) -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    match preset {
        SkillImportPreset::Hermes => vec![home.join(".hermes").join("skills")],
        SkillImportPreset::OpenClaw => vec![
            home.join(".openclaw").join("skills"),
            home.join(".openclaw").join("workspace").join("skills"),
            home.join(".agents").join("skills"),
        ],
        SkillImportPreset::Cursor => vec![home.join(".cursor").join("skills")],
    }
}

pub fn project_cursor_skills_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".cursor")
        .join("skills")
}

pub fn discover_skill_dirs(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut found = Vec::new();
    let mut seen = HashSet::new();
    discover_skill_dirs_inner(root, &mut found, &mut seen)?;
    found.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    Ok(found)
}

fn discover_skill_dirs_inner(
    root: &Path,
    found: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
) -> Result<()> {
    let skill_md = root.join("SKILL.md");
    if skill_md.is_file() {
        let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        if seen.insert(canonical.clone()) {
            found.push(root.to_path_buf());
        }
        return Ok(());
    }

    for entry in std::fs::read_dir(root).with_context(|| format!("read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        discover_skill_dirs_inner(&path, found, seen)?;
    }
    Ok(())
}

pub fn resolve_skill_source(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if file_name.eq_ignore_ascii_case("SKILL.md") {
            return path
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| anyhow::anyhow!("SKILL.md has no parent directory"));
        }
        bail!(
            "expected a skill directory or SKILL.md file, got {}",
            path.display()
        );
    }

    if path.join("SKILL.md").is_file() {
        return Ok(path.to_path_buf());
    }

    if path.is_dir() {
        let discovered = discover_skill_dirs(path)?;
        if discovered.len() == 1 {
            return Ok(discovered[0].clone());
        }
        if discovered.is_empty() {
            bail!("no SKILL.md found under {}", path.display());
        }
        bail!(
            "found {} skills under {}; pass a single skill directory or run bulk import",
            discovered.len(),
            path.display()
        );
    }

    bail!("skill path does not exist: {}", path.display());
}

pub fn parse_skill_md(path: &Path) -> Result<(SkillFrontmatter, String)> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read SKILL.md {}", path.display()))?;
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        bail!("SKILL.md must begin with YAML frontmatter (---)");
    }

    let rest = trimmed.strip_prefix("---").unwrap_or(trimmed);
    let rest = rest.trim_start_matches(['\r', '\n']);
    let end = rest
        .find("\n---")
        .ok_or_else(|| anyhow::anyhow!("SKILL.md frontmatter is not closed with ---"))?;
    let frontmatter_raw = &rest[..end];
    let body = rest[end + 4..].trim_start_matches(['\r', '\n']).to_string();

    let frontmatter: SkillFrontmatter = serde_yaml::from_str(frontmatter_raw)
        .with_context(|| format!("parse SKILL.md frontmatter in {}", path.display()))?;
    if body.trim().is_empty() {
        bail!("SKILL.md body is empty in {}", path.display());
    }
    Ok((frontmatter, body))
}

pub fn sanitize_skill_id(raw: &str, fallback_dir: &Path) -> String {
    let source = if raw.trim().is_empty() {
        fallback_dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("skill")
    } else {
        raw.trim()
    };

    let mut slug = String::new();
    for ch in source.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            slug.push(lower);
        } else if matches!(lower, '-' | '_' | ' ') {
            if !slug.ends_with('-') {
                slug.push('-');
            }
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("skill");
    }
    if slug.len() > MAX_SKILL_ID_LEN {
        slug.truncate(MAX_SKILL_ID_LEN);
        while slug.ends_with('-') {
            slug.pop();
        }
    }
    slug
}

fn title_from_id(id: &str) -> String {
    id.split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let mut title = first.to_ascii_uppercase().to_string();
                    title.push_str(chars.as_str());
                    title
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn build_manuscript_from_skill(
    id: &str,
    frontmatter: &SkillFrontmatter,
    skill_dir_name: &str,
    extends: Option<&str>,
) -> IdentityManuscriptFile {
    let name = frontmatter
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(title_from_id)
        .unwrap_or_else(|| title_from_id(id));
    let description = frontmatter
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let task_template = description.clone().map(|desc| {
        format!("Apply the {name} specialty.\n\nTrigger context: {desc}")
    });

    IdentityManuscriptFile {
        api_version: MANUSCRIPT_API_VERSION.to_string(),
        kind: MANUSCRIPT_KIND.to_string(),
        metadata: ManuscriptMetadata {
            id: id.to_string(),
            name,
            description,
            extends: extends.map(str::to_string),
        },
        spec: ManuscriptSpec {
            persona: ManuscriptPersonaSpec {
                display_name: None,
                voice_appendix: None,
                soul_md: Some(format!("./{skill_dir_name}/SKILL.md")),
            },
            prompts: ManuscriptPromptsSpec {
                system_appendix_sttp: None,
                task_template,
            },
            ..Default::default()
        },
    }
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)
        .with_context(|| format!("create {}", target.display()))?;
    for entry in std::fs::read_dir(source)
        .with_context(|| format!("read {}", source.display()))?
    {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = target.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "copy {} -> {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }
    Ok(())
}

pub fn import_skill(
    source: &Path,
    scope: ManuscriptScope,
    force: bool,
    extends: Option<&str>,
) -> Result<SkillImportResult> {
    let skill_dir = resolve_skill_source(source)?;
    let skill_md = skill_dir.join("SKILL.md");
    let (frontmatter, _) = parse_skill_md(&skill_md)?;

    let id = sanitize_skill_id(
        frontmatter.name.as_deref().unwrap_or(""),
        &skill_dir,
    );
    let target_root = match scope {
        ManuscriptScope::Project => project_manuscripts_dir(),
        ManuscriptScope::User => user_manuscripts_dir(),
    };
    std::fs::create_dir_all(&target_root)
        .with_context(|| format!("create manuscript dir {}", target_root.display()))?;

    let yaml_path = target_root.join(format!("{id}.yaml"));
    let assets_dir = target_root.join(&id);
    if yaml_path.exists() || assets_dir.exists() {
        if !force {
            bail!(
                "specialty '{id}' already exists; pass --force to replace",
            );
        }
        if yaml_path.is_file() {
            std::fs::remove_file(&yaml_path)?;
        }
        if assets_dir.is_dir() {
            std::fs::remove_dir_all(&assets_dir)?;
        }
    }

    copy_dir_recursive(&skill_dir, &assets_dir)?;

    let mut manuscript = build_manuscript_from_skill(&id, &frontmatter, &id, extends);
    apply_skill_sandbox_defaults(&mut manuscript, &skill_dir);
    validate_manuscript(&manuscript, &yaml_path)?;

    let yaml = serde_yaml::to_string(&manuscript).context("encode imported manuscript yaml")?;
    std::fs::write(&yaml_path, yaml)
        .with_context(|| format!("write manuscript {}", yaml_path.display()))?;

    let _ = build_manuscript_context(&id)?;

    Ok(SkillImportResult {
        id: id.clone(),
        name: manuscript.metadata.name.clone(),
        yaml_path,
        skill_assets_dir: assets_dir,
        source: skill_dir,
    })
}

/// When a skill ships runnable scripts, enable OpenShell sandbox defaults on the manuscript.
pub fn apply_skill_sandbox_defaults(manuscript: &mut IdentityManuscriptFile, skill_dir: &Path) {
    if !skill_has_runnable_scripts(skill_dir) {
        return;
    }
    manuscript.spec.openshell = ManuscriptOpenshellSpec {
        enabled: true,
        policy_template: Some("skill-sandbox".to_string()),
        sandbox_from: Some("medousa-openshell-sandbox:local".to_string()),
        allow_scheduled: false,
    };
    let mut allow = manuscript.spec.tools.allow.clone();
    for tool in [
        "cognition_skill_discover",
        "cognition_skill_propose",
        "cognition_skill_probe",
        "cognition_openshell_status",
        "cognition_openshell_sandbox_run",
    ] {
        if !allow.iter().any(|existing| existing == tool) {
            allow.push(tool.to_string());
        }
    }
    manuscript.spec.tools = ManuscriptToolsSpec { allow };
}

pub fn import_skills_from_roots(
    roots: &[PathBuf],
    scope: ManuscriptScope,
    force: bool,
    extends: Option<&str>,
) -> Result<Vec<SkillImportResult>> {
    let mut results = Vec::new();
    let mut errors = Vec::new();

    for root in roots {
        if !root.is_dir() {
            continue;
        }
        for skill_dir in discover_skill_dirs(root)? {
            match import_skill(&skill_dir, scope, force, extends) {
                Ok(result) => results.push(result),
                Err(error) => errors.push(format!("{}: {error:#}", skill_dir.display())),
            }
        }
    }

    if results.is_empty() && errors.is_empty() {
        bail!("no SKILL.md files found in the provided paths");
    }
    if results.is_empty() && !errors.is_empty() {
        bail!("skill import failed:\n{}", errors.join("\n"));
    }
    if !errors.is_empty() {
        eprintln!("medousa: some skills failed to import:");
        for error in &errors {
            eprintln!("  {error}");
        }
    }
    Ok(results)
}

pub fn import_skills_at_path(
    path: &Path,
    scope: ManuscriptScope,
    force: bool,
    extends: Option<&str>,
) -> Result<Vec<SkillImportResult>> {
    if path.is_file() || path.join("SKILL.md").is_file() {
        return Ok(vec![import_skill(path, scope, force, extends)?]);
    }
    import_skills_from_roots(&[path.to_path_buf()], scope, force, extends)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_sample_skill(dir: &Path, name: &str) -> PathBuf {
        fs::create_dir_all(dir.join("references")).expect("dir");
        fs::write(dir.join("references/api.md"), "api docs").expect("write");
        let skill_md = dir.join("SKILL.md");
        fs::write(
            &skill_md,
            format!(
                r#"---
name: {name}
description: Use when testing skill import.
version: 1.0.0
---

# Test Skill

## When to Use
When tests run.
"#
            ),
        )
        .expect("write skill");
        dir.to_path_buf()
    }

    #[test]
    fn sanitize_skill_id_normalizes_names() {
        assert_eq!(sanitize_skill_id("My_Cool Skill!!", Path::new(".")), "my-cool-skill");
    }

    #[test]
    fn parse_skill_md_reads_frontmatter_and_body() {
        let dir = std::env::temp_dir().join(format!(
            "medousa-skill-parse-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        write_sample_skill(&dir, "parse-me");
        let (frontmatter, body) = parse_skill_md(&dir.join("SKILL.md")).expect("parse");
        assert_eq!(frontmatter.name.as_deref(), Some("parse-me"));
        assert!(body.contains("When to Use"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_skill_creates_manuscript_and_assets() {
        let base = std::env::temp_dir().join(format!(
            "medousa-skill-import-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let source = base.join("source");
        write_sample_skill(&source, "invoice-helper");

        let manuscripts = base.join("manuscripts");
        fs::create_dir_all(&manuscripts).expect("dir");

        let original_user = user_manuscripts_dir();
        // We cannot easily override user_manuscripts_dir in tests without refactoring.
        // Import directly by mimicking install layout instead.
        let skill_dir = resolve_skill_source(&source).expect("resolve");
        let (frontmatter, _) = parse_skill_md(&skill_dir.join("SKILL.md")).expect("parse");
        let id = sanitize_skill_id("invoice-helper", &skill_dir);
        let manuscript = build_manuscript_from_skill(&id, &frontmatter, &id, Some("base-researcher"));
        let yaml_path = manuscripts.join(format!("{id}.yaml"));
        let assets_dir = manuscripts.join(&id);
        copy_dir_recursive(&skill_dir, &assets_dir).expect("copy");
        let yaml = serde_yaml::to_string(&manuscript).expect("yaml");
        fs::write(&yaml_path, yaml).expect("write yaml");

        assert!(yaml_path.is_file());
        assert!(assets_dir.join("SKILL.md").is_file());
        assert!(assets_dir.join("references/api.md").is_file());
        let loaded = fs::read_to_string(&yaml_path).expect("read yaml");
        assert!(loaded.contains("soul_md: ./invoice-helper/SKILL.md"));
        assert!(loaded.contains("extends: base-researcher"));
        let _ = fs::remove_dir_all(&base);
        let _ = original_user;
    }

    #[test]
    fn discover_skill_dirs_finds_nested_skills() {
        let base = std::env::temp_dir().join(format!(
            "medousa-skill-discover-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        write_sample_skill(&base.join("alpha"), "alpha");
        write_sample_skill(&base.join("group").join("beta"), "beta");
        let found = discover_skill_dirs(&base).expect("discover");
        assert_eq!(found.len(), 2);
        let _ = fs::remove_dir_all(&base);
    }
}
