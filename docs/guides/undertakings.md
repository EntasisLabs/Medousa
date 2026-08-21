# Code projects

Code projects keep a goal, its repository, open files, conversations, terminals,
agents, and review together. The durable center is the **editor**: after you
open a project, Medousa lands you on a desk — **tree + buffer**, with Terminal
under the editor via ``Ctrl/Cmd+` `` — not a waiting room. Agents, run/verify,
and Review attach around that host.

They live behind **Code** in the side rail. **Work** remains the place to ask
for, do, and track activity. Code is where software work stays available while
you move through Medousa. On phones and narrow windows, **Menu → Code** opens
the same projects with the project list and editor shown one level at a time.

## Loop

```text
Intent → Set up → Edit → Verify → Review → Finish
```

1. Open **Code**, choose **New project**, select a recent or pinned repository,
   or browse the connected workshop, then describe the outcome you want. The
   project title is that outcome.
2. **Set up project** creates a safe working copy. Medousa then opens the file
   tree (primary in the rail) and a landing file (README or first source) so
   you can edit immediately. If setup or the tree API fails, the center shows
   that failure honestly — never an empty “Open a file” as success.
3. Edit in the buffer. ``Ctrl/Cmd+` `` toggles a Terminal strip under the
   editor (same Code tab). **Pop out** opens a full Terminal shell tab. Use the
   operator strip for who edits, dirty count, issues, and last verify. Ask
   Codex or Cursor from **More** or from a selection; **Stop** / **Resume
   editing** interrupt or reclaim the agent. **Understand** explains
   relationships without changing anything.
4. **Review changes** gathers what changed and how it was made.
5. **Approve changes**, then **Finish project**. Discard, Terminal in the
   working copy, and Reveal remain under **More**. Technical details stay
   collapsed there too.

Chat and Terminal show the same compact project context. Open the context chip
to move between the project, Review, Terminal, and a coding agent without
rebuilding context. It shows the current stage and collaborator.

Coder mode does not require choosing a project first. In chat, switch to
**Coder**, then use **Choose or create project** to continue ready work or open
the same repository-and-branch setup used by Code. Creating from chat keeps the
picker open and binds the resulting governed working copy back to that chat;
there is no separate miniature creation path. **Let Medousa choose or create
it** sends the current message through a restricted setup phase: it can list,
bind, or create a project, but receives no repository mutation or command
authority until the following bound turn.

Cursor and Codex start only after the conversation has a project. Medousa
resolves that durable binding on the workshop daemon and launches the external
agent inside the project's governed working copy. Switching projects stops the
old agent session and starts or resumes it in the newly selected project;
detaching a project stops the project-bound agent instead of leaving it active
in the previous folder. Plain General-mode external chats may remain unbound.

If repository or tree APIs return 404, Medousa reports that the workshop daemon
is older than the project tools — rebuild and restart `medousa_daemon` from this
checkout rather than showing a fake-ready empty editor. If the project has no
working copy yet, **Set up project** is the primary action in the center and rail.

## Choose a repository

Repository discovery always follows the connected workshop. A local workshop
uses the native folder picker; a remote workshop shows folders from the remote
computer. Medousa never uploads a local folder or presents the Home device's
filesystem as if it belonged to the workshop.

- Recent and pinned repositories belong to the workshop and appear on every
  client connected to it.
- The remote browser starts from scoped workshop places and lists folders and
  Git repositories without requiring a server path.
- Inspection explains whether the repository is clean. Existing uncommitted
  changes stay in the original checkout; the project starts from a committed
  revision in an isolated working copy.
- If active Medousa work already targets the repository, choose **Continue**
  that project or explicitly **Start another change**.
- Manual path entry remains under the advanced disclosure for unusual mounts.
- **Clone from GitHub or GitLab…** appears only when you need to bring hosted
  work into the workshop. Medousa uses the provider CLI and its existing sign-in
  on the connected workshop; credentials never move through Home. Choose the
  destination on that same workshop, including when Home is remote.

## Source editor

Code is a view inside Medousa’s permanent workspace, not a replacement shell
for the whole product. The source editor uses the same polished editor and
language-support foundation as Scripts, while Scripts remains its own
automation surface. Open files and Review are **Workshop shell tabs** (LME
Code resources); pane splits and tab cycling belong to the Workshop shell, not
a private Code IDE chrome.

