# Native workspace: Chat, Notes, Code, and Reviews

The native app in `apps/medousa-native` is the RNDR desktop implementation under
active development. It is separate from the older `apps/rndr-home` preview and
does not replace the installed Tauri app automatically.

Start it from a developer checkout with the configured daemon already running:

```sh
cargo run --manifest-path apps/medousa-native/Cargo.toml
```

The app attaches to the existing connection and credentials. It does not start or
restart the daemon, provision credentials, or add remote pairing.

## Switch workspaces

Use **Chat** (conversation icon), **Notes** (book icon), **Code** (code icon), or **Reviews** (check icon)
in the left rail. Hover
for labels, or Tab to a control and press Enter/Space. The conversation sidebar
belongs to Chat; it is not the app-level navigation.

Switching to Notes, Code, or Reviews keeps chat tabs, split panes, unsent drafts, undo history and
background streams in this window. Returning restores the chat workspace. Chat
send shortcuts do not send a message while another workspace is active.

## Sidebar grouping

Chat history uses the same local-date sections as the Tauri app: **Today**,
**Yesterday**, **Previous 7 days**, and **Older**. Older starts collapsed. Select
section headings to collapse or expand them. Search temporarily reveals matching
sections without losing your disclosure choices. Regrouping does not change the
selected conversation or its draft.

Code groups work threads by the daemon's repository path, with a separate
collapsible heading for each repository. Threads without a repository identity
remain separate rather than being merged by a shared display name. Collapsing a
repository hides its thread rows without closing the active editor or changing
the selected work thread.

These are the first native sidebar parity changes, not full Tauri interaction
parity: context menus, drag/reorder affordances, and the complete Code sidebar tab
organization are not yet equivalent.

## Preview images

Select an image thumbnail in Chat to open a window-sized preview. This works for
both your attachments and images returned by Medousa. Use **+** / **−** to zoom,
**Fit** to return to the fitted view, and scroll to pan when zoomed in. Close with
**Close**, **Escape**, or a click outside the preview. Closing returns focus to
the thumbnail without changing the conversation.

Expanded previews load a higher-resolution copy through the same authenticated,
session-scoped media endpoint; they do not open a local file or external URL.
Very large images are downsampled to a bounded preview, not an original-file editor.

## Inspect delegated collaboration

When Medousa delegates a turn worker, a compact **Working with…** marker appears
with the matching assistant turn. Select it to inspect the assignment, recorded
follow-ups, live output and persisted result in the same pane. **Back to Medousa**
returns to the retained parent draft, viewport and focus; the parent can continue
working while you read. Opening this view does not create another chat.

The work item opens as a **thread on the right**, retaining the main chat and its
composer on wide windows. Compact windows temporarily show just the thread; close
it to return to the same parent conversation and draft. The task header identifies
its status and participant. **Reply to this thread…** has a separate draft for each
work item and routes through the daemon's governed worker steering endpoint, never
the main chat or a guessed external-agent chat. Replies are available only when the
recorded execution route supports them and work is active. Unsupported or terminal
work remains readable with an explanation. A reply does not approve, start or widen
permissions. Delivery uncertainty retains the draft and does not automatically retry.
Closing a thread does not cancel work. Recorded tool evidence uses its normal disclosure.
Private rolling reasoning is not copied into this view. Tool evidence may be a
retained tail rather than a complete execution log; the persisted transcript's
retention note explains that limit. If only a card preview is available, the view
says so instead of presenting it as the complete history.

A returned worker result is not evidence that Medousa has reviewed, approved or
applied changes. **Result ready · Awaiting Medousa summary** and **Result recorded**
are distinct from approval or integration. Use the ordinary review flow for that.

Activity refreshes periodically while Chat is visible; **Refresh** retries an
unavailable exchange without sending another task. Recorded content remains
available after a read error. Discovery uses the latest 100 workspace cards for
this parent session, including retained terminal cards. If the parent turn is
outside loaded history, use the collapsed **Earlier collaborations** group;
the app never attaches it to an unrelated turn. Placement is rebuilt from durable
work IDs and parent-turn bindings after reloading history or relaunching, subject
to daemon retention. Parent drafts and the currently open reading view remain
window-lifetime state.

Pending decisions are compact **work-item announcements** beside the conversation.
Select one to open its thread; purpose, permissions and approval controls live there,
not in a wall of instructions over the chat. Request details are collapsed by default.
**Approve** and **Start** remain separate explicit actions. Opening, closing or replying
to a thread never starts work. Historical pending requests remain reviewable and do
not automatically open the side panel. Completed requests retain their collaboration
history rather than showing obsolete action controls.

### Linked execution chats

