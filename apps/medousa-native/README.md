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

The shell contains a left conversation sidebar and native chat workspace.
Session search/paging and mutations, model/mode/depth controls, source-document
composition, typed history/stream rendering, attachments, runtime controls,
approvals, and session context use the existing daemon contracts. RNDR owns
editing, selection, controls, layout, Markdown, and platform rendering.

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

```sh
cargo test
```

The test suite uses fixture transports and headless RNDR frames. Passing it is
not evidence that every workflow has been manually exercised against a live
daemon or that arbitrary web/HTML content has native parity.
