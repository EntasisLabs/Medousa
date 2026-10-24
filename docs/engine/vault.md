# Vault

**Audience:** integrator

The vault is Medousa's markdown note store with wikilinks, backlinks, tags, and multi-root support.

---

## HTTP API

| Method | Path | Purpose |
|--------|------|---------|
| GET/POST | `/v1/vault/roots` | List / register roots |
| PUT | `/v1/vault/active` | Set active root for writes |
| GET/POST | `/v1/vault/notes` | List (`VaultNoteSummary[]`) / create |
| GET/PUT/DELETE | `/v1/vault/notes/{*note_path}` | CRUD single note (`VaultNote` + body) |
| GET | `/v1/vault/tags` | Tag index |
| GET | `/v1/vault/search` | Active-root full-text search (`q`), or root-qualified title/path search (`root_id`) |
| GET | `/v1/vault/changes` | Generation delta (`since_generation`) |
| GET | `/v1/vault/backlinks` | Backlinks for path |

Optimistic concurrency: `If-Match` on note PUT maps to a strict daemon
compare-and-write (`Match` precondition) inside a per-path mutation lane.
Exactly one writer with a given expected version commits; losers receive a
typed stale/conflict outcome. Create uses create-only publication (no
check-then-write). Responses may include opaque `vault_generation` /
`note_version` fields (H07).

`GET /v1/vault/notes` returns light `VaultNoteSummary` rows (path, title, mtime,
kind, tags) from the resident generation-stamped projection. The historical
`limit: 500` behavior is a **bounded adapter** and must not be treated as a
complete vault corpus — use cursors/`truncated` (when present) for large vaults.

External editors: Medousa writers have strict CAS. Uncooperative filesystem
edits are observed as separate versions / conflicts; Medousa does not claim
portable byte-CAS against arbitrary processes.

Multi-workshop / data dir: [upgrade-and-data-dir runbook](../runbooks/upgrade-and-data-dir.md), ADR-003.

---

## Multi-root & Obsidian

Add another markdown folder on the same Mac via **Add vault folder…** (Library sidebar). Roots with a `.obsidian` directory are labeled **Obsidian** in the picker. Medousa indexes `*.md` and other known text files, skips `.obsidian` / binary assets, and does not inject workshop semantic tags into Obsidian or other external roots on write — so YAML frontmatter stays yours.

Same-PC co-located Obsidian vaults work as an additional root; switch with the vault folder picker. For a single `.md` without registering a root, use **Open markdown file…**.

For cross-vault discovery, list `/v1/vault/roots`, then call
`GET /v1/vault/search?root_id=<id>&q=<text>&limit=24` for each root. This performs
read-only fuzzy title/path matching within that configured root without changing
the daemon's active editing root. With an explicit `root_id`, `q` may be empty
to return recent notes. `tags` remains a comma-separated match-all filter.
Responses echo `root_id`; clients must check it before labeling a result, since
older engines may ignore the query parameter. Without `root_id`, existing
active-root full-text search behavior is unchanged.

The root catalog skips hidden entries and symlinks, reads up to 8 KiB per file
for metadata and a short preview, and caches results for three seconds. Each scan
is bounded to 50,000 directory entries and 256 MiB of text prefixes; exceeding a
bound or encountering an unreadable root returns an error. Callers can retain
successful results from other roots. The default Medousa root also includes its
existing project overlay, with user-root paths taking precedence.

---

## Agent tools

| Tool | Purpose |
|------|---------|
| `cognition_store_read` | `store=vault`, `op=list\|read\|search` (`search` with `path` is in-file grep; `facet=tags` lists tags) |
| `cognition_store_write` | `store=vault`, `op=write\|delete\|move` |

Same primitives cover artifacts (`store=artifacts`) and code (`store=code`). See [artifacts.md](artifacts.md).

Bootstrap domain **documents** groups vault + artifact edit tools — see [agent-tools.md](agent-tools.md).

---

## App integration

- Desktop: Notes / Files / Artifacts rail doors (`ArtifactLibraryPanel` for artifacts)
- Mobile: Notes / Artifacts tabs in `MobileLibraryPanel`

Cookbook: [vault-and-library.md](../cookbook/vault-and-library.md)