- File paths are always relative to the project’s safe working copy.
- Files stay editable when the project can start (or continue) a human editing
  session — the first keystroke or save begins one. While a revision is sealed
  for review, Code shows that edits start a new attempt; sealed evidence stays
  available as a recovery point.
- Medousa keeps that editing session available while the file surface is open.
- Saves present the lease fence and the digest from the opened file. A stale
  lease or an externally changed file returns `409`; Medousa never silently
  overwrites it.
- Absolute paths, parent traversal, and symlink escapes are rejected by the
  workshop daemon. Binary files and text over 2 MiB open as a read-only preview
  (hex dump or truncated text) with encoding metadata instead of a hard refusal.
- `Cmd/Ctrl+S` saves the focused editor; `Cmd/Ctrl+Shift+S` saves all modified
  open files for the project. `Cmd/Ctrl+Shift+T` reopens the last closed file.
  Close and cycle tabs with Workshop shell controls (`Ctrl+;` then `n` / `p`,
  or close from the tab strip). Split Editor (`Ctrl+;` then `%` / `"`) keeps the
  current file visible in both panes; drag a tab to a pane edge to move it.
  Directional pane focus (`Ctrl+;` then hjkl) follows on-screen geometry.
  Code Back/Forward restores the remembered pane when that group still exists.
  Problems, Structure, Search, Changes, Tests, the selected project command,
  and the Terminal dock restore with the project (`workspace-state` layout);
  they are not permanent chrome across Chat or Notes.
  `Cmd/Ctrl+Shift+P` opens Spotlight in command mode (`>`). Code actions also
  appear under familiar VS Code names (Quick Open, Search, Changes, Problems,
  Output, Terminal, Tests, Run Project, Build Project, Test Project, Save All,
  Format Document, Rename Symbol, New File, New Folder, Revert, Reveal in
  Explorer, and Repair Language Support). Editor menus, Explorer buttons,
  keyboard shortcuts, and Spotlight dispatch those shared commands.
  Settings → Preferences → Code exposes the bounded shortcut subset that is
  actually remappable. Focus a shortcut and press its replacement; conflicts
  are rejected and each override can be reset independently.
- The Code explorer lists tracked and unignored repository files. `Cmd/Ctrl+P`
  opens Quick Open with fuzzy path matching: type a file name, `@` plus a name
  for project symbols, or `:` plus a number to jump to a line. `Cmd/Ctrl+Shift+F`
  opens Search across
  tracked and untracked source (regex, case, whole word, include/exclude globs,
  changed-files scope, and load-more pagination). **Replace…** previews
  digest-fenced edits; uncheck files to skip them, then Apply writes the rest
  atomically. **Changes** shows the governed working copy’s branch, upstream
  ahead/behind when known, conflict state, and changed-file list. Select a file
  for a baseline comparison with real context expansion, **Revert hunk**, and
  **Restore baseline**. Conflicted paths offer Keep ours / Take theirs / Use
  baseline (clears unmerged state). **Fetch / Pull / Push / Sync** are
  lease-guarded (fast-forward pull only; Forge branch push; never force).
  **Seal for Review** checkpoints into Review; Share still happens from Review
  after finish. History and Blame are available on the Changes panel.
  Review remains the finish/decision surface.
- New file and New folder are available in the repository explorer. Nested
  parents are created as needed. Rename and delete work on the selected file or
  folder (folder ops apply one guarded multi-file transaction).
- Settings → Preferences → Code also owns bounded human-workbench behavior:
  optional format-on-save for the active language-aware file, optional 1.2
  second autosave, save-all or require-clean run preflight, and whether failed
  task matchers open Problems. The selected primary task remains scoped to the
  project and is changed from the Code command bar.
- Open files become project-scoped shell tabs with independent unsaved drafts.
  Cursor targets and protected draft recovery survive view changes and app
  restarts. If the file changed outside Medousa, the recovered draft remains
  visible with a conflict warning.
- The editor header shows clickable path breadcrumbs and, when symbols are
  known, the containing type/function trail. Folder crumbs focus that path in
  the explorer; symbol crumbs jump to the definition line. A slim operator
  strip shows who edits, dirty count, issues, and last verification. The
  status bar shows find/save/open hints, `Ln`/`Col`, indentation, language id,
  and session ownership. **View** toggles word wrap and line numbers. Saves
  whisper `Saving…` / timed `Saved`.
