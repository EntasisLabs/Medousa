//! Deterministic vault fixture generators for H07.0 / P06.
//!
//! Ordinary tests use small shapes. Large note counts are driven by
//! `MEDOUSA_P06_NOTES` in the retained/nightly harness only — corpora are
//! never checked into the tree.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Fixture topology shapes used by baselines and unit tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultFixtureShape {
    Shallow,
    Deep,
    Wide,
    LinkHeavy,
}

impl VaultFixtureShape {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shallow => "shallow",
            Self::Deep => "deep",
            Self::Wide => "wide",
            Self::LinkHeavy => "link_heavy",
        }
    }
}

#[derive(Debug, Clone)]
pub struct VaultFixtureSpec {
    pub shape: VaultFixtureShape,
    pub note_count: usize,
    pub body_bytes: usize,
}

impl VaultFixtureSpec {
    pub fn small(shape: VaultFixtureShape) -> Self {
        let note_count = match shape {
            VaultFixtureShape::Shallow => 24,
            VaultFixtureShape::Deep => 16,
            VaultFixtureShape::Wide => 32,
            VaultFixtureShape::LinkHeavy => 20,
        };
        Self {
            shape,
            note_count,
            body_bytes: 96,
        }
    }

    pub fn scaled(shape: VaultFixtureShape, note_count: usize) -> Self {
        Self {
            shape,
            note_count: note_count.max(1),
            body_bytes: 96,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GeneratedVaultFixture {
    pub root: PathBuf,
    pub note_paths: Vec<String>,
    pub shape: VaultFixtureShape,
}

/// Materialize a deterministic markdown vault under `root`.
pub fn generate_vault_fixture(
    root: &Path,
    spec: &VaultFixtureSpec,
) -> Result<GeneratedVaultFixture> {
    fs::create_dir_all(root).with_context(|| format!("create fixture root {}", root.display()))?;
    let mut note_paths = Vec::with_capacity(spec.note_count);
    for index in 0..spec.note_count {
        let relative = note_path_for(spec.shape, index, spec.note_count);
        let absolute = root.join(&relative);
        if let Some(parent) = absolute.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create parent {}", parent.display()))?;
        }
        let body = note_body(spec, index, &relative, &note_paths);
        fs::write(&absolute, body.as_bytes())
            .with_context(|| format!("write fixture note {}", absolute.display()))?;
        note_paths.push(relative.replace('\\', "/"));
    }
    Ok(GeneratedVaultFixture {
        root: root.to_path_buf(),
        note_paths,
        shape: spec.shape,
    })
}

fn note_path_for(shape: VaultFixtureShape, index: usize, total: usize) -> String {
    match shape {
        VaultFixtureShape::Shallow => format!("note-{index:04}.md"),
        VaultFixtureShape::Deep => {
            let depth = (index % 8) + 1;
            let mut parts = Vec::with_capacity(depth + 1);
            for level in 0..depth {
                parts.push(format!("d{level}"));
            }
            parts.push(format!("leaf-{index:04}.md"));
            parts.join("/")
        }
        VaultFixtureShape::Wide => {
            let bucket = index % 8;
            format!("bucket-{bucket}/note-{index:04}.md")
        }
        VaultFixtureShape::LinkHeavy => {
            let hub = if index == 0 {
                "hub.md".to_string()
            } else if index < total.saturating_div(2).max(1) {
                format!("spokes/spoke-{index:04}.md")
            } else {
                format!("leaves/leaf-{index:04}.md")
            };
            hub
        }
    }
}

fn note_body(spec: &VaultFixtureSpec, index: usize, relative: &str, existing: &[String]) -> String {
    let title = relative
        .rsplit('/')
        .next()
        .unwrap_or(relative)
        .trim_end_matches(".md");
    let mut body = format!(
        "# {title}\n\nindex={index} shape={}\n\n",
        spec.shape.as_str()
    );
    if matches!(spec.shape, VaultFixtureShape::LinkHeavy) && index > 0 {
        if let Some(hub) = existing.first() {
            let hub_stem = hub.trim_end_matches(".md");
            body.push_str(&format!("See [[{hub_stem}]] and [[missing-{index}]].\n\n"));
        }
        if let Some(prev) = existing.last() {
            let stem = prev
                .rsplit('/')
                .next()
                .unwrap_or(prev)
                .trim_end_matches(".md");
            body.push_str(&format!("Also [[{stem}]].\n\n"));
        }
    }
    while body.len() < spec.body_bytes {
        body.push_str("pad ");
    }
    body.truncate(spec.body_bytes.max(body.len().min(spec.body_bytes + 32)));
    body.push('\n');
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn small_fixtures_are_deterministic_and_bounded() {
        for shape in [
            VaultFixtureShape::Shallow,
            VaultFixtureShape::Deep,
            VaultFixtureShape::Wide,
            VaultFixtureShape::LinkHeavy,
        ] {
            let dir = tempdir().unwrap();
            let spec = VaultFixtureSpec::small(shape);
            let first = generate_vault_fixture(dir.path().join("a").as_path(), &spec).unwrap();
            let second = generate_vault_fixture(dir.path().join("b").as_path(), &spec).unwrap();
            assert_eq!(first.note_paths, second.note_paths);
            assert_eq!(first.note_paths.len(), spec.note_count);
            assert!(first.note_paths.iter().all(|path| path.ends_with(".md")));
        }
    }
}
