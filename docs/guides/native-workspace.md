# Native workspace: focused views and mixed desktops

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

## Customize navigation and pin bots

Press **+** on the navigation rail to open **Customize navigation**. Toggle
supported app destinations, and use the up/down controls to change their order.
The rail keeps the existing destination layout, with **+** above Settings.
The shared destinations use the daemon's active environment preset; native-only
Create, Reviews, and Terminal entries use explicitly native rail metadata in that
same daemon profile, not a second device-local preference file.

Under **Your bots**, choose **Pin** to add an individual bot avatar to the rail.
Bot shortcuts are opt-in: no bot is pinned by default. A shortcut resumes that
bot's canonical primary conversation through the Bot API, rather than creating a
new ordinary chat on each click. **Unpin** removes the shortcut, not the bot or its
history. Unavailable or archived bots do not appear as usable shortcuts.

Choose **Create bot**, give it a name, description, bundled character avatar, and
plain-language instructions. **Add to navigation** is optional. **Create & chat**
creates an identity manuscript for those instructions and opens the bot's chat.
This is a multi-step daemon operation: a failure may leave a manuscript behind;
the error identifies it for recovery, and the app does not silently retry or
pretend it rolled back the operation.

A bot's primary conversation opens in its own retained view, separate from normal
chat-session history and tabs. Selecting **Chat** restores the previous normal
conversation; reopening a bot retains its conversation and unsent draft. Bot
avatars still open the same daemon-backed primary session, never a duplicate.

In a bot's primary chat, its avatar/name and **Customize** control distinguish it
from an ordinary conversation. The right-hand editor changes its name,
description, character avatar, instructions, pin, and accent color without
replacing the chat machinery. Profile changes use the bot's revision, preserving
advanced capabilities and executor bindings. Instructions are stored in the
primary manuscript's voice appendix; they do not replace its tools or task
template. Unsaved editor drafts remain in this window and are parked by owning
workshop when switching daemons; they are not restored after app restart.

Pins and accents are saved in the daemon environment's optional navigation
metadata and preserved by both native and Tauri. Saving requires the updated
daemon; a running older daemon must be rebuilt/restarted before it can retain
these new fields. Cross-device concurrent environment edits use the existing
fresh-read/full-PUT API, which has no compare-and-swap guarantee.

## Arrange a mixed Workspace

The **Workspace** home icon is the first destination in the navigation rail. Chat, Notes, Code,
Terminal, Reviews, and Create remain focused views; opening Workspace does not
turn their navigation sidebars into nested panes.

Workspace's sidebar lists **virtual desktops** and the resources currently open
in this window. Choose a resource to open its existing identity in the focused
tab group. **New chat** and **New shell** create through their normal authenticated
connections; **Browse** goes to a focused view to create or open other content,
then return to Workspace and select that resource. Importing does not copy a
conversation, note, file, review, canvas, or shell.

- Edit the name field, then use **New desktop** or **Rename desktop**. Desktops
  can also be duplicated or deleted; deleting one removes its arrangement, not
  its resources.
- Use **Split right** / **Split down** to add groups, or drag a tab to a group's
  edge to split. Drag tabs within or between groups to reorder or move them.
  Groups can mix chat, shell, note, code, review, and Create tabs. Split sashes
  resize nested groups. **Zoom / restore pane** temporarily expands the focused
  group without changing the arrangement.
- Select **Save arrangement** to save geometry under the name field's value.
  Saved layouts are reusable split/group arrangements, not copies of resources;
  applying one retains the desktop's current tabs.
- Keyboard and composer actions follow the focused content pane, not the rail
  icon. Active input-method composition prevents switching or rearranging its
  editor. Hidden desktops retain live conversations, shell connections, drafts,
  editor buffers, undo history, and scroll in this window.

Desktop names, resource references, tab groups, splits, ratios, zoom, and layout
templates restore from the device-local `native-desktops.json` preference file.
This file contains no document bodies or credentials. Writes are coalesced and
atomic; a failed write is reported with a retry action. Closing a tab only removes
its Workspace reference; it does not kill a shell or discard an unsaved buffer.
Draft text itself is not restored from this layout file after an app restart.