- File and folder create/rename/delete begin or reuse the editing session and
  remain inside the working copy; rename/delete refuse unsaved open drafts and
  use change-conflict protection.
- Repository status refreshes with the tree. When Home regains focus, the
  active clean file refreshes if an agent or Terminal changed it; a dirty human
  draft is preserved and receives an explicit external-change warning. Live
  reconciliation for every open file is still in progress.
- When an installed language server is available, Code attaches it to the
  closest language root inside the governed working copy. In a monorepo, the
  nearest `package.json`, `Cargo.toml`, `go.mod`, or other registered marker
  scopes that file without letting discovery escape the project. Diagnostics,
  completion, hover, and navigation come from the real server; unavailable
  servers degrade honestly to basic editing. `F12` goes to a definition and
  `Ctrl/Cmd+F12` goes to an implementation, opening an unopened project file as
  a shell tab when needed.
  Right-click also offers declaration and type-definition navigation when the
  server advertises them. Use `Alt+Left` / `Alt+Right` or the editor arrows to
  move through precise code locations. `Cmd/Ctrl+Shift+O` opens **Structure**.
  The editor menu also includes Find Uses, Rename, Format, Organize Imports,
  copy path, and Reveal in Explorer. `F2` opens inline rename. **Problems**
  collects diagnostics from every active project-language session, groups them
  by file, filters by severity or text, and opens the exact location even when
  the file is not already open. `Cmd/Ctrl+F` opens find with the shared editor
  chrome.
- If a language server stops, Code keeps the file editable, shows the degraded
  state, and makes three short reconnect attempts. Use **Restart language
  server** to retry immediately or **Show language server logs** to inspect the
  resolved package root, recent progress, LSP messages, and captured workshop
  process output. **Repair language support** remains the package path after
  bounded recovery is exhausted; remote repair still belongs to the connected
  workshop.
- Find uses, rename, formatting, and import organization appear only when the
  active language server supports them. Multi-file rename opens **Review
  refactor** first, including text changes and any proposed create, rename, or
  delete operations. Apply verifies the previewed digest or absence of every
  path and commits the ordered edit as one governed transaction; a conflict or
  write failure leaves every file unchanged. An older connected daemon can
  still apply text-only refactors, while resource operations explain that the
  workshop must be updated instead of partially applying the rename.
- Repository `.editorconfig` rules feed indentation before Medousa falls back
  to the file’s existing style and language defaults. An explicit user
  preference still wins.
- A co-located Medousa app can repair missing coding packages from the editor.
  Repair consults the workshop language matrix and installs `coding-engine`
  plus that language's exact package id when one exists (Svelte / TypeScript /
  JavaScript / Python use `langservers`). Languages without a package id explain
  which workshop binary is missing instead of installing an unrelated pack. A
  remote client links to Packages, because installation belongs on the connected
  workshop machine. Opening `*.svelte` uses Svelte syntax highlighting and the
  Svelte language server; JSX/TSX files use JSX-aware grammars while still
  talking to the TypeScript language server.

Terminal and Understand remain contextual tools around the editor. **Review**
opens as its own project tab — the same canvas for human, agent, and Terminal
changes — so decide/approve/finish never forks by author.

## Run and test

Medousa derives safe project commands from manifests at the repository root and
in bounded nested project roots instead of asking you to approve arbitrary
command lines. Rust, Go, JavaScript, Python, Make, and .NET projects can
contribute checks, tests, builds, and development processes. JavaScript roots
use their bun, pnpm, Yarn, or npm lockfile rather than assuming npm. A nested
command runs in the directory shown beside its name in the command picker.
Use a root `.vscode/tasks.json` entry or Terminal when the desired application
is not detected yet.

Commands whose executable or JavaScript dependencies are missing remain visible
as **unavailable** instead of failing after a save or lease transition. Hover
the disabled Run control for the workshop-machine repair instruction, install
the tool or dependencies at the stated root, and reopen the project to refresh
the catalog. Medousa prefers a healthy development/run task, then build, test,
and check commands; an explicit selection remains the project default.

- The project command bar remains available before a file is opened. Choose a
  command and select **Run**; Medousa remembers that selection for the project.
  Run saves every dirty Code buffer first and does not start if a save conflict
  needs attention. A running command becomes a **Stop** action and can be
  cancelled without closing the project or Terminal.
