# Agent work in chat

Changes, tasks, and **Sticky notes** share one slim row immediately above, but outside, the composer’s outline. Attachment previews sit above that context row; multiple attachments scroll horizontally instead of adding more rows. Tight spacing visually groups these separate elements without extending the input border around them. Runtime settings remain just below the outline. When a project has changed files, its quiet **changes · View** summary opens the existing change review; sealed changes show **Review** instead. Tasks follow it, with notes on the right. Without changes, tasks stay on the left. Narrow windows shorten the summaries rather than adding another permanent row.

Use the agent-icon task indicator to open **Work & attention**. Current tasks are grouped into **Needs attention** and **In progress**. Approval, blocked work, failures, and pending sender review remain visible; returned work awaiting a summary is not counted as actively working. Finished sessions move into a collapsed **Finished** history, separate from current tasks. With no current tasks, the indicator becomes **Task history**, not an apparently active task count. Choosing an item does not start, approve, apply, or publish work.

Finished items offer **Dismiss**. This hides them from the default history list on this device, not from the conversation or daemon. **Show dismissed** and **Restore** make it reversible. Dismissals persist locally, scoped to workshop, runtime authority, and chat; storage restrictions fall back to in-memory behavior. A task that becomes active or needs attention resurfaces automatically. Session ended is not proof of success, acceptance, or applied changes.

## A quieter session sidebar

The sidebar initially loads a page of recent chats. Choose **Load older chats** at the bottom to keep browsing; refreshing keeps the depth you have already loaded. **Pinned** sits between **Connected agents** and the date groups. Saved pins are looked up independently, so an older pinned chat does not need to be in the recent page.

Sidebar search, in-app Spotlight chat search, and Spotlight’s **@** conversation picker search the workshop’s session catalog by name, preview, or session ID—not just the recent page. Spotlight still shows a short ranked list. Search results do not replace the browse list, and clearing search returns to your loaded history. Loading or failed searches are indicated rather than presented as a complete empty result. This searches session summaries, not every message in a transcript.

Click **Bots**, **Connected agents**, **Pinned**, **Today**, **Yesterday**, **Previous 7 days**, or **Older** to shrink or expand that section. The heading and count stay visible, and add/refresh actions remain separate. Collapse choices are saved on this device per workshop, runtime authority, and profile. Searching temporarily expands matching sections without replacing your saved choices.

Where recorded handoffs confirm both chats belong to this workshop authority, execution chats appear beneath their originating conversation rather than repeating in the date lists. A small arrow in the parent chat's row expands or collapses those related chats; the chat title still opens the parent conversation. There is no separate related-chats heading. Deeper descendants are flattened into one level. Generic **Medousa Coder** labels use the recorded task as their sidebar name; custom names are preserved. This presentation does not rename the daemon's session or change work state.

Pinned child chats remain at the top level instead of being duplicated underneath a parent. Search finds children directly and shows their parent context. Inside a linked child chat, **From:** returns to the originating conversation. Related-chat disclosures can also be collapsed and remembered.

Home discovers these links from existing handoff records and keeps a scoped local index. Older workshops without those records, unlinked chats, foreign-workshop execution chats, and chats whose parent is not in the loaded list remain standalone. No relationship is guessed from a similar name. Discovering a link or opening a chat does not start work, cancel it, accept a result, or imply that anything is currently working.

## Follow an exchange

Select a task in the picker, or its handoff link in the conversation. Both open the same exchange view inside the chat, with a **Medousa ↔ agent** header. The parent chat stays mounted: your draft and reading position are not replaced by a different session.

- Local bound agent sessions show their recorded conversation in chronological order, with **Earlier messages**, **Later messages**, and **Refresh exchange**. The latest page refreshes while the view is open.
- Worker tasks show the recorded assignment and latest output or result. These workers do not expose a separate message history; the view labels that limitation.
- For work on another workshop, the view shows the recorded handoff, updates, result, and review. The full conversation remains on its execution workshop; local session identifiers are never used to fetch a foreign workshop's transcript.

Use **Our chat**, **Return to our chat**, Escape, or mobile Back to return. There is no message composer in this view, so inspecting the exchange cannot accidentally redirect the agent.

## Decisions remain explicit

Requests that need approval show their requested work and **Approve & start** (or **Approve & adopt**) and **Decline** controls. An approved request whose start was not confirmed offers **Start approved work** instead of asking for approval again.

For ongoing or returned work, **Assignment details & navigation** contains secondary request information and explicit links to the agent conversation or execution workshop where available. Opening a conversation is separate from sending instructions to it.

A result is evidence, not proof that changes were accepted, applied, merged, or published. Project review remains a separate decision.