Saved references reopen only through the exact owning workshop and existing
resource loaders. A disconnected workshop, unavailable reference, or an unbound
new-chat/new-shell draft remains explicit rather than creating a replacement or
opening a same-named file elsewhere. **Open resource** retries. If that content
kind is bound to another workshop, first choose the owning workshop in its focused
view and return; existing unsaved-edit guards still apply.

Create and Reviews currently expose their canonical open document/review, rather
than multiple independent simultaneous editors of that kind. Switching that
canonical resource does not retarget an older Workspace tab; its reference remains
available for reopening. This is the initial mixed-workspace implementation, not
complete behavioral parity with every Tauri multiplexer feature.

## Write a message and choose a model

When Chat opens without a loaded conversation, write directly in the initial
composer. The first Send creates the conversation in that same view and sends
your message; you do not need to click **New chat** first. The editor, draft and
focus stay in place. Creation failures keep your draft, and repeated Enter while
creation is pending does not create duplicate conversations. If you edit the
draft during creation, the updated text stays unsent until you send it again.

New native conversations start in **Assistant** mode. The mode selector is
usable in the initial composer, before a conversation exists. Choose another
available mode there to apply it before the first message; existing conversations
retain their server-reported mode. Mode changes are disabled while a turn is busy.

New conversations use the owning workshop's saved provider/model, answer depth,
reasoning effort and stage routes, from the same runtime-defaults record used by
Tauri and **Settings → Medousa Agent**. An explicit model choice stays with that
conversation; opening another new chat starts from the saved defaults again.
Reopened conversations still restore their recorded response model. Each
workshop supplies its own defaults, including when it is remote. Saving new
defaults in native Settings updates subsequent new chats without restarting;
an older startup read cannot replace that accepted save.

In the main Chat composer, **Enter** sends and **Shift+Enter** inserts a new
line. **Command+Enter** on macOS (or **Ctrl+Enter** elsewhere) also sends.
These shortcuts belong to the message editor, not search fields, menus, Notes,
or Code. Input-method composition keeps Enter for confirming text rather than
sending the draft.

Use the runtime name in the composer to choose Medousa, an available installed
agent, or a connected provider conversation. From the initial view, opening this
picker creates the conversation in place without sending your draft, using the
selected mode. Its choices update when the owning workshop's catalog finishes
loading. Runtimes needing installation or sign-in appear under **Needs setup**.
Runtime changes are disabled during a turn; a failed selection never silently
sends the message through Medousa instead. Conversation tabs use selected fill
without an accent underline, while retaining keyboard focus rings. The active
sidebar workspace uses a solid, theme-accent icon; inactive icons remain outlined.

For Medousa's own runtime, select the model name below the composer, choose a
**provider**, then choose one of that provider's models. **All providers** goes
back without changing the active model; only picking a model changes the route.
The active choices have check marks. Large catalogs scroll inside the menu and
use **Previous page** / **Next page** beyond the menu's bounded capacity.
**Search models…** opens the catalog filter; type a model or provider name and
press Enter (or **Choose provider…**) to browse matching providers and models.
Clear the filter to see the full catalog again. Escape or clicking outside
closes the menu without changing the model or draft.

## Open note and code references

Select a link in a chat message or a note's Read view to follow it. In the rich
note editor, use **Command-click** on macOS or **Ctrl-click** on Windows/Linux to
follow a link; a normal click still places the caret so you can edit without
navigating away. Heading links reveal their destination in the Read view, with
any unsaved draft retained.

App note references use `medousa://vault/folder/note.md`. Code references use
`medousa://undertaking/WORK_ID/location?path=src%2Flib.rs&line=42` and open the
specified work thread, file, and one-based line. Paths in a note can also refer to
other notes relative to that note's folder; wikilinks without an extension gain
`.md`. Note heading fragments such as `#installation` reveal the matching heading,
including headings outside the current virtualized viewport.

Navigation uses the emitting conversation or note's owning workshop, not the
currently highlighted workshop picker. Remote note/code links remain on that
daemon; they never open a similarly named device file. An attempted cross-workshop
switch cannot discard unsaved note or code edits. Normal web/email links use the
system opener; unsupported schemes and escaping paths do not become filesystem
or process actions.

