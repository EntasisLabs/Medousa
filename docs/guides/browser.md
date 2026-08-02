# Medousa for Chromium browsers

Medousa for Chromium browsers keeps a focused companion beside the page you are
already using. It captures page context only when you ask, so the current tab
stays yours and Medousa gets the smallest useful context for the next turn.

## Install a development build

From the Medousa repository:

```bash
cd integrations/browser
npm install
npm run build
```

Open `chrome://extensions` or `edge://extensions`, enable **Developer mode**,
choose **Load unpacked**, and select `integrations/browser/dist`.

Click the Medousa toolbar icon to open the side panel. The default workshop is
`http://127.0.0.1:7419`.

## Use the companion

- Open the panel from the toolbar or `Ctrl/Cmd+Shift+M`.
- Select text and choose **Ask Medousa about this selection** from the context
  menu.
- Use **Ask Medousa about this page** when the whole readable page is the
  useful context.
- Turn off **Include page content** when a question only needs the title,
  URL, or selected text.
- Click the conversation title for history, switching, naming, and deletion.
- Use **Stop** to release an active response, or **Copy** beneath a settled
  answer to move it elsewhere.

When Medousa needs fresh page state, the browser companion serves the
read-only `browser_page_snapshot` client tool from the active tab. The page
never becomes a passive background feed: the daemon requests a snapshot during
the turn, and the extension returns only the bounded title, URL, selection, and
readable text it can capture at that moment.

Advanced workshop work still belongs in Medousa Home. The browser companion is
the quick, contextual place to ask, understand, draft, and continue.

## Connect a paired workshop

Open the gear in the side panel and enter the workshop URL. Supply a bearer
token only when the workshop requires one. The extension asks for permission
for that remote origin when you save it; local daemon access is included by
default.

If the panel says the workshop is unavailable, confirm the daemon is running
and that the URL is the daemon endpoint, not the MCP gateway (`:7420`) or local
inference endpoint (`:7421`).

If page context shows only **Current page**, reload the unpacked extension and
open it by clicking the Medousa toolbar icon. Then click **Refresh context** or
send a prompt; Chrome grants page access from that explicit action, and the
companion can request the current site's optional permission if necessary.
