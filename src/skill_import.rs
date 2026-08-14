//! Import AgentSkills-format `SKILL.md` files (Cursor, Hermes, OpenClaw) as Medousa manuscripts.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::identity_manuscript::{
    IdentityManuscriptFile, MANUSCRIPT_API_VERSION, MANUSCRIPT_KIND, ManuscriptMetadata,
    ManuscriptOpenshellSpec, ManuscriptPersonaSpec, ManuscriptPromptsSpec, ManuscriptScope,
    ManuscriptSpec, ManuscriptToolsSpec, build_manuscript_context, manuscript_storage_name,
    project_manuscripts_dir, user_manuscripts_dir, validate_manuscript,
};
use crate::skill_execution::{skill_has_runnable_scripts, skill_has_runnable_scripts_in_store};
use crate::store_root::{StoreEntryKind, StorePath, StoreRoot};

const MAX_SKILL_ID_LEN: usize = 64;
const MAX_SKILL_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_SKILL_ASSET_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SkillFrontmatter {
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

#[cfg(test)]
pub(crate) fn parse_skill_md(path: &Path) -> Result<(SkillFrontmatter, String)> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read SKILL.md {}", path.display()))?;
    parse_skill_text(&raw, path)
}

fn parse_skill_text(raw: &str, path: &Path) -> Result<(SkillFrontmatter, String)> {
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
        } else if matches!(lower, '-' | '_' | ' ') && !slug.ends_with('-') {
            slug.push('-');
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

pub(crate) fn build_manuscript_from_skill(
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

    let task_template = description
        .clone()
        .map(|desc| format!("Apply the {name} specialty.\n\nTrigger context: {desc}"));

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

fn copy_skill_tree(
    source_root: &StoreRoot,
    source_dir: Option<&StorePath>,
    target_root: &StoreRoot,
    target_dir: &StorePath,
) -> Result<()> {
    target_root.create_dir_all(target_dir)?;
    let entries = match source_dir {
        Some(path) => source_root.list_directory(path)?,
        None => source_root.list_root()?,
    };
    for entry in entries {
        let source_path = match source_dir {
            Some(parent) => parent.join(&entry.path)?,
            None => entry.path.clone(),
        };
        let target_path = target_dir.join(&entry.path)?;
        match entry.kind {
            StoreEntryKind::Directory => {
                copy_skill_tree(source_root, Some(&source_path), target_root, &target_path)?
            }
            StoreEntryKind::File => {
                target_root.atomic_copy_from(
                    &target_path,
                    source_root,
                    &source_path,
                    MAX_SKILL_ASSET_BYTES,
                )?;
            }
            StoreEntryKind::Link | StoreEntryKind::Other => {
                bail!("skill import rejects link-backed or special entries")
            }
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
    let skill_dir = if skill_dir.is_absolute() {
        skill_dir
    } else {
        std::env::current_dir()?.join(skill_dir)
    };
    let source_root = StoreRoot::open_nofollow(&skill_dir)
        .with_context(|| format!("open skill source {}", skill_dir.display()))?;
    let skill_md = skill_dir.join("SKILL.md");
    let skill_md_relative = StorePath::parse("SKILL.md")?;
    let skill_md_bytes = source_root
        .read_limited(&skill_md_relative, MAX_SKILL_MANIFEST_BYTES)
        .with_context(|| format!("read SKILL.md {}", skill_md.display()))?;
    let skill_md_text = std::str::from_utf8(&skill_md_bytes).context("SKILL.md is not UTF-8")?;
    let (frontmatter, _) = parse_skill_text(skill_md_text, &skill_md)?;

    let id = sanitize_skill_id(frontmatter.name.as_deref().unwrap_or(""), &skill_dir);
    let storage_name = manuscript_storage_name(&id)?;
    let target_root = match scope {
        ManuscriptScope::Project => project_manuscripts_dir(),
        ManuscriptScope::User => user_manuscripts_dir(),
    };
    let target_store = StoreRoot::open_or_create_nofollow(&target_root)
        .with_context(|| format!("open manuscript dir {}", target_root.display()))?;

    let yaml_relative = StorePath::parse(&format!("{storage_name}.yaml"))?;
    let assets_relative = StorePath::parse(&storage_name)?;
    let yaml_path = target_root.join(yaml_relative.file_name());
    let assets_dir = target_root.join(assets_relative.file_name());
    if target_store.is_file(&yaml_relative)? || target_store.is_dir(&assets_relative)? {
        if !force {
            bail!("specialty '{id}' already exists; pass --force to replace",);
        }
        if target_store.is_file(&yaml_relative)? {
            target_store.remove_file(&yaml_relative)?;
        }
        if target_store.is_dir(&assets_relative)? {
            target_store.remove_dir_all(&assets_relative)?;
        }
    }

    copy_skill_tree(&source_root, None, &target_store, &assets_relative)?;

    let mut manuscript = build_manuscript_from_skill(&id, &frontmatter, &storage_name, extends);
    apply_skill_sandbox_defaults_for_scripts(
        &mut manuscript,
        skill_has_runnable_scripts_in_store(&source_root),
    );
    validate_manuscript(&manuscript, &yaml_path)?;

    let yaml = serde_yaml::to_string(&manuscript).context("encode imported manuscript yaml")?;
    target_store
        .atomic_write(&yaml_relative, yaml.as_bytes())
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
    apply_skill_sandbox_defaults_for_scripts(manuscript, skill_has_runnable_scripts(skill_dir));
}

fn apply_skill_sandbox_defaults_for_scripts(
    manuscript: &mut IdentityManuscriptFile,
    has_runnable_scripts: bool,
) {
    if !has_runnable_scripts {
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
        assert_eq!(
            sanitize_skill_id("My_Cool Skill!!", Path::new(".")),
            "my-cool-skill"
        );
    }

    #[test]
    fn parse_skill_md_reads_frontmatter_and_body() {
        let dir = std::env::temp_dir().join(format!("medousa-skill-parse-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write_sample_skill(&dir, "parse-me");
        let (frontmatter, body) = parse_skill_md(&dir.join("SKILL.md")).expect("parse");
        assert_eq!(frontmatter.name.as_deref(), Some("parse-me"));
        assert!(body.contains("When to Use"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_skill_creates_manuscript_and_assets() {
        let base =
            std::env::temp_dir().join(format!("medousa-skill-import-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let source = base.join("source");
        write_sample_skill(&source, "invoice-helper");
        let source = source.canonicalize().expect("canonical source");

        let manuscripts = base.join("manuscripts");
        fs::create_dir_all(&manuscripts).expect("dir");
        let manuscripts = manuscripts.canonicalize().expect("canonical manuscripts");

        let original_user = user_manuscripts_dir();
        // We cannot easily override user_manuscripts_dir in tests without refactoring.
        // Import directly by mimicking install layout instead.
        let skill_dir = resolve_skill_source(&source).expect("resolve");
        let (frontmatter, _) = parse_skill_md(&skill_dir.join("SKILL.md")).expect("parse");
        let id = sanitize_skill_id("invoice-helper", &skill_dir);
        let storage_name = manuscript_storage_name(&id).unwrap();
        let manuscript =
            build_manuscript_from_skill(&id, &frontmatter, &storage_name, Some("base-researcher"));
        let yaml_path = manuscripts.join(format!("{storage_name}.yaml"));
        let assets_dir = manuscripts.join(&storage_name);
        let source_store = StoreRoot::open_nofollow(&skill_dir).expect("source store");
        let target_store = StoreRoot::open_nofollow(&manuscripts).expect("target store");
        let assets_relative = StorePath::parse(&storage_name).expect("assets path");
        copy_skill_tree(&source_store, None, &target_store, &assets_relative).expect("copy");
        let yaml = serde_yaml::to_string(&manuscript).expect("yaml");
        target_store
            .atomic_write(
                &StorePath::parse(&format!("{storage_name}.yaml")).expect("yaml path"),
                yaml.as_bytes(),
            )
            .expect("write yaml");

        assert!(yaml_path.is_file());
        assert!(assets_dir.join("SKILL.md").is_file());
        assert!(assets_dir.join("references/api.md").is_file());
        let loaded = fs::read_to_string(&yaml_path).expect("read yaml");
        assert!(loaded.contains(&format!("soul_md: ./{storage_name}/SKILL.md")));
        assert!(loaded.contains("extends: base-researcher"));
        let _ = fs::remove_dir_all(&base);
        let _ = original_user;
    }

    #[test]
    fn discover_skill_dirs_finds_nested_skills() {
        let base =
            std::env::temp_dir().join(format!("medousa-skill-discover-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        write_sample_skill(&base.join("alpha"), "alpha");
        write_sample_skill(&base.join("group").join("beta"), "beta");
        let found = discover_skill_dirs(&base).expect("discover");
        assert_eq!(found.len(), 2);
        let _ = fs::remove_dir_all(&base);
    }
}
