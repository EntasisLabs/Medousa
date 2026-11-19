# Medousa native desktop

Fresh RNDR application; no dependency on `apps/rndr-home`.

```sh
cd apps/medousa-native
cargo run
```

Uses the sibling RNDR checkout at `../../../../rndr`, shared
`medousa-app-core`, and shared desktop-host connection discovery. On launch it
attempts to attach to the configured local daemon using the existing local
credentials. It does **not** start, install, or restart a daemon. An unavailable
daemon is an ordinary connection error, not a separate preview application.

The shell has a slim app-level rail for **Chat**, **Notes**, **Code**, and **Reviews**, separate from the
contextual sidebar. Only mounted destinations appear; there are no placeholder
screens. Switching surfaces retains chat tabs, splits, drafts, scroll, and ongoing
streams. Notes and Code have their own editor focus; chat send shortcuts are
inactive in either editor.

The Chat surface contains a left conversation sidebar and native chat workspace.
Session search/paging and mutations, model/mode/depth controls, source-document
composition, typed history/stream rendering, attachments, runtime controls,
approvals, and session context use the existing daemon contracts. RNDR owns
editing, selection, controls, layout, Markdown, and platform rendering.

**Behavior** opens an anchored native popover without shifting the composer.
Depth selection nests inside it; Escape closes the inner selector first, then
returns focus to the Behavior trigger when closing the outer popover. Clicking
outside dismisses it without activating the underlying control. **Search models**
closes the popover and reveals the existing model filter. Icon toolbar actions
share wrapping keyboard/pointer hints; Code and Notes use the same compact
32-point refresh/check action targets. No new motion or window materials are added.

The workspace has native draggable dividers for the conversation sidebar and
optional details dock. Focus a divider and use arrow keys to resize, or
double-click it to reset. The header buttons hide/show each pane. Opening the
details dock in a narrow window temporarily hides the sidebar; closing it or
widening the window restores the sidebar preference without replacing the draft.
Pane proportions and visibility are saved on this device in
`native-workspace.json` under the Medousa configuration directory, independently
of the connected workshop.


Conversation selection opens or focuses a retained tab; selecting an already-open
conversation does not refetch or replace its editor. Open two conversations and
use **Split** to put the selected tab in a second horizontal pane; **Join** brings
the tabs back together. Each conversation retains its own canonical chat state,
draft/undo document, timeline/scroll, model/mode/depth, admission and stream route.
Background streams continue when another tab is focused. Closing a tab does not
cancel its turn or delete its session; reopening restores its in-window draft.
Narrow windows show the focused group instead of squeezing both composers.
Tabs use RNDR keyboard selection/close and same-strip reordering; the explicit
Split/Join controls are the supported path for this initial two-pane layout.
This window retains at most 24 conversations, including closed drafts, to bound
resources. Open tabs and drafts are window-lifetime state, not restored across
application relaunch; only the sidebar/details preferences above are persisted.

## Notes

Choose the book icon in the rail. Notes lists the connected daemon's active vault,
not a folder on the app device. Filter the loaded page locally, use **Load more**
when available, and select a note to read its rendered Markdown. **Edit source**
opens its retained source editor; **Read** returns to the rendered document.
Compact open-note tabs and a breadcrumb toolbar keep navigation separate from
the document. Delimited frontmatter stays in source but is hidden in reading mode. Drafts and undo
history remain in this window when switching notes or returning to Chat.

Saving is explicit and version-checked. A changed or unconfirmed daemon version
leaves your edits intact and blocks another save until you check the latest
version and resolve the baseline. There is no automatic overwrite or retry.
Notes drafts are not persisted across application relaunch. Native controls now
cover creation, root-qualified rename/move, confirmed trash deletion, restore,
daemon-index search, vault listing/linking/switching, and clean document closure.
Vault switches require all documents closed; drafts cannot silently cross roots.
Editing remains retained Markdown source, not inline rich-text.
It uses the current native app connection; it does not add remote pairing.

## Code

Choose the code icon in the rail. Projects and source files belong to the connected
workshop's Forge workspace, not a local folder picker. The native editor reuses
app-core project bindings and RNDR source documents; Forge remains authoritative
for project ownership and source-file versions.

