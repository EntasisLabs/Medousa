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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerKind {
    /// In-process Grapheme language server.
    Grapheme,
    /// External stdio language server binary.
    Stdio { command: String },
}

#[derive(Debug, Clone, Default)]
pub struct ServerRegistry {
    specs: Vec<ServerLaunchSpec>,
}

impl ServerRegistry {
    pub fn with_defaults() -> Self {
        let mut reg = Self::default();
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("grapheme"),
            kind: ServerKind::Grapheme,
            root_markers: vec![],
            args: vec![],
        });
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("python"),
            kind: ServerKind::Stdio {
                command: "pyright-langserver".into(),
            },
            root_markers: vec![
                "pyproject.toml".into(),
                "setup.py".into(),
                "requirements.txt".into(),
            ],
            args: vec!["--stdio".into()],
        });
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("typescript"),
            kind: ServerKind::Stdio {
                command: "typescript-language-server".into(),
            },
            root_markers: vec!["package.json".into(), "tsconfig.json".into()],
            args: vec!["--stdio".into()],
        });
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("javascript"),
            kind: ServerKind::Stdio {
                command: "typescript-language-server".into(),
            },
            root_markers: vec!["package.json".into(), "jsconfig.json".into()],
            args: vec!["--stdio".into()],
        });
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("svelte"),
            kind: ServerKind::Stdio {
                command: "svelteserver".into(),
            },
            root_markers: vec![
                "svelte.config.js".into(),
                "svelte.config.ts".into(),
                "svelte.config.mjs".into(),
                "package.json".into(),
            ],
            args: vec!["--stdio".into()],
        });
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("rust"),
            kind: ServerKind::Stdio {
                command: "rust-analyzer".into(),
            },
            root_markers: vec!["Cargo.toml".into()],
            args: vec![],
        });

        reg.register(ServerLaunchSpec {
            language: LanguageId::new("go"),
            kind: ServerKind::Stdio {
                command: "gopls".into(),
            },
            root_markers: vec!["go.work".into(), "go.mod".into()],
            args: vec![],
        });
        for language in ["c", "cpp"] {
            reg.register(ServerLaunchSpec {
                language: LanguageId::new(language),
                kind: ServerKind::Stdio {
                    command: "clangd".into(),
                },
                root_markers: vec![
                    "compile_commands.json".into(),
                    "compile_flags.txt".into(),
                    ".clangd".into(),
                ],
                args: vec![],
            });
        }
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("csharp"),
            kind: ServerKind::Stdio {
                command: "omnisharp".into(),
            },
            root_markers: vec!["*.sln".into(), "*.csproj".into()],
            args: vec!["-lsp".into()],
        });
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("java"),
            kind: ServerKind::Stdio {
                command: "jdtls".into(),
            },
            root_markers: vec![
                "pom.xml".into(),
                "build.gradle".into(),
                "build.gradle.kts".into(),
            ],
            args: vec![],
        });
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("kotlin"),
            kind: ServerKind::Stdio {
                command: "kotlin-language-server".into(),
            },
            root_markers: vec!["settings.gradle".into(), "settings.gradle.kts".into()],
            args: vec![],
        });
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("ruby"),
            kind: ServerKind::Stdio {
                command: "solargraph".into(),
            },
            root_markers: vec!["Gemfile".into(), ".ruby-version".into()],
            args: vec!["stdio".into()],
        });
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("php"),
            kind: ServerKind::Stdio {
                command: "intelephense".into(),
            },
            root_markers: vec!["composer.json".into()],
            args: vec!["--stdio".into()],
        });
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("swift"),
            kind: ServerKind::Stdio {
                command: "sourcekit-lsp".into(),
            },
            root_markers: vec!["Package.swift".into()],
            args: vec![],
        });
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("lua"),
            kind: ServerKind::Stdio {
                command: "lua-language-server".into(),
            },
            root_markers: vec![".luarc.json".into(), ".luarc.jsonc".into()],
            args: vec![],
        });
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
        std::fs::write(&file, "<script lang=\"ts\">export let name = \"Medousa\";</script>").unwrap();

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
        let project = dir.path().join("governed");
        let source = project.join("src");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        let file = source.join("lib.rs");
        std::fs::write(&file, "pub fn value() {}").unwrap();

        let root =
            ServerRegistry::with_defaults().resolve_root(&LanguageId::new("rust"), &file, &project);
        assert_eq!(root, project.canonicalize().unwrap());
    }

    #[test]
    fn resolve_root_supports_extension_markers() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(dir.path().join("Product.sln"), "").unwrap();
        let file = source.join("Program.cs");
        std::fs::write(&file, "class Program {}").unwrap();

        let root = ServerRegistry::with_defaults().resolve_root(
            &LanguageId::new("csharp"),
            &file,
            dir.path(),
        );
        assert_eq!(root, dir.path().canonicalize().unwrap());
    }
}
