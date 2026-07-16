# Vault

**Audience:** integrator

The vault is Medousa's markdown note store with wikilinks, backlinks, tags, and multi-root support.

---

## HTTP API

| Method | Path | Purpose |
|--------|------|---------|
| GET/POST | `/v1/vault/roots` | List / register roots |
| PUT | `/v1/vault/active` | Set active root for writes |
| GET/POST | `/v1/vault/notes` | List / create |
| GET/PUT/DELETE | `/v1/vault/notes/{*note_path}` | CRUD single note |
| GET | `/v1/vault/tags` | Tag index |
| GET | `/v1/vault/search` | Search (`q` query param) |
| GET | `/v1/vault/backlinks` | Backlinks for path |

Optimistic concurrency: `If-Match` on note PUT (hash of last known content).

Multi-workshop / data dir: [upgrade-and-data-dir runbook](../runbooks/upgrade-and-data-dir.md), ADR-003.

---

## Multi-root & Obsidian

Add another markdown folder on the same Mac via **Add vault folder…** (Library sidebar). Roots with a `.obsidian` directory are labeled **Obsidian** in the picker. Medousa indexes `*.md` and other known text files, skips `.obsidian` / binary assets, and does not inject workshop semantic tags into Obsidian or other external roots on write — so YAML frontmatter stays yours.

Same-PC co-located Obsidian vaults work as an additional root; switch with the vault folder picker. For a single `.md` without registering a root, use **Open markdown file…**.

---

## Agent tools

| Tool | Purpose |
|------|---------|
| `cognition_vault_list` | List notes |
| `cognition_vault_read` | Read note (optional `line_start` / `line_end`) |
| `cognition_vault_grep` | Literal grep across vault |
| `cognition_vault_search` | Full-text search |
| `cognition_vault_tags` | Tag listing |
| `cognition_vault_write` | Create/update note |

Bootstrap domain **documents** groups vault + artifact edit tools — see [agent-tools.md](agent-tools.md).

---

## App integration

- Desktop Library: vault tree + files + **Presentations** tab (`ArtifactLibraryPanel`)
- Mobile: Notes / Presentations tabs in `MobileLibraryPanel`

Cookbook: [vault-and-library.md](../cookbook/vault-and-library.md)