## Drop files into Chat

With the **Medousa** runtime selected, drag files from your device into Chat to
attach them to the focused conversation. A composer hint appears while files are
hovered. A new chat is created in place if needed, preserving your text and mode;
attaching files never sends your message. Wait for uploads to finish, then send.
Remove a failed attachment to unblock sending, or remove it and drop it again to
retry. Directories, empty files, and files over **25 MiB** are rejected. A message
can contain at most **10 files**, including files already attached or uploading.

Files are read from the device only after an explicit drop and uploaded through
the conversation's existing authenticated workshop connection. A remote daemon
receives the file contents, not a device filesystem path. Vault links are different:
opening a note reads the owning daemon's vault, not a file on your device.
External runtimes that cannot send attachments reject drops explicitly.

The current RNDR desktop binding reports dropped paths without a drop position on
macOS, Windows, and X11. Those reports target the focused Chat pane, even when
released elsewhere in the Chat window; no stale mouse position is used to guess.
Positioned reports target the composer card in the matching visible split pane.
Drops outside Chat do not attach anything. Wayland file drops are not supported by
the current windowing binding; use the file picker instead.

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

Returned work completions display the agent's reply text, with Markdown intact,
not the structured JSON delivery envelope. This also applies when reopening saved
chat history. Copying the reply copies that text; durable work receipts and causal
links remain unchanged. Ordinary JSON answers are not treated as completions.

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

Open **Terminal** from the navigation rail for a dedicated workspace with a
collapsible, resizable session sidebar and terminal tabs. The sidebar discovers
active human shells on the owning workshop; agent, task and SSH processes are
not offered as interactive shells. **Active sessions** contains running shells only.
**Open undertakings** excludes completed, discarded and failed work. Choose an
existing session to open it in a tab. The sidebar's top **+** creates a shell
without an undertaking, in the daemon's configured shell root. Each open
undertaking has its own **+** to create a shell in that checkout. All shell
actions live in the sidebar as icons with tooltips, including refresh, reconnect
and detach for the selected tab. Tabs and active sessions use friendly numbered
names such as **Shell 1** and **Shell 2**, retained through reconnects and detach /
reattach while this Terminal view remains mounted. Raw session IDs are available
in tooltips instead of the visible names; these labels are not persisted across
app restarts.
Opening Terminal does not itself start a process. The **Terminal** action in Code
opens this same destination for the selected undertaking.

Multiple shells can remain attached at once, including several for the same
undertaking. Switching tabs retains terminal screens and connections. Closing a
tab **detaches only**: it does not terminate the workshop shell, which can be
rediscovered in the sidebar. Switching to another view also leaves sessions
running. The shell runs on the **workshop**, never in a substitute folder on the
app device, and opening it does not take a Forge editing lease.

The native terminal supports ANSI/full-screen programs, keyboard navigation,
Ctrl-C, selection/copy, and bracketed paste through its terminal emulator.
**Reconnect** reattaches the same known shell after a disconnect. Pending input is
discarded, never replayed. If a write fails its delivery may be unconfirmed: inspect
the shell before retyping. Failed shell creation is not automatically retried;
refresh sessions before trying again so an unconfirmed start is not duplicated.

