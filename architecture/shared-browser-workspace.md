# Shared Browser Workspace

Medousa Home exposes a **browser-first Web surface** with a **floating agent workshop** — the inverse of the Chat surface hierarchy.

## Layout

| Layer | Component | Role |
|-------|-----------|------|
| Main | `BrowserPanel` | Full-width tabs, URL bar, WebView (iframe on desktop v1; native WebView via BrowserHost bridge) |
| Floating | `BrowserWorkshop` | Draggable IM chat scoped to tab group + session (vault note workshop pattern) |
| Dock | Minimized workshop chip | Restore floating chat |

The Chat surface is unchanged for conversation-heavy work. The Web surface is for **view-first** browsing with optional agent extension.

## Backend

```
BrowserBridge (medousa-browser-bridge crate)
├── tab_group_id
├── tabs: create | close | activate | list
├── navigate / snapshot
└── control: agent | user | awaiting_operator

BrowserHost (Home Tauri, :7422)
├── /v1/search, /v1/fetch
├── /v1/tab-groups/*  (bridge API)
└── resume → daemon browser session complete

Daemon
├── /v1/browser/sessions/{id}/complete
├── /v1/browser/sessions/{id}/resume
└── SSE: browser_challenge, browser_navigated
```

## Agent tools

| Tool | Behavior |
|------|----------|
| `cognition_web_search` | Search via BrowserHost → lite fallback; emits `browser_navigated` on shared tab |
| `cognition_browser_fetch` | Fetch URL markdown excerpt |
| `cognition_browser_snapshot` | Snapshot URL for synthesis |

CAPTCHA: operator solves in **main browser tab**; `resume` re-runs search and completes daemon session.

## Mobile (iOS)

Web lives under **You → Web** as a full-screen browser surface. Agent chat opens as a **bottom sheet** (`MobileBrowserWorkshop`) over the browser — same vault-workshop pattern, adapted for thumb reach.

## Desktop rendering

On Tauri desktop, `BrowserWebView` mounts a **native child webview** (WKWebView on macOS, WebView2 on Windows) positioned over the browser panel region via `browser_webview_sync`. Mobile and non-Tauri builds fall back to a sandboxed iframe.

## Client registration

On Home connect, `registerBrowserClient` registers `supports_browser_host` with the daemon so turns receive browser-capable scope.

## Steve Jobs test

User opens **Web**, browses full-screen. Agent research updates tabs in the main browser; a small floating workshop appears for follow-up. Chat never competes with the page for horizontal space.
