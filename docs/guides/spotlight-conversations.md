# Talk from Spotlight

Open Spotlight with **⌘K** on macOS or **Ctrl+K** on Windows/Linux. Type `@` in
its search field to choose a destination:

- **Bots** continues a saved Bot’s conversation.
- **Conversations** continues an existing session, keeping its context.
- **Start new** creates a Medousa conversation, or a Cursor, Codex, or Hermes
  session when that runtime is available on the current workshop.

Type a name to narrow the picker. Use **↑/↓** and **Enter** or **Tab** to choose,
or click a destination. The destination appears beside the same search field;
there is no separate message composer. Type your message there and press
**Enter** to send. Further messages continue in that conversation.

Spotlight’s body shows recent messages with complete answers, Markdown, and
attachments. It shows one progress indicator while a request is running, then
the saved reply when it finishes, including if the live connection was interrupted.
Routine tool activity and background loading notices stay out of this view.
A failed send or request shows a compact explanation you can expand.
**Open conversation** opens its full view when you need more detail. Sending from Spotlight does not switch
the conversation you already have open. Existing session model selections and
Bot runtime bindings still apply. Coding sessions that require a project keep
that requirement; use their full conversation to select the project.

**Escape** dismisses the target picker first, then closes Spotlight. Click the
selected destination’s **×**, or press **Backspace** in an empty field, to return
to search. A failed send keeps your text in the field. Closing Spotlight does not
cancel accepted work. Destinations belong to the current workshop; switching
workshops clears the selection.

Requests sent from **global Desktop Spotlight** send an OS notification when the
agent finishes, even after you dismiss Spotlight and return to another app.
Completion does not open or focus a window. Reopen Spotlight to continue the
session, or open its conversation when you are ready. This uses the existing
completion receipts, so tool retries and intermediate progress do not send alerts.
**Spotlight replies** is on by default in notification preferences and can be
disabled independently of regular chat turn updates. OS notification permissions
still apply.

## Screenshots and references

Paste a screenshot into Spotlight, or drag an image onto it. Choose **@Medousa**
and ask, for example, “Create an editable wireframe from this.” Images appear as
removable thumbnails and upload to the selected conversation's workshop when you
send. Failed sends retain the draft. Medousa creates an empty frame and proposes
editable text and shapes; this rebuilds the UI as movable, restylable objects,
not an image-generation result or a flattened screenshot. Frames, groups, labels,
buttons, and other UI regions remain individually editable. Follow **Open in Create**, preview the proposal, and
choose **Use this**. Applying remains undoable. From Desktop Spotlight, the
result opens in a Create window without opening the main shell.

After choosing a recipient, type **@** within your request to reference a note,
Code project, or Create project. **/note**, **/project**, and **/frame** narrow the
picker. Chat supports the same reference picker. Notes show their vault, including
when several vaults contain the same relative path. Create frame references use
object IDs; project references let Medousa inspect their top-level objects.

References supply context. **Work here** on a Code project explicitly binds the
conversation to it through the normal project binding. Removing the reference
chip does not undo that binding. Resource references and screenshot attachments
currently require Medousa; external coding-agent and provider conversations keep
their existing text-only input contract.

Your message remains your wording. Resource identities and captured passages are
stored separately as context, and Medousa reads referenced resources with tools.
Closing Spotlight leaves accepted work running; changing workshops clears its
unsubmitted attachment and reference draft.
