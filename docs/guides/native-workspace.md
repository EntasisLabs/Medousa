# Native workspace: Chat, Notes, and Code

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

Use **Chat** (conversation icon), **Notes** (book icon), or **Code** (code icon)
in the left rail. Hover
for labels, or Tab to a control and press Enter/Space. The conversation sidebar
belongs to Chat; it is not the app-level navigation.

Switching to Notes or Code keeps chat tabs, split panes, unsent drafts, undo history and
background streams in this window. Returning restores the chat workspace. Chat
send shortcuts do not send a message while Notes or Code is active.

## Inspect delegated collaboration

When Medousa delegates a turn worker, a compact **Working with…** marker appears
with the matching assistant turn. Select it to inspect the assignment, recorded
follow-ups, live output and persisted result in the same pane. **Back to Medousa**
returns to the retained parent draft, viewport and focus; the parent can continue
working while you read. Opening this view does not create another chat.

The exchange is **read-only**. There is no worker composer, and closing it does
not cancel work. Recorded tool evidence is available behind the usual disclosure.
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

This first slice covers native turn workers exposed by the daemon's workspace
contracts, not arbitrary Bots, external runtime sessions or a general branch tree.

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
are saved on the device. This Notes surface does not yet manage vault roots,
create/delete notes, or provide inline rich-text editing. The reading view uses
the existing native Markdown renderer, not the Svelte rich-note editor.

## Read and edit project source

Choose **Code**, select a project, then expand folders and select a source file.
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
recovered after quitting. This first Code milestone does not add language
intelligence, terminal/tasks, changes/review tools, or project creation.

### Interactive build performance

The native app's normal development build optimizes the application and RNDR/text
dependencies while retaining debug assertions. A fully unoptimized build can feel
substantially slower and is not the interactive baseline. Hidden chat views keep
background model updates without reconciling their presentation on every Notes or
Code interaction. Before relaunching an updated build, save window-only drafts.
CPU fixture timings are not a guarantee of GPU/display or end-to-end input latency.
