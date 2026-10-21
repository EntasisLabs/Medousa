# Desktop companion

The desktop companion keeps a small, draggable Medousa pet and its most useful tools close without keeping the main window open.

## Show or hide it

Open Medousa's menu-bar menu and choose **Companion**. Choose it again, or choose **More → Hide companion** in the floating bar, to hide it.

Closing the main Medousa window hides Home but leaves the companion and background process running. Use **Quit** in Medousa's menu-bar or tray menu when you want to stop Medousa completely.

Floating windows are created when you open them. Closing Chat, a sticky note,
Web, custom views, the guide, or the companion releases that window's renderer.
Chat drafts are saved before closing. A sticky note saves pending edits first;
if saving fails, the note stays open. Reopen a tool from Medousa or the companion
when you need it again.

Click the pet to open a slim floating bar beside it. Drag the pet to move it. The same field stays anchored as search results, destinations, or recent messages open below it. Medousa keeps the bar inside the current screen’s usable area.

## What it can do

- Type in the field to search conversations and tools, or send a new request to Medousa. Click the field or press **↓** to browse recent destinations.
- Ask in the single message field to start a new Medousa conversation, without changing the conversation open in the main window.
- Type **@** to choose a named bot, an existing conversation, or a new Medousa, Cursor, Codex, or Hermes session. Other agents appear when their runtimes are installed and available.
- Read a short transcript below the same field and send follow-ups to the selected destination. **Open conversation** opens that exact session in the full Chat window.
- Use **More → New request** to start fresh, remove the destination chip to choose another, or use **More → Use clipboard** to place clipboard text into a draft for review before sending.
- Keep the draft and destination for each workshop when collapsing or reopening the companion. Older Chat or Ask drafts carry over from the previously selected mode.
- Show active-work status and approval requests. Collapsing or hiding the companion leaves admitted work running.
- Search for **Open Chat**, **New note**, **Open browser**, a custom view, or **Open Medousa** to open a tool. Additional companion controls are in **More**.
- Press **Escape** or click the pet to collapse the bar. Escape dismisses the destination picker or overflow menu first.

The companion is another client of the workshop daemon. Conversations, active turns, and approvals stay authoritative there rather than living only in the floating window.

## Choose your companion

Go to **Settings → Preferences → Look → Desktop companion**. Choose Medousa, Seahorse, or Starfish to use one of the new pixel-art characters. Their expressions respond to activity, success, and attention. The colored Medousa mark choices remain available. An already-open companion updates immediately.

The companion follows the system's reduced-motion preference.
