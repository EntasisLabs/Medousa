# Native workspace: Chat and Notes

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

Use **Chat** (conversation icon) or **Notes** (book icon) in the left rail. Hover
for labels, or Tab to a control and press Enter/Space. The conversation sidebar
belongs to Chat; it is not the app-level navigation.

Switching to Notes keeps chat tabs, split panes, unsent drafts, undo history and
background streams in this window. Returning restores the chat workspace. Chat
send shortcuts do not send a message while Notes is active.

## Read and edit notes

Notes uses the connected daemon's active vault, never an assumed folder on the
app device. Select a note to load its Markdown source. Filtering searches the
loaded summaries only; load additional pages when offered. An empty filtered
page is not proof that the vault is empty.

Each opened note retains its unsaved edits and undo history when you select a
different note or return to Chat. **Save note** is explicit and checked against
the daemon version that was loaded. If that version changed, or a save result
cannot be confirmed, your edits remain intact. Check the latest version before
resolving the baseline and saving again; the app does not automatically overwrite
or retry an uncertain write.

Save before quitting: open tabs and unsaved chat/note drafts are window-lifetime
state, not recovered after relaunch. Only Chat sidebar/details layout preferences
are saved on the device. This Notes surface does not yet manage vault roots,
create/delete notes, or provide rich Markdown preview.