- **Output** is the named task channel (`Task: …`). It streams stdout/stderr while
  a check runs, shows **ready** for background/dev servers, lists clickable
  problem locations, and offers **Open in Browser** for detected loopback URLs
  (direct on a co-located workshop; tokenized private proxy when remote).
  Spotlight **Output** toggles the panel. Long-running applications open it
  automatically; short successful checks stay quiet when it was closed.
- Safe entries from `.vscode/tasks.json` (`npm` / `shell` / `process`) merge into
  the project command list with optional problem-matcher patterns and background
  readiness. Dependency graphs and the full VS Code matcher catalog are not
  imported.
- **Tests** progressively lists individual Rust, Python, JavaScript/TypeScript,
  and Go tests under the nearest compatible nested-project runner. Open one at
  its definition, run it, and see the latest retained passed/failed state beside
  it. **Run Nearest Test** is available from Spotlight and the editor context
  menu for addressable Rust, Python, and Go providers. JavaScript package
  runners currently narrow to the file, so Medousa does not mislabel them as a
  stable nearest named-test action.
- The latest result stays beside Code with a one-click exact rerun, including
  the same targeted test when one was selected. Compiler, test, and stack-trace
  locations open the referenced project file and line.
- Active and recent runs belong to the project rather than the mounted editor.
  Leave Code or reopen Medousa and the Output dock reconnects to the active run
  from its next ordered event. After completion, use the Output header to switch
  among retained recent runs. Output visibility and active/recent references
  restore with the project; older workshops fall back to the last saved run.
- Problems, Output, Tests, and Terminal share one feedback panel, so opening a
  channel replaces the visible channel instead of stacking another dock. Task
  matcher locations appear in Problems with their run identity while language
  diagnostics remain independent. Failed matched builds open navigable
  Problems unless **Panel on failure** is disabled in Code preferences. Output
  includes clear, copy, exact rerun, stop, and command-reveal actions.
- Completed checks are written into Forge command evidence. Review uses the
  latest completed result to say whether verification passed; cancelled runs
  are preserved as activity but do not pretend the revision failed.
- Interactive and background commands run directly in one workshop PTY. Open
  the Terminal channel to type into that same task process; hiding, reopening,
  moving, or popping out the pane only detaches and does not restart the task.
  Stop first sends an interrupt and changes to **Force stop** if the process
  remains alive. Ready applications offer **Open Preview** and **Open Beside
  Code** through Medousa's Web surface. Full OSC shell integration remains
  later work.

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
- The Code terminal dock supports **Find** (Mod+F while focused / Spotlight),
  clickable `path:line` links into Code, session switching, and **Run Selected
  Text in Terminal** from the editor. The default workshop shell profile uses
  `$SHELL` (login for bash/zsh); custom shell profiles and OSC shell
  integration arrive later.
- Choose **Diagnostic** / **New shell** in the Terminal header to open a separate
  untracked shell. Its commands are not part of sealed evidence.

## Review and Understand

Review starts by answering what this seal did and why each file changed, then
lets you dig into symbols and diffs only when something smells wrong. It can
open from a bound Coder conversation without leaving chat, or as a **separate
Workshop tab** for deeper editing. Human edits, coding-agent attempts, and
Terminal work all land on that same Review — there is no separate “user commit”
center.

While a coding attempt is still active, the bottom of its conversation shows a
live **Working changes** receipt with the current file inventory and line
counts. **Review** opens a responsive chat sheet backed by the same baseline
diffs used in Code. When the attempt seals, the receipt becomes **Ready for
review** and adds verification and risk signals. This is intentionally a
workspace-wide change receipt; it does not claim that one assistant message
authored every displayed edit.

Sealed diffs support the same line comments inside chat. Open comments are
carried into **Ask Medousa to revise**, which places the revision request in the
composer for inspection before sending. **Open in Code** remains available when
the task needs direct editing or the full project review workflow.

The first viewport shows an **outcome** line, quiet status chips (who wrote it,
checks, follow-up to your comments), and a **file skim**: path, symbol count
when known, `+/-` lines, and the coder’s sealed intents (hover or click the
intents control — that does **not** open the diff). Expand a file to see changed
symbols when World has indexed them, or the whole-file diff. Expand a symbol for
that scope’s hunks only. Large pure deletions start collapsed. Custody details
(base, digest, history, index coverage, PR) live under **About**.

