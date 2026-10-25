# Desktop Spotlight and work windows

With Medousa running, press **⌘ Shift Space** on macOS or **Ctrl Shift Space** on
Windows/Linux to show Desktop Spotlight over your current app. Press the shortcut
again, press **Escape**, or click outside it to dismiss it. Closing the main
Medousa window leaves the launcher available. Use the tray or menu-bar **Quit**
action to quit Medousa completely.

On macOS, you can click and hold an image in another app, then drag it into
Spotlight. The launcher stays visible while the mouse button is held. Releasing
outside dismisses it; dropping inside attaches the image and returns keyboard
focus to the composer. Escape and the shortcut still dismiss it immediately.

You can also choose **Spotlight** from Medousa’s tray/menu-bar menu or search for
**Desktop Spotlight** in the app’s command center. **⌘/Ctrl K** still opens the
in-app command center. If another app owns the global shortcut, Medousa reports
the registration failure in Desktop Spotlight; the menu entry remains available.

## Open your work

Type to search recent conversations, Create canvases, and Code projects. Press
**Enter** or click a result to open it in its own native window. **Chat**,
**Create**, and **Code** open their respective browsers; **New chat** creates a
conversation. **Open Medousa** returns to the full workspace and its tabs.

Desktop Spotlight uses the same search, lenses, previews and command catalog as
the in-app command center. Search notes, conversations, bots, connected agents,
scripts, browser history and saved SSH destinations. Use **Overview**, **Recent**,
**Activity**, **Create** and **Actions**, or choose a named workspace to inspect
its tabs and panes. `+` filters creation commands, `!` finds runnable actions,
and `>` opens advanced commands. Pins and **Resume last Spotlight** work here too.

Note search covers all linked vaults in the selected workshop, without opening
Notes first. Each result shows its vault and path, including when two vaults have
the same filename. Searching and previewing leave the editing vault unchanged.
Opening a result saves pending note edits before switching to its vault. If a
vault cannot be searched, Spotlight names it and keeps available results visible.

Type `@` to [talk from Spotlight](spotlight-conversations.md) without opening a
full chat. The short transcript and continuing conversation stay in the widget;
**Open conversation** opens or focuses its native Chat window. Closing Spotlight
detaches the preview and leaves accepted work running.

[Activity](spotlight-activity.md) shows running work, **Needs you**, finished tasks
and execution previews inline. Opening a conversation or Code project uses its
native window. **Open terminal**, `ssh <saved server>`, and SSH picker results open
native Terminal windows without bringing the main window forward. Recent live
terminal sessions are searchable too. Task details, reviews, notes, settings and editor
actions bring the main window forward at their real destination. These commands
use the main window's current document and workspace, rather than an invisible
copy of its tabs inside the widget. If that context changes before execution,
search again to select the current command. Failures remain visible, and a
submitted action is not automatically retried.

Opening the same conversation, canvas, or Code project again focuses and restores
its existing window. Files requested for an open Code project open inside that
project’s existing window. Different workshops keep separate window identities.

Terminal windows reuse the same terminal emulator and workshop-owned PTYs as the
main app. Opening the same live session focuses its existing window. Repeating an
SSH destination reuses its live session, including after closing and reopening
its window. **New shell** creates a separate terminal; the sessions menu opens or
focuses another session’s window. Closing a Terminal window only detaches the view.
Use `exit` to end its shell. Session reuse lasts while Medousa is running; after a
full app restart, surviving sessions remain available through recent terminal results.
Launching an SSH session with an uncertain response retains the same request key
on retry instead of automatically replaying the operation.

These are the same editors and daemon-backed resources used in the main app.
Main-window tabs and detached navigation remain independent. Saved Create edits
refresh in clean views, Code uses its existing project-change reconciliation, and
chat receipts and unchanged composer drafts synchronize between windows.
Unsaved edits are preserved when another view changes the same resource; the
existing revision/digest checks still protect saves from overwriting newer work.

## Closing and workshop ownership

Closing a work window saves pending canvas edits or modified Code files before
releasing its renderer. Chat composer drafts persist too. A failed save keeps the
window open so you can resolve it or explicitly close without saving. Closing a work window detaches its stream consumers; admitted daemon
jobs continue running.

Switching workshops detaches the main window’s live subscriptions without
cancelling accepted turns, jobs, or development servers. Running work does not
require a switch confirmation. Returning to a workshop reloads its saved session
and reattaches to any live turn. Unsaved notes still save before switching, and
Code checkpoints unsaved buffer drafts without writing them into source files. A checkpoint/save failure keeps
you in the current workshop.

A work window keeps addressing its original workshop when the main app switches
workshops. Chat actions, agent approvals, attachments, artifacts, profiles, workshop
preferences,
and Code SSH setup use the workshop that owns their window. They continue to work
when the main window selects another workshop. Sign-in stays bound to its original
workshop for the whole flow. Device setup and maintenance remain in the main window.

Each opening of Desktop Spotlight also retains its displayed workshop for inline
conversations and Activity. A main-window workshop switch does not redirect those
requests or cancel their work. Commands that require the main shell ask you to
reopen Spotlight if its workshop no longer matches. Showing the launcher again
adopts the main window's current workshop and refreshes recent resources.

Desktop Spotlight is a desktop-only feature. The existing mobile and browser tab
workflows are unchanged.