Terminal has its own workshop source binding. Close attached tabs before changing
that source; changing Code's source does not redirect existing terminals. The
native bridge currently supports **local workshop** connections only and uses
authenticated WebSocket headers, not URL tokens. Remote workshop routing,
pop-outs, terminal search and clickable file paths are not implemented. Task run
history remains separate from interactive shells. Shells have normal workshop
filesystem permissions, independently of editor save leases; review commands
before executing them.

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
**Changes** keeps changed-file navigation and sealed diffs out of the summary. Technical identities and the
activity timeline are under **About**. **Changes** presents full-width file accordions
with per-file addition/removal counts. Expand individual headers or use **Expand all**
and **Collapse all**. Exact sealed file reads load one at a time, with cached
results reused only within the same evidence revision. Closing files stops queued
reads; an already-running read may finish into that revision’s cache. Failed reads
require an explicit retry. Switching candidates clears the cache and expansion state.
Choose **Comment on this file** to select the file for the anchored comment form;
opening another file preserves its separate draft. File diffs retain their own scroll
positions when other accordions change. Files open through the sealed review-file API,
not the mutable Code checkout. Binary, truncated and unavailable diffs are identified
explicitly. Each diff uses a bounded viewport in the scrolling file stream above
the decision form, using RNDR's
compact monospace code metrics, separate old/new line-number gutters, and
theme-aware generic syntax colors (not language-semantic highlighting). Paired replacement
lines also show word-level intraline backgrounds, preserving syntax colors and UTF-8
boundaries. Highlight computation is bounded for large/generated diffs; unmatched
lines retain their full-row addition/removal background. **Show unchanged context**
reveals the file's omitted lines from the exact sealed before/after text;
**Collapse unchanged context** returns to the original hunks. This is a per-file
all-context toggle, not yet the Tauri per-gap 20-line reveal. Controls are omitted
for binary, truncated, missing, inconsistent, or oversized context (over 20,000
combined source lines). Context visibility is local to the selected revision and
does not alter comment anchors or issue checkout reads. Source
indentation and blank lines are preserved. Long lines remain single-line and
are horizontally scrollable; the extent includes offscreen rows. The evidence
digest, baseline and sealed head identify what you are reading.

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


## Calendar

The native Calendar surface reads and writes `calendar/personal.ics` through the
connected workshop daemon; it never opens a separate device-local calendar file.
Month provides a continuous, height-filling six-week grid and an optional
selected-day agenda; Day and Week provide
chronological agendas. Previous, Today, and Next navigate the selected view.
Select a date to inspect its events, then use **New event** or open an existing
event to edit it. Selecting a date or **+N more** opens its agenda; **Agenda** /
**Hide agenda** also toggles it. On narrow windows the agenda is a bounded panel
below the grid. Today has a compact date marker, separate from the selected day.
The **Calendar** control reveals the daemon calendar source path.

The first native editor supports title, start/end, all-day, location, and
description. Enter dates as `YYYY-MM-DD` and timed-event times as 24-hour `HH:MM`.
Timed events use the device's local time zone; nonexistent or ambiguous daylight
saving times are rejected. All-day events use calendar dates without zone shifts;
the end date is exclusive (the day after the last included day).
Existing linked-note metadata and custom alarms are preserved on save. Under
**Event reminders**, add display alarms using the 5/15/30-minute, hour, day,
two-day, and week presets, or remove individual alarms. Duplicate presets are
not added. These alarms are persisted through the daemon; this does not enable
native notification delivery or create task reminders. **Delete event**
requires a separate confirmation. A failed/uncertain mutation retains the draft
and disables resubmission: close the editor, refresh, and inspect the daemon's
current calendar before retrying. Navigating to another surface retains an open
draft; navigation within Calendar is disabled while the editor is open.

Use the toolbar **Search** control to expand or collapse the search form; hiding
it retains the query and results. Submit **Search** in the form to filter the current view's loaded events by title, location,
or description (case-insensitive). The scope is the displayed Day, Week, or
six-week Month grid—not a global calendar search. Results retain their original
event identity for editing. **Clear search** restores the normal view. The query
is retained when changing dates or returning from an event editor.

This is not full Tauri parity. Week/Day remain chronological agendas, as in the
current reference app; the reference Week view additionally uses side-by-side
day columns. Recurring occurrences are visible but read-only
because the current update endpoint addresses the series by UID. Recurrence
editing, native notification delivery, task-reminder integration, global search,
drag/resizing, and native `.ics`
import/export controls remain outstanding. Opening an event does not enable a
native notification service.

## Peers: trusted workshop conversations

Choose **Peers** in the native navigation rail to see already-trusted workshops.
Search by name or full device identity, select a peer, and use **Refresh** to
fetch messages. On narrow windows, **All peers** returns to the list. Unread
counts reflect loaded messages; **Mark read** explicitly marks the selected
thread's unread incoming messages, not other conversations.

