# Artifacts (SDK)

**Audience:** integrator

Engine guide: [../engine/artifacts.md](../engine/artifacts.md)

---

## List UI catalog

```rust
use medousa_types::ArtifactListUiRequest;

let list = client.runtime().artifact_list_ui(&ArtifactListUiRequest {
    session_id: None, // all sessions
    limit: 50,
    query: Some("quarterly".into()),
}).await?;

for artifact in list.artifacts {
    println!("{} — {}", artifact.label, artifact.artifact_id);
}
```

---

## Fetch HTML body

```rust
use medousa_types::ArtifactFetchRequest;

let body = client.runtime().artifact_fetch(&ArtifactFetchRequest {
    session_id: artifact.session_id.clone(),
    artifact_id: artifact.artifact_id.clone(),
}).await?;

assert!(body.mime.contains("html"));
// body.body — full HTML document
```

`fetch` returns the **latest revision** in a lineage chain.

---

## Artifact command (TUI / automation)

```rust
use medousa_types::{ArtifactCommandRequest, ArtifactCommandSpec};

let response = client.runtime().artifact_command(&ArtifactCommandRequest {
    session_id: "medousa-home".into(),
    spec: ArtifactCommandSpec::List { /* … */ },
}).await?;
```

`ArtifactCommandSpec` variants mirror TUI slash commands (lookup, chunks, pack, verify, …). See `medousa_types::daemon_api` and `src/artifact_command_runtime.rs`.

---

## Tauri

```rust
// apps/medousa-home/src-tauri — invokes SDK runtime():
client(&state).runtime().artifact_list_ui(&request).await
```

Types re-exported from `medousa_types` in `daemon/types.rs` — do not duplicate in frontend `types.rs`.
