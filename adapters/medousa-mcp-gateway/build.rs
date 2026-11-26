//! Embeds `connectors/*/connector.toml` (and optional `SKILL.md`) into the gateway.

use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let connectors = manifest_dir.join("../../connectors");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", connectors.display());

    let mut entries = Vec::new();
    if connectors.is_dir() {
        let mut dirs: Vec<_> = fs::read_dir(&connectors)
            .unwrap_or_else(|error| panic!("read {}: {error}", connectors.display()))
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.join("connector.toml").is_file())
            .collect();
        dirs.sort();
        for dir in dirs {
            let manifest = dir.join("connector.toml");
            let skill = dir.join("SKILL.md");
            println!("cargo:rerun-if-changed={}", manifest.display());
            if skill.is_file() {
                println!("cargo:rerun-if-changed={}", skill.display());
            }
            let id = dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string();
            entries.push((id, manifest, skill.is_file().then_some(skill)));
        }
    }

    let mut body = String::from(
        "pub struct EmbeddedConnector {\n    \
         pub id: &'static str,\n    \
         pub manifest: &'static str,\n    \
         pub skill: Option<&'static str>,\n}\n\n\
         pub static EMBEDDED_CONNECTORS: &[EmbeddedConnector] = &[\n",
    );
    for (id, manifest, skill) in &entries {
        let manifest = escape_path(manifest);
        let skill_expr = match skill {
            Some(path) => format!("Some(include_str!(\"{}\"))", escape_path(path)),
            None => "None".to_string(),
        };
        body.push_str(&format!(
            "    EmbeddedConnector {{ id: \"{id}\", manifest: include_str!(\"{manifest}\"), skill: {skill_expr} }},\n"
        ));
    }
    body.push_str("];\n");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    fs::write(Path::new(&out_dir).join("connector_catalog_gen.rs"), body)
        .expect("write connector catalog");
}

fn escape_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", path.display()))
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}
