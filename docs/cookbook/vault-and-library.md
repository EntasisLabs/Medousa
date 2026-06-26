# Vault & library

**Audience:** operator, integrator

Markdown notes, wikilinks, multi-root vaults, and external file pins.

---

## CLI

```bash
medousa vault list
medousa vault search "quarterly review"
```

See [cli-and-workspace.md](cli-and-workspace.md).

---

## HTTP

| Task | Route |
|------|-------|
| List notes | `GET /v1/vault/notes` |
| Read note | `GET /v1/vault/notes/{path}` |
| Save note | `PUT /v1/vault/notes/{path}` with `If-Match` |
| Search | `GET /v1/vault/search?q=` |
| Backlinks | `GET /v1/vault/backlinks?path=` |
| Add root | `POST /v1/vault/roots` |
| Active root | `PUT /v1/vault/active` |

Full table: [vault.md](../engine/vault.md)

---

## Agent tools

`cognition_vault_list`, `read` (line range), `grep`, `search`, `tags`, `write` — grouped in **documents** discover domain.

---

## Wikilinks & spaces

Notes use `[[wikilink]]` syntax. Spaces (Journal, Inbox, …) filter the tree in the app.

Structured views (kanban, database) — see [vault-editing plan](../../architecture/vault-editing-and-structured-notes-plan.md).

---

## External files

Desktop Library **Files** tab pins folders outside the vault root. Search spans pinned roots.

Mobile: external files are desktop-first today; notes and presentations are fully supported on mobile.

---

## Talk about a note

Mobile/desktop: open note → chat prefill with vault scope (`chat.prefillFromVaultNote`).
