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
            root_markers: vec![".sln".into(), ".csproj".into()],
            args: vec!["-lsp".into()],
        });
        reg.register(ServerLaunchSpec {
            language: LanguageId::new("java"),
            kind: ServerKind::Stdio {
                command: "jdtls".into(),
            },
            root_markers: vec!["pom.xml".into(), "build.gradle".into(), "build.gradle.kts".into()],
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

    /// Walk parents from `path` looking for a root marker; fall back to `fallback`.
    pub fn resolve_root(&self, language: &LanguageId, path: &Path, fallback: &Path) -> PathBuf {
        let Some(spec) = self.get(language) else {
            return fallback.to_path_buf();
        };
        if spec.root_markers.is_empty() {
            return fallback.to_path_buf();
        }
        let mut cur = path.to_path_buf();
        if cur.is_file() {
            cur = cur
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| fallback.to_path_buf());
        }
        loop {
            for marker in &spec.root_markers {
                if cur.join(marker).exists() {
                    return cur;
                }
            }
            if !cur.pop() {
                break;
            }
        }
        fallback.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_include_grapheme_and_common_langs() {
        let reg = ServerRegistry::with_defaults();
        assert!(reg.get(&LanguageId::new("grapheme")).is_some());
        assert!(reg.get(&LanguageId::new("python")).is_some());
        assert!(reg.get(&LanguageId::new("rust")).is_some());
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
        let root = reg.resolve_root(&LanguageId::new("rust"), &file, Path::new("/tmp"));
        assert_eq!(root, dir.path());
    }
}