The first native slice supports text messaging, not agent assignments. Type a
message and press **Send** explicitly; Enter does not send. Each peer has its
own window-lifetime draft, retained when switching peers or surfaces, but not
recovered after quitting. A delayed completion cannot clear a newer draft.
Failed or uncertain sends retain the draft and block another send for that
peer. Refresh and inspect the thread before explicitly unlocking another send;
messages are never replayed automatically. A read failure may have marked some
messages; refresh before deciding again.

This surface talks only to its selected daemon. That daemon owns peer identity,
pairing, credentials, pinned identities, and signed LAN/Iroh connections; native
never reads the old app's peer registry or connects directly to those peers.
Embedding the daemon on mobile changes packaging, not this ownership boundary.
Existing client-only Tauri pairings are not silently adopted as daemon trust;
pair from Connections to establish a daemon-owned connection. The existing
Tauri client retains its legacy portal behavior during migration.
Opening a thread does not pair a device, change trust, start an agent, or grant
workspace access. Partial source failures are shown, not represented as a
successful empty inbox. Availability is checked on request; a paired workshop
is not necessarily online. Use Refresh for incoming updates in this slice.

### Connect and manage trust

Open **Connections…** in Peers. Opening it reads trust status only; it does not
create an invite, scan the LAN or pair a device. **Find nearby workshops** lists
untrusted addresses. Choose **Use this address**, then paste the invite URL from
that workshop. Alternatively enter its reachable workshop URL manually. Select
**Review pairing**, check the destination and **Confirm change**. The selected
daemon performs signed pairing and retains the connection. This creates a peer,
not a client portal or execution grant. It does not switch the active workshop.
Unified browsing of connected workshops is a separate follow-on; these peer
connections do not yet provide federated workspace browsing.

To invite another device, use **Check LAN status**, then **Enable LAN…** if
needed. Review and confirm: this interrupts connections while restarting only
the positively identified managed personal engine. Custom, remote or untracked
engines are rejected. Use **Disable LAN…** after pairing to return to loopback;
private tunnels and trust configuration are preserved. A partial restart failure
is reported, never automatically retried; inspect the engine before unlocking.

Use **Get current invite** to copy its link, or **Show pairing QR** to display an
expiring QR image for another device to scan. Invite links and QR images are
sensitive; share only with the intended recipient. Closing Connections clears
the displayed QR; expired images disappear. Camera scanning and importing a QR
image file are not implemented.

In **Trusted peers**, select the exact device and pairing. Incoming peers support
trust expiry (a future RFC3339 timestamp such as `2027-01-01T00:00:00Z`) and
inactivity expiry (1–3650 whole days). Blank values clear those limits. Use
**Review trust policy**, then confirm. A policy change expires the current
session; the peer must authenticate again. Execution policies are unchanged.
Outbound trust policy belongs to the remote workshop and is not editable here.

**Forget selected peer…** requires confirmation. For incoming peers it revokes
trust on this workshop. For outbound peers it removes the selected daemon's
connection credentials; it does not revoke trust held by the other workshop. Full device
and pairing identities are checked again before removal or policy mutation.
Refresh trust after a change. If a change cannot be confirmed, inspect both ends,
use **Refresh trust**, then explicitly unlock actions; no mutation is replayed.
Returning to conversations refreshes the peer list and preserves window drafts.

### Share notes and artifacts

In **Connections… → Trusted peers**, select a peer and enter a note path or
artifact ID from this workshop, not a path on the app device. Choose **share**,
**ask** (review request), or **bring** (bring-home message) for the matching item
type. Review the exact item, destination device and pairing, then confirm.
The daemon exports the full item and sends it once through the authenticated
peer transport. Recipients auto-import using rename-on-conflict. Ask/Bring are
message kinds, not agent starts or execution grants. Uncertain delivery blocks
another change until you inspect both ends, refresh trust and explicitly unlock.

Return to conversations and Refresh. Newly imported attachment messages offer
**Open note**/**Open artifact**, reading the actual recipient IDs from their exact
storage workshop. Previews are read-only plain text, capped at 32,000 characters;
HTML/scripts never execute and binary artifacts have no inline text preview.
Older messages may lack imported IDs: locate those items in the source workshop
rather than guessing a path or automatically importing again. Save window-only
drafts before restarting the native app.