Accepted native Coder assignments and external-runtime work with an independently
addressable execution chat use the same compact marker → read-only exchange flow.
The exchange shows the assignment, recorded progress, canonical chat history, live
turn events while selected, and any recorded result. **Open execution chat** opens
that exact chat in a retained tab (or focuses it if already open). Returning to the
parent tab preserves its draft, viewport and the exchange you were inspecting.
The execution tab uses the ordinary chat admission controls; the exchange itself
has no composer and grants no new continuation or approval authority.

Links require a durable accepted binding or receipt for the parent session and
connected authority. A proposed session ID, runtime label or agent ID alone does
not create a chat link. Cross-authority assignments and work without an addressable
chat are not projected by this flow. Inbox pages are followed up to a defensive
100-page bound; read failures preserve prior recorded content. Placement uses the
canonical proposal-to-turn relationship where available; otherwise the link is in
**Earlier collaborations**, never attached to a guessed parent turn. Live reads
reconcile with persisted history and deduplicate replay through the shared chat
core. Returning from an exchange only detaches its reader; it does not cancel work.
A returned result remains separate from Medousa review, approval or integration.

Arbitrary Bot-to-Bot messaging and general branch/tree navigation are not included.

## Read and edit notes

Notes uses the connected daemon's active vault, never an assumed folder on the
app device. Select a note to read its rendered Markdown. Use **Edit source** to switch to
the retained Markdown-source editor, and **Read** to return to the document.
Delimited frontmatter is hidden in reading mode but remains unchanged in source.
Open-note tabs switch between retained documents; overflowing tabs use the native
tab strip navigation. Filtering searches the
loaded summaries only; load additional pages when offered. An empty filtered
page is not proof that the vault is empty.

Each opened note retains its unsaved edits and undo history when you select a
different note or return to Chat. **Save** is explicit and checked against
the daemon version that was loaded. If that version changed, or a save result
cannot be confirmed, your edits remain intact. Check the latest version before
resolving the baseline and saving again; the app does not automatically overwrite
or retry an uncertain write.

Save before quitting: open tabs and unsaved chat/note drafts are window-lifetime
state, not recovered after relaunch. Only Chat sidebar/details layout preferences
are saved on the device. The reading view uses the native Markdown renderer;
editing uses retained Markdown source rather than the Svelte rich-note editor.

Use **New note** with a relative `.md` path to create a note on the workshop.
**Rename / move…** moves a clean saved note within its explicit vault using the
daemon's journaled relocation service, preserving note identity and links.
**Move to trash…** requires confirmation; **Trash** offers explicit restore.
Save or resolve drafts before moving, deleting, or closing a note. If you type
after starting a move, the newer local draft is retained and blocked from saving
to the obsolete path; copy that draft to the moved note before closing it.
Mutations are single-flight and never automatically retried after an uncertain result.

**Search vault** searches the daemon index (up to 100 results), unlike the sidebar's
loaded-page filter. **Vaults** lists workshop directories and links an existing
absolute directory on the daemon—not a folder on this device. Save and close all
open notes before switching vaults; relative note paths are never rebound across roots.

## Read and edit project source

Choose **Code**, choose a project, then choose one of its undertakings (threads).
The fixed **Project / Undertaking** breadcrumb above the file filter switches
context: the project segment opens the project picker; the undertaking segment
lists only threads in the current project. **Back to files** cancels a picker
without changing the active workspace. Choosing a project in the picker does not
switch files or editing ownership until you choose an undertaking.

Once selected, the sidebar shows that workspace's filesystem tree, not the entire
project/thread catalog above it. Expand folders and select a source file. The
filter searches file paths while browsing and choices while picking; switching
between these modes clears the filter. Repository paths disambiguate projects
with the same name.
Projects and source come from the connected workshop's Forge workspace; Code
does not treat a folder on the app device as the remote project's filesystem.
The tree may be bounded; a truncation notice means some files are not shown.

Each opened source file retains its own draft, undo history, selection and scroll
when switching files, projects, or surfaces. The project and path together identify
a document, so projects with the same relative filenames remain independent.

To save, use **Begin editing**, or **Continue editing** when
offered. These use Forge's existing human editing lifecycle; Code will not take
an agent's lease. **Save** requires confirmed editing ownership and the
loaded file digest. Binary, truncated, and lossy previews are read-only.

If a file changes externally, or a write cannot be confirmed, your draft stays
intact. Use **Check latest**. Review the incoming content, merge in
your editor, then explicitly acknowledge the latest baseline if appropriate.
Acknowledging is not a save; saving remains a separate action. Uncertain writes
are not automatically retried.

Source drafts, like chat and note drafts, are retained only for this window—not
recovered after quitting.

### Search, language results, changes, and tasks

**Search** queries saved project files on the daemon with a literal query and a
100-hit limit. Select a result to open that project's file at its line.
**Problems**, **Outline**, **Hover**, and **Definition** query the coding engine
for a clean saved file; hover/definition use the current cursor's UTF-16 position.
Results for an edited/replaced baseline are discarded instead of being presented
as current. Language-server availability and supported languages follow the daemon.

