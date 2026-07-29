# Code projects

Code projects keep a goal, its repository, open files, conversations, terminals,
agents, and review together. You tell Medousa what you want to change; Medousa
handles the working copy and recovery details needed to keep that work safe.

They live under **Workspace → Code**. **Work** remains the place to ask for,
do, and track activity. Code is where software work stays available while you
move through Medousa. On phones and narrow windows, **Menu → Code** opens the
same projects with the project list and editor shown one level at a time.

## Loop

```text
Intent → Set up → Work → Understand → Review → Finish
```

1. Open **Code**, choose **New project**, then describe the change, the desired
   outcome, the repository folder, and starting branch.
2. **Set up project** creates a safe working copy.
3. Edit files, ask Codex or Cursor to continue, or open Terminal. **Understand**
   explains code relationships without changing anything.
4. **Review changes** gathers what changed and how it was made.
5. **Approve changes**, then **Finish project**. Discard remains under **More**.

Chat and Terminal show the same compact project context. Open the context chip
to move between the project, Review, Terminal, and a coding agent without
rebuilding context. It shows the current stage and collaborator. Internal state
is available under **Technical details** when troubleshooting requires it.

## Source editor

Code is a view inside Medousa’s permanent workspace, not a replacement shell
for the whole product. The source editor uses the same polished editor and
language-support foundation as Scripts, while Scripts remains its own
automation surface.

- File paths are always relative to the project’s safe working copy.
- Files open read-only until **Edit** starts an editing session.
- Medousa keeps that editing session available while the file surface is open.
- Saves present the lease fence and the digest from the opened file. A stale
  lease or an externally changed file returns `409`; Medousa never silently
  overwrites it.
- Absolute paths, parent traversal, symlink escapes, binary files, and text
  files over 2 MiB are rejected by the workshop daemon.
- `Cmd/Ctrl+S` saves the focused editor, `Cmd/Ctrl+Shift+S` saves all modified
  tabs, and `Cmd/Ctrl+W` closes the active tab with a discard guard.
- The Code explorer lists tracked and unignored repository files. `Cmd/Ctrl+P`
  focuses its quick filter.
- Open files become project-scoped editor tabs with independent unsaved drafts.
  Tabs, cursor targets, a secondary editor group, and protected draft
  draft recovery survive view changes and app restarts. If the file changed
  outside Medousa, the recovered draft remains visible with a conflict warning.
- Use **Split** or **Open to side** to compare two source files. The editors sit
  side by side when space permits and stack on narrow screens; `Cmd/Ctrl+\\`
  toggles the secondary group and `Ctrl+Tab` cycles source tabs.
- New file, rename, and delete are available in the repository explorer. All
  three begin or reuse the editing session and remain inside the working copy;
  rename/delete refuse unsaved open drafts and use change-conflict protection.
- Repository status and open files reconcile while Code is visible. Clean files
  changed by an agent or Terminal refresh in place; a dirty human draft is
  preserved and receives an explicit external-change warning.
- When an installed language server is available, Code attaches it to the
  project repository root. Diagnostics, completion, hover, and navigation
  come from the real server; unavailable servers degrade honestly to basic
  editing. `Cmd/Ctrl+Shift+O` opens **Structure**.

Terminal, Understand, and Review remain contextual tools around the editor:
Terminal executes, Understand explains, and Review helps the user decide what
to keep. They do not become permanent chrome when they have nothing relevant to
say.

## What each surface does

| Surface | Role |
|---------|------|
| **Project actions** | Set up, continue, review, finish, or discard |
| **Review** | See changes, risks, commands, and approve the result |
| **Understand** | Explain code and impact without changing files |
| **Versions** | Vault material memory (separate) |

Seal does not wait for Detamu indexing. World bindings show `queued` /
`indexing` / `ready` / `failed`. Missing analyzers are unavailable — not “zero
impact.”

## Terminal ownership

- **Work in Terminal** begins a human attempt when needed and opens the PTY with
  `work_id` + `lease_id` so commands can enter sealed evidence.
- Tracked Terminal tabs retain their undertaking when restored and keep their
  active lease fresh while open.
- Choose **Diagnostic** in the Terminal header to open a separate untracked
  shell. Its commands are not part of sealed evidence.

## Review and Understand

Review starts with changed files and compact code-understanding coverage.
Patch and command evidence remain scrollable supporting detail. Policy
exceptions and risky content are called out before approval; an exception must
be explicitly acknowledged. Applying an approved checkpoint has its own
confirmation boundary.

Understand can compare **Before** and **Current**. Search for a class, function,
or other name, then inspect its possible impact or open its file and line in
Code. Unavailable analyzers mean Medousa lacks evidence; they do not mean zero
impact. Understand is always read-only.

File and entity rows share one undertaking-location contract: undertaking,
repository-relative path, optional line, and optional entity. Selecting one
keeps that location attached to the undertaking while moving between Code,
World, and ForgeLens. **Copy link** produces a
`medousa://undertaking/…/location` link that restores the same editor location.

## Preserve a portable copy

Choose **Save project record…** in Review to create a portable folder containing
what changed, how it was made, and the decisions you recorded.

- For a local workshop, Medousa asks for a folder on this device and creates a
  named export directory inside it.
- For a remote workshop, enter a destination path on the workshop machine.
  Home never presents a local folder picker or uploads a local folder for this
  operation.

## API (Home clients)

The implementation retains the internal **Forge**, **undertaking**, **attempt**,
**lease**, and **evidence** names below. They are precise engine contracts, not
concepts users must learn to work in Code.

See `apps/medousa-home/src/lib/forge.ts` and daemon routes:

- `GET /v1/forge/items`, `…/review`, `…/evidence/{id}/patch|commands`
- `POST /v1/forge/repositories/inspect` and `POST /v1/forge/items/start` for
  folder-first project setup with inferred Git context
- `GET|POST|PUT|PATCH|DELETE /v1/forge/items/{id}/source` for bounded,
  governed source editing
- `GET|PUT /v1/forge/items/{id}/workspace-state` for durable editor recovery
- `GET|POST /v1/forge/items/{id}/tasks[/…/run]` for detected checks, tests, and
  builds whose results become review evidence
- `POST …/decisions` with **review intent** (server builds the decision)
- `GET /v1/forge/stream` for freshness
- `GET /v1/world/bindings/{work_id}` for World status
