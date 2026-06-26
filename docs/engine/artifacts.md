# Artifacts

**Audience:** integrator

HTML UI artifacts are versioned documents the agent can present inline, in a side panel, or fullscreen. Integrators interact via **HTTP runtime routes** and/or **agent cognition tools** during turns.

---

## Dual API

| Layer | When to use |
|-------|-------------|
| **Agent tools** (`cognition_artifact_*`) | Agent reads/writes HTML during a turn |
| **HTTP** (`/v1/runtime/artifact/*`) | Clients fetch bodies, list catalog, TUI slash commands |

### Agent tools

| Tool | Purpose |
|------|---------|
| `cognition_artifact_list` | List artifacts in session |
| `cognition_artifact_read` | Read excerpt (optional line range) |
| `cognition_artifact_grep` | Literal case-insensitive grep with context |
| `cognition_artifact_write` | New revision (`if_match_hash64`, `supersedes`) |

Registered in `src/artifact_tools.rs`. Requires `supports_ui_artifacts=true` on the turn surface.

Vault parity: `cognition_vault_grep`, line-range `cognition_vault_read` — see [vault.md](vault.md).

### HTTP routes

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/v1/runtime/artifact/fetch` | Full HTML body (`ArtifactFetchRequest`) |
| POST | `/v1/runtime/artifact/list-ui` | Library catalog (`ArtifactListUiRequest`) |
| POST | `/v1/runtime/artifact/command` | TUI slash command DSL (`ArtifactCommandSpec`) |

`fetch` resolves to the **latest revision** in a lineage chain.

SDK: `runtime().artifact_fetch`, `artifact_list_ui`, `artifact_command` — [sdk/artifacts.md](../sdk/artifacts.md)

---

## Versioning

- `supersedes_artifact_id` links revisions
- `root_artifact_id` groups a lineage
- Stream event `artifact_updated` carries `previous_artifact_id` + `root_artifact_id`

Store: `src/artifact_store.rs`

---

## Presentation modes

| Mode | UI behavior |
|------|-------------|
| `inline` | Embedded in chat, height-capped |
| `panel` | Slide-over panel |
| `fullscreen` | Modal overlay (mobile: safe-area chrome) |

Stream field: `ui_artifact.presentation`

---

## Cookbook

[artifacts-and-presentations.md](../cookbook/artifacts-and-presentations.md)