**Changes** lists project changes, opens bounded file diffs, and offers recorded
change history. These are inspection controls, not review approval or integration.
**Tasks** lists the daemon's configured tasks. Starting a task requires current
human editing ownership and saved project drafts. **Run history** recovers runs;
select one for bounded stdout/stderr, parsed source locations, explicit refresh,
and cancellation while active. An unconfirmed task start is never replayed
automatically. This is task output, not an interactive terminal emulator.

### Interactive terminal

Select an undertaking in **Code**, then choose **Terminal** to open the resizable
dock below the editor. This attaches to that undertaking's existing human shell,
or creates one in its workshop workspace if none is running. The header shows the
working folder; its tooltip includes the session identity and full status.
The shell runs on the **workshop**, never in a substitute folder on the app device.
Opening it does not take a Forge editing lease or attach your keyboard to an
agent/task process.

The native terminal supports ANSI/full-screen programs, keyboard navigation,
Ctrl-C, selection/copy, and bracketed paste through its terminal emulator.
**Hide** collapses the dock without ending the shell. Switching undertakings keeps
separate terminal screens and connections; it discovers an existing human shell
for the new undertaking but does not silently create one. Use **New shell** there
to create one explicitly. **New shell** also starts a fresh shell after an exit;
it does not terminate previous workshop sessions.

**Reconnect** reattaches the same known shell after a disconnect. Pending input is
discarded, never replayed. If a write fails its delivery may be unconfirmed: inspect
the shell before retyping. Failed shell creation is not automatically retried;
reopening first discovers sessions so an unconfirmed start is not duplicated.

This initial native bridge supports the configured **local workshop** only and
uses authenticated WebSocket headers, not URL tokens. Expanded terminal tabs,
pop-outs, terminal search, clickable file paths, and remote workshop routing are
not part of this dock yet. Task run history remains separate from interactive
shells. Shells have normal workshop filesystem permissions, independently of
editor save leases; review commands before executing them.

These native workflows do not add project creation, workspace-edit refactors,
rich-text note editing, or hosted pull-request review.

### Interactive build performance

The native app's normal development build optimizes the application and RNDR/text
dependencies while retaining debug assertions. A fully unoptimized build can feel
substantially slower and is not the interactive baseline. Hidden chat views keep
background model updates without reconciling their presentation on every Notes or
Code interaction. Before relaunching an updated build, save window-only drafts.
CPU fixture timings are not a guarantee of GPU/display or end-to-end input latency.

## Review sealed Forge work

Choose **Reviews** in the left rail for the connected workshop's Forge queue.
Search the queue by title or work ID. It is grouped into **Ready for review**,
**In progress**, and **Reviewed**, with **All**, **Active**, and **Reviewed** filters.
**Refresh** reads the latest evidence; it never seals, approves, or applies work.
The queue follows daemon pages with a defensive bound; an incomplete read is an
error, not proof of an empty queue.

Select work, then a sealed candidate. **Summary** includes the outcome, risk,
verification command/result, policy findings and blockers. Wide windows place checks,
risk and candidate metadata beside the reading column; narrow windows stack them.
**Changes** opens a bounded changed-file navigator and the selected sealed diff,
instead of wrapping every filename into the summary. Technical identities and the
activity timeline are under **About**. Select a changed file to open it through the sealed review-file API,
not the mutable Code checkout. Binary, truncated and unavailable diffs are identified
explicitly. The evidence digest, baseline and sealed head identify what you are reading.

Select **Old side** or **New side**, enter a line number actually present in the diff, and
write a comment. **Add comment** persists that exact sealed file/side/line
anchor. Existing comments can be
resolved explicitly when the daemon permits review. **Request changes** requires
a rationale. Rationale/comment drafts survive file and surface changes and are
isolated by work and evidence for this window only, not recovered after quitting.

Choose an integration strategy before **Approve**. Approval opens a separate
confirmation showing the candidate identity and does not apply it. **Apply** is
another explicit confirmation using the recorded decision ID. Daemon allowed-actions
govern all controls; stale evidence, blocking checks, policy findings and changed
baselines prevent approval. Each write first re-reads the selected candidate,
compares its full revision identity and rechecks permissions. The daemon remains
the final atomic authority. Refreshes invalidate pending confirmations; delayed
reads cannot replace a newer selection.

After an error or uncertain write, inspect and **Refresh** before deciding again.
Writes are single-flight and are never replayed automatically. Opening Reviews
does not acquire an editing lease, start work, prepare/seal an attempt, or restart
the daemon. Hosted provider PR inboxes, reviewer requests and provider merge
actions remain separate follow-up work; this workspace is Forge Reviews.
