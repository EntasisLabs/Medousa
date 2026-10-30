# Chat sticky notes

Sticky notes keep small, consequential details present while a conversation grows: corrections, exact exceptions, and things to return to later. They are working memory for **one chat**, not a replacement for the scratchpad or a conversation summary.

## Use the board

Open **Sticky notes** beside the chat composer to see the **Kept in mind** notebook. It opens in a lightweight popover without dimming or replacing the conversation. Click outside, press Escape, or use the close button to dismiss it; unfinished edits are retained while you stay in the same chat and profile.

Medousa can maintain the board through its tools; you can also:

- Choose **Add a note**, or click a note's text to edit it inline. **Save changes** and **Cancel** stay beside the editor.
- Open a note's **⋯** menu for pinning, exact wording, resolve/reopen, or removal.
- Mark a note **Pinned** or **Preserve exact text** to protect it from consolidation.
- **Resolve** a note when it no longer needs to stay hot. Turn on **Show resolved** to inspect it or **Reopen** it.
- **Remove** a note explicitly, with confirmation.
- **Reload latest** to see changes made by the agent or another client.

Edits are explicit saves, not automatic writes while you type. If another writer changed the board first, reload and review the latest state before making another change. If a request's result is uncertain, **Retry same change** reuses its original request identity rather than submitting a new mutation.

## What stays present

The runtime refreshes the active board before each model inference, including tool-loop rounds. The board follows the chat across supported Medousa modes and daemon restarts. It does not depend on semantic recall to be selected each turn.

The prompt lane has a bounded reserve. Whole notes are selected, with pinned and exact notes prioritized. If the reserve is too small, the model receives an explicit overflow notice; omitted notes remain stored. Neither old age nor context pressure silently deletes a note. Resolve stale items or consolidate unprotected notes instead of collecting everything.

Pinned and exact notes cannot be replaced by a rollup. Consolidated source notes become resolved and stay available to inspect or reopen; the replacement carries links to them. An explicit edit or removal can still change protected notes. Note text and source references are data, not permission grants or higher-priority instructions. Deleting the chat also purges its sticky boards across profiles, independently of the optional permanent-memory purge.

## Scope and privacy

The board is scoped to the chat and authenticated profile. It uses dedicated temporary storage on the daemon's durable memory backend, outside permanent-memory and semantic-recall tables. Notes are not automatically promoted into personal or project memory. Removing a note is different from resolving it: removal is intentional deletion; resolution keeps it available to reopen.

This is a Medousa runtime feature. An externally hosted provider conversation does not acquire Medousa sticky residency simply because its transcript is visible in the app. A runtime without the durable backend reports that sticky storage is unavailable instead of pretending to save notes.