Binary changes show honest file metadata instead of an unreadable patch. Policy
exceptions and risky content (secrets, oversize) are called out above the file
list and must be acknowledged before approval; softer warnings such as “checks
haven’t run” do not block Approve. Applying an approved revision has its own
confirmation boundary.

When several sealed attempts exist, pick another from a quiet overflow under the
outcome — not as the hero of the page.

Choose **Restore before this change…** from a file’s overflow when one path
should go back for another pass. Medousa reopens the project for editing and
keeps the reviewed revision saved as a recovery point, so restoring never
destroys the newer work before you decide. Binary versions remain safe in Git
but must currently be restored outside the Home text editor.

Line comments live on the reviewed revision. Hover a changed line and add a
comment, or press `.` while Review is focused. The comments rail appears when
you are composing or when threads exist (`c` toggles it). Open comments compile
into a **revision brief**. Choose **Continue editing** to reopen the project
yourself without starting an agent, or **Request changes** to hand the brief to
Codex/Cursor for another attempt. Typing in Code while sealed does the same
human reopen. After a follow-up seal, Review notes that this is a follow-up to
your last review when files differ from the attempt that received your feedback.

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

## Share completed work

Review can hand completed work to the repository provider without turning Code
into a provider dashboard.

- **Share branch and open review** pushes the isolated Forge branch and creates
  or updates its GitHub pull request or GitLab merge request.
- The review description includes the intended outcome, status, risk,
  verification result, changed-file count, and sealed Forge evidence digest.
- Add HTTPS links for issues, pull requests, or tickets when they are part of
  the project’s context. They remain attached to the project and are included
  in the external review summary.
- For GitHub reviews, **Review feedback** can read comments through the workshop
  CLI. Turn a comment into a separate follow-up project when it represents new
  intent; Medousa does not silently reopen or mutate completed work.

These actions are optional. If no supported origin or signed-in provider CLI is
available, the project, Review, portable record, and preserved branch continue
to work normally.

## API (Home clients)

The implementation retains the internal **Forge**, **undertaking**, **attempt**,
**lease**, and **evidence** names below. They are precise engine contracts, not
concepts users must learn to work in Code.

See `apps/medousa-home/src/lib/forge.ts` and daemon routes:

- `GET /v1/forge/items`, `…/review`, `…/evidence/{id}/patch|commands`
- `GET|PUT /v1/forge/repositories` for daemon-owned recents and pins,
  `GET /v1/forge/repositories/browse` for scoped workshop browsing,
  `POST /v1/forge/repositories/inspect` for Git state and duplicate detection,
  `GET|POST /v1/forge/repositories/provider` for optional provider capability
  discovery and workshop-owned clone,
  and `POST /v1/forge/items/start` for inferred project setup
- `GET|POST|PUT|PATCH|DELETE /v1/forge/items/{id}/source` for bounded,
  governed source editing
- `PUT /v1/forge/items/{id}/source/workspace-edit` for previewed, atomic
  multi-file text and resource refactors
- `GET|PUT /v1/forge/items/{id}/workspace-state` for durable editor recovery
- `GET /v1/forge/items/{id}/review[/file]` for synthesis and exact per-file
  comparisons; `POST …/review/file` for checkpoint-preserving restoration
- `GET|POST /v1/forge/items/{id}/tasks[/…/run]` for detected checks, tests, and
  builds whose results become review evidence
- `POST …/tasks/{task_id}/runs` plus `GET|DELETE …/task-runs/{run_id}` and
  `GET …/task-runs/{run_id}/events?since=…` for cancellable runs with live
  bounded output streaming
- `POST …/task-runs/{run_id}/preview` and `ANY /v1/forge/preview/{token}/…` for
  private Browser handoff to workshop loopback services
- `GET|POST /v1/forge/items/{id}/provider`, plus `…/context` and `…/comments`,
  for optional external review handoff and follow-up intent
- `POST …/decisions` with **review intent** (server builds the decision)
- `GET /v1/forge/stream` for undertaking-list freshness
- `GET /v1/forge/items/{id}/project-events?since=…` for resumable path-aware
  source/Git events (Code reconciles every open buffer from this stream)
- `GET /v1/world/bindings/{work_id}` for World status

Repository inspection distinguishes a branch name from a usable commit. Empty
repositories must receive an initial commit before Code can create its isolated
working copy; if a saved starting branch was renamed or deleted, choose an
existing branch and retry.