A single header shows the file's path (collapsing leading folders when narrow)
with Search, Changes, Problems, Save/Begin editing and Check latest as icons;
**More** (…) holds Open file, Tasks, Tests, Run history, Outline, Hover,
Definition, Find uses and Find in file. Status explanations appear below it.

The first milestone covers project selection, source browsing, retained per-file
editing, and explicit version-checked saves with human edit ownership. Partial,
binary, or lossy previews are read-only. Switching files or surfaces does not
replace a dirty draft. Unconfirmed writes are not blindly retried. Source drafts
are window-lifetime state; save before quitting.

The native workbench now includes bounded daemon-backed content search,
Problems/Outline/Hover/Definition for saved files, changes/diffs/history, and
configured tasks with lease-checked starts, run history, output, source jumps and
explicit cancellation. It does not add an interactive terminal emulator, project
creation, workspace-edit refactors or the separate review approval lifecycle.
See [native workflow guide](../../docs/guides/native-workspace.md).

```sh
cargo test
```

The test suite uses fixture transports and headless RNDR frames. Passing it is
not evidence that every workflow has been manually exercised against a live
daemon or that arbitrary web/HTML content has native parity.

## Native appearance and reading size

The native shell follows the platform light/dark preference, including changes
while the window is open. If the platform reports no preference, the current
appearance is retained. Editors and reading surfaces stay opaque.

Use **Cmd/Ctrl + Plus** (or **Cmd/Ctrl + =**) and **Cmd/Ctrl + Minus** to adjust
reading/source size; **Cmd/Ctrl + 0** resets it. This window-local setting adjusts
Chat, Notes reading/source, and Code without changing navigation text, drafts,
selection or undo. It is not persisted across launches. RNDR's independent UI
text scale and contrast settings are preserved when appearance changes; current
platform reporting does not provide OS text-size, contrast or transparency
preferences. Existing RNDR motion continues to honor reduced motion; this pass
does not introduce new transitions or translucent materials.

## Interactive performance

`cargo run` now uses an optimized dev profile: application opt-level 1,
dependencies (including RNDR and text shaping) opt-level 3. Debug assertions
remain enabled. The first build/rebuild after changing profiles takes longer;
fully unoptimized builds are not representative of interactive performance.

Hidden Chat presentation is not reconciled on every Notes/Code interaction;
background transport/model updates continue and the view catches up on return.
Retained source adapters use canonical document revisions to avoid comparing
unchanged file contents. Source notifications are acknowledged after capture so
long typing sessions do not exhaust RNDR's bounded event queue.

Repeat explicit CPU probes with:

```sh
cargo test performance_probe -- --ignored --nocapture --test-threads=1
```

The loaded Code probe uses the production Parley text engine, an integrated shell,
and synthetic source edits/editor offsets. The small shell probe uses the default
headless text engine. Both measure CPU update/layout/paint, not GPU submission,
platform input latency or display presentation. They do not replace a foreground
check with a real loaded conversation/project. Save window-only drafts before
relaunching to evaluate the new executable.

## Forge Reviews

The retained Reviews workspace reads the workshop Forge queue, sealed candidates,
summary/checks/policy evidence and revision-bound file diffs. It supports anchored
comments, comment resolution, request-changes rationale, explicit
approval strategy/confirmation and separate decision-bound apply. All writes
preflight exact revision identity and daemon allowed actions; unknown writes
require refresh and are never replayed. Drafts are window-lifetime and isolated
by work/evidence. This is not hosted-provider pull-request integration. See
[the native workspace guide](../../docs/guides/native-workspace.md#review-sealed-forge-work).

## Trusted Peers

Peers provides retained trusted-workshop threads, search, unread counts,
explicit mark-read and text sends through the shared desktop-host authenticated
peer service. Window-only drafts survive peer/surface changes. Unknown delivery
is never retried automatically. Incoming updates use explicit Refresh.
**Connections** adds nearby discovery, explicit peer-only invite pairing,
current invite retrieval, incoming trust expiry/inactivity policies and confirmed
forget/revoke. Remote execution permissions are not changed. Outbound removal
forgets local credentials, not trust on the other workshop. Sharing and attachment
opening remain in the reference app.
See [the native workspace guide](../../docs/guides/native-workspace.md#peers-trusted-workshop-conversations).
