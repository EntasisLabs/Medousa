//! Language → server launch registry (Neovim-style defaults).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LanguageId(String);

impl LanguageId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into().to_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for LanguageId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for LanguageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerLaunchSpec {
    pub language: LanguageId,
    /// How to start the server.
    pub kind: ServerKind,
    /// Files that identify a project root (first match walking up).
    #[serde(default)]
    pub root_markers: Vec<String>,
    /// Extra argv after the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// File extensions associated with this language (no leading dot).
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Optional Settings → Packages id that Repair should install.
    #[serde(default)]
    pub package_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerKind {
    /// In-process Grapheme language server.
    Grapheme,
    /// External stdio language server binary.
    Stdio { command: String },
}

impl ServerKind {
    pub fn command_name(&self) -> Option<&str> {
        match self {
            ServerKind::Grapheme => None,
            ServerKind::Stdio { command } => Some(command.as_str()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ServerRegistry {
    specs: Vec<ServerLaunchSpec>,
}

fn stdio_spec(
    language: &str,
    command: &str,
    root_markers: &[&str],
    args: &[&str],
    extensions: &[&str],
    package_id: Option<&str>,
) -> ServerLaunchSpec {
    ServerLaunchSpec {
        language: LanguageId::new(language),
        kind: ServerKind::Stdio {
            command: command.into(),
        },
        root_markers: root_markers.iter().map(|marker| (*marker).into()).collect(),
        args: args.iter().map(|arg| (*arg).into()).collect(),
        extensions: extensions.iter().map(|ext| (*ext).into()).collect(),
        package_id: package_id.map(str::to_string),
    }
}

impl ServerRegistry {
    pub fn with_defaults() -> Self {
        let mut reg = Self::default();
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("grapheme"),
            kind: ServerKind::Grapheme,
            root_markers: vec![],
            args: vec![],
            extensions: vec!["grapheme".into(), "gr".into()],
            package_id: None,
        });
        reg.register(stdio_spec(
            "python",
            "pyright-langserver",
            &["pyproject.toml", "setup.py", "requirements.txt"],
            &["--stdio"],
            &["py"],
            Some("langservers"),
        ));
        reg.register(stdio_spec(
            "typescript",
            "typescript-language-server",
            &["package.json", "tsconfig.json"],
            &["--stdio"],
            &["ts", "tsx"],
            Some("langservers"),
        ));
        reg.register(stdio_spec(
            "javascript",
            "typescript-language-server",
            &["package.json", "jsconfig.json"],
            &["--stdio"],
            &["js", "jsx", "mjs"],
            Some("langservers"),
        ));
        reg.register(stdio_spec(
            "svelte",
            "svelteserver",
            &[
                "svelte.config.js",
                "svelte.config.ts",
                "svelte.config.mjs",
                "package.json",
            ],
            &["--stdio"],
            &["svelte"],
            Some("langservers"),
        ));
        reg.register(stdio_spec(
            "rust",
            "rust-analyzer",
            &["Cargo.toml"],
            &[],
            &["rs"],
            None,
        ));
        reg.register(stdio_spec(
            "go",
            "gopls",
            &["go.work", "go.mod"],
            &[],
            &["go"],
            None,
        ));
        for language in ["c", "cpp"] {
            let extensions: &[&str] = if language == "c" {
                &["c", "h"]
            } else {
                &["cc", "cpp", "cxx", "hh", "hpp", "hxx"]
            };
            reg.register(stdio_spec(
                language,
                "clangd",
                &["compile_commands.json", "compile_flags.txt", ".clangd"],
                &[],
                extensions,
                None,
            ));
        }
        reg.register(stdio_spec(
            "csharp",
            "omnisharp",
            &["*.sln", "*.csproj"],
            &["-lsp"],
            &["cs"],
            None,
        ));
        reg.register(stdio_spec(
            "java",
            "jdtls",
            &["pom.xml", "build.gradle", "build.gradle.kts"],
            &[],
            &["java"],
            None,
        ));
        reg.register(stdio_spec(
            "kotlin",
            "kotlin-language-server",
            &["settings.gradle", "settings.gradle.kts"],
            &[],
            &["kt", "kts"],
            None,
        ));
        reg.register(stdio_spec(
            "ruby",
            "solargraph",
            &["Gemfile", ".ruby-version"],
            &["stdio"],
            &["rb"],
            None,
        ));
        reg.register(stdio_spec(
            "php",
            "intelephense",
            &["composer.json"],
            &["--stdio"],
            &["php"],
            None,
        ));
        reg.register(stdio_spec(
            "swift",
            "sourcekit-lsp",
            &["Package.swift"],
            &[],
            &["swift"],
            None,
        ));
        reg.register(stdio_spec(
            "lua",
            "lua-language-server",
            &[".luarc.json", ".luarc.jsonc"],
            &[],
            &["lua"],
            None,
        ));
        reg
    }

    pub fn register(&mut self, spec: ServerLaunchSpec) {
        self.specs.retain(|s| s.language != spec.language);
        self.specs.push(spec);
    }

    pub fn get(&self, language: &LanguageId) -> Option<&ServerLaunchSpec> {
        self.specs.iter().find(|s| &s.language == language)
    }

    pub fn languages(&self) -> impl Iterator<Item = &LanguageId> {
        self.specs.iter().map(|s| &s.language)
    }

    pub fn specs(&self) -> &[ServerLaunchSpec] {
        &self.specs
    }

    /// Walk parents from `path` looking for the closest root marker without
    /// ever escaping the canonical governed project root in `fallback`.
    pub fn resolve_root(&self, language: &LanguageId, path: &Path, fallback: &Path) -> PathBuf {
        let fallback = fallback
            .canonicalize()
            .unwrap_or_else(|_| fallback.to_path_buf());
        let Some(spec) = self.get(language) else {
            return fallback;
        };
        if spec.root_markers.is_empty() {
            return fallback;
        }
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !path.starts_with(&fallback) {
            return fallback;
        }
        let mut cur = path;
        if cur.is_file() {
            cur = cur
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| fallback.clone());
        }
        while cur.starts_with(&fallback) {
            for marker in &spec.root_markers {
                if directory_has_marker(&cur, marker) {
                    return cur;
                }
            }
            if cur == fallback || !cur.pop() {
                break;
            }
        }
        fallback
    }
}

fn directory_has_marker(directory: &Path, marker: &str) -> bool {
    let Some(suffix) = marker.strip_prefix('*') else {
        return directory.join(marker).exists();
    };
    std::fs::read_dir(directory).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.ends_with(suffix))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_include_grapheme_and_common_langs() {
        let reg = ServerRegistry::with_defaults();
        assert!(reg.get(&LanguageId::new("grapheme")).is_some());
        assert!(reg.get(&LanguageId::new("python")).is_some());
        assert!(reg.get(&LanguageId::new("svelte")).is_some());
        assert!(reg.get(&LanguageId::new("rust")).is_some());
        assert_eq!(
            reg.get(&LanguageId::new("svelte"))
                .and_then(|spec| spec.package_id.as_deref()),
            Some("langservers")
        );
    }

    #[test]
    fn resolve_root_prefers_svelte_config_over_outer_package() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("apps/medousa-home");
        let source = package.join("src");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::write(package.join("package.json"), "{}").unwrap();
        std::fs::write(package.join("svelte.config.js"), "export default {};").unwrap();
        let file = source.join("App.svelte");
        std::fs::write(
            &file,
            "<script lang=\"ts\">export let name = \"Medousa\";</script>",
        )
        .unwrap();

        let root = ServerRegistry::with_defaults().resolve_root(
            &LanguageId::new("svelte"),
            &file,
            dir.path(),
        );
        assert_eq!(root, package.canonicalize().unwrap());
    }

    #[test]
    fn resolve_root_finds_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("src");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        let file = nested.join("main.rs");
        std::fs::write(&file, "fn main() {}").unwrap();
        let reg = ServerRegistry::with_defaults();
        let root = reg.resolve_root(&LanguageId::new("rust"), &file, dir.path());
        assert_eq!(root, dir.path().canonicalize().unwrap());
    }

    #[test]
    fn resolve_root_prefers_the_closest_nested_project() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("packages/app");
        let source = package.join("src");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::write(package.join("package.json"), "{}").unwrap();
        let file = source.join("main.ts");
        std::fs::write(&file, "export {};").unwrap();

        let root = ServerRegistry::with_defaults().resolve_root(
            &LanguageId::new("typescript"),
            &file,
            dir.path(),
        );
        assert_eq!(root, package.canonicalize().unwrap());
    }

    #[test]
    fn resolve_root_never_uses_a_marker_above_the_governed_project() {
        let dir = tempfile::tempdir().unwrap();
        let outer = dir.path().join("outer");
        let project = outer.join("project");
        let nested = project.join("src");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(outer.join("Cargo.toml"), "[package]\n").unwrap();
        let file = nested.join("main.rs");
        std::fs::write(&file, "fn main() {}").unwrap();
        let project = project.canonicalize().unwrap();
        let root =
            ServerRegistry::with_defaults().resolve_root(&LanguageId::new("rust"), &file, &project);
        assert_eq!(root, project);
    }

    #[test]
    fn resolve_root_supports_extension_markers() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("App.csproj"), "<Project></Project>").unwrap();
        let file = project.join("Program.cs");
        std::fs::write(&file, "class Program {}").unwrap();
        let root = ServerRegistry::with_defaults().resolve_root(
            &LanguageId::new("csharp"),
            &file,
            &project,
        );
        assert_eq!(root, project.canonicalize().unwrap());
    }
}
