export function chatHtml(nonce: string): string {
  return String.raw`<!doctype html>
<html lang="en"><head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'nonce-${nonce}';">
  <style>
    :root { color-scheme: light dark; --gap: 10px; }
    * { box-sizing: border-box; }
    body { margin: 0; height: 100vh; overflow: hidden; color: var(--vscode-foreground); background: var(--vscode-sideBar-background); font-family: var(--vscode-font-family); font-size: var(--vscode-font-size); }
    button, textarea, input { font: inherit; }
    button { cursor: pointer; }
    button:focus-visible, input:focus-visible, textarea:focus-visible { outline: 1px solid var(--vscode-focusBorder); outline-offset: 1px; }
    .shell { height: 100vh; display: grid; grid-template-rows: auto minmax(0,1fr) auto; }
    header { display: flex; align-items: center; gap: 8px; min-height: 42px; padding: 7px 10px; border-bottom: 1px solid var(--vscode-sideBarSectionHeader-border, var(--vscode-panel-border)); }
    .identity { min-width: 0; flex: 1; display: flex; align-items: center; gap: 8px; }
    .mark { width: 23px; height: 23px; border-radius: 50%; display: grid; place-items: center; color: var(--vscode-button-foreground); background: var(--vscode-button-background); font-weight: 700; }
    .identity-copy { min-width: 0; }
    .title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 650; line-height: 1.15; }
    .title-button { min-width: 0; border: 0; padding: 0; color: inherit; background: transparent; text-align: left; }
    .title-button:hover .title { text-decoration: underline; text-underline-offset: 2px; }
    .connection { display: flex; align-items: center; gap: 5px; color: var(--vscode-descriptionForeground); font-size: 11px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--vscode-disabledForeground); }
    .dot.connected { background: var(--vscode-testing-iconPassed); }
    .dot.checking, .dot.reconnecting { background: var(--vscode-charts-yellow); animation: pulse 1.2s infinite; }
    .dot.unavailable, .dot.unauthorized { background: var(--vscode-testing-iconFailed); }
    .icon-button { flex: 0 0 auto; width: 28px; height: 28px; border: 0; border-radius: 4px; color: var(--vscode-icon-foreground); background: transparent; }
    .icon-button:hover { background: var(--vscode-toolbar-hoverBackground); }
    main { position: relative; min-height: 0; overflow-y: auto; padding: 12px 10px 18px; scroll-behavior: smooth; }
    .empty { min-height: 65%; display: grid; place-content: center; gap: 9px; text-align: center; color: var(--vscode-descriptionForeground); padding: 20px 8px; }
    .empty[hidden] { display: none; }
    .empty-mark { margin: 0 auto; width: 42px; height: 42px; border-radius: 14px; display: grid; place-items: center; color: var(--vscode-button-foreground); background: var(--vscode-button-background); font-size: 20px; }
    .empty h2 { margin: 0; color: var(--vscode-foreground); font-size: 15px; }
    .empty p { margin: 0; line-height: 1.45; }
    .suggestions { display: grid; gap: 6px; margin-top: 6px; }
    .suggestion { border: 1px solid var(--vscode-widget-border); border-radius: 6px; padding: 7px 8px; color: var(--vscode-foreground); background: var(--vscode-editor-background); text-align: left; }
    .suggestion:hover { border-color: var(--vscode-focusBorder); }
    #messages { display: flex; flex-direction: column; gap: 13px; }
    .turn { display: flex; flex-direction: column; gap: 5px; }
    .turn.user { align-items: flex-end; }
    .bubble { max-width: 94%; border-radius: 9px; padding: 8px 10px; overflow-wrap: anywhere; line-height: 1.48; }
    .user .bubble { color: var(--vscode-input-foreground); background: var(--vscode-input-background); border: 1px solid var(--vscode-input-border, transparent); }
    .assistant .bubble { max-width: 100%; padding-left: 0; padding-right: 0; background: transparent; }
    .turn-actions { min-height: 23px; display: flex; gap: 2px; opacity: 0; transition: opacity .12s ease; }
    .turn:hover .turn-actions, .turn:focus-within .turn-actions { opacity: 1; }
    .turn.streaming .turn-actions { display: none; }
    .turn-action { border: 0; border-radius: 4px; padding: 3px 6px; color: var(--vscode-descriptionForeground); background: transparent; font-size: 11px; }
    .turn-action:hover { color: var(--vscode-foreground); background: var(--vscode-toolbar-hoverBackground); }
    .bubble p { margin: 0 0 8px; }
    .bubble p:last-child { margin-bottom: 0; }
    .bubble h1, .bubble h2, .bubble h3 { margin: 12px 0 6px; font-size: 1em; }
    .bubble ul { margin: 5px 0 8px; padding-left: 19px; }
    .bubble code.inline { padding: 1px 4px; border-radius: 3px; background: var(--vscode-textCodeBlock-background); font-family: var(--vscode-editor-font-family); }
    .code-block { margin: 8px 0; border: 1px solid var(--vscode-widget-border); border-radius: 6px; overflow: hidden; background: var(--vscode-textCodeBlock-background); }
    .code-head { display: flex; align-items: center; justify-content: space-between; gap: 6px; padding: 4px 6px; color: var(--vscode-descriptionForeground); background: var(--vscode-editorGroupHeader-tabsBackground); font-size: 11px; }
    .code-actions { display: flex; gap: 4px; }
    .code-actions button { border: 0; padding: 2px 5px; color: var(--vscode-foreground); background: transparent; }
    .code-actions button:hover { background: var(--vscode-toolbar-hoverBackground); }
    pre { margin: 0; padding: 9px; overflow-x: auto; white-space: pre; font-family: var(--vscode-editor-font-family); font-size: var(--vscode-editor-font-size); }
    a { color: var(--vscode-textLink-foreground); }
    .status-line { min-height: 20px; display: flex; align-items: center; gap: 6px; color: var(--vscode-descriptionForeground); font-size: 11px; padding: 3px 0; }
    .spinner { width: 10px; height: 10px; border: 1px solid var(--vscode-progressBar-background); border-top-color: transparent; border-radius: 50%; animation: spin .8s linear infinite; }
    .tools { display: grid; gap: 5px; }
    .tool { border: 1px solid var(--vscode-widget-border); border-radius: 6px; background: var(--vscode-editor-background); }
    .tool summary { list-style: none; display: flex; align-items: center; gap: 6px; padding: 6px 8px; cursor: pointer; }
    .tool summary::-webkit-details-marker { display: none; }
    .tool-state { margin-left: auto; color: var(--vscode-descriptionForeground); font-size: 10px; text-transform: capitalize; }
    .tool .tool-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--vscode-charts-yellow); }
    .tool.succeeded .tool-dot { background: var(--vscode-testing-iconPassed); }
    .tool.failed .tool-dot { background: var(--vscode-testing-iconFailed); }
    .tool-detail { padding: 0 8px 7px 21px; color: var(--vscode-descriptionForeground); font-size: 11px; white-space: pre-wrap; }
    .attention { border: 1px solid var(--vscode-inputValidation-warningBorder); border-radius: 6px; padding: 8px; background: var(--vscode-inputValidation-warningBackground); }
    .attention-actions { display: flex; gap: 6px; margin-top: 7px; }
    .attention button { border: 0; padding: 4px 7px; color: var(--vscode-button-foreground); background: var(--vscode-button-background); }
    .error { border-left: 2px solid var(--vscode-errorForeground); padding: 7px 9px; color: var(--vscode-errorForeground); background: var(--vscode-inputValidation-errorBackground); white-space: pre-wrap; }
    footer { border-top: 1px solid var(--vscode-panel-border); padding: 8px 9px 9px; background: var(--vscode-sideBar-background); }
    #context { display: flex; gap: 5px; overflow-x: auto; padding-bottom: 6px; }
    .chip { flex: 0 0 auto; display: inline-flex; align-items: center; gap: 4px; max-width: 170px; border: 1px solid var(--vscode-widget-border); border-radius: 999px; padding: 2px 6px; color: var(--vscode-descriptionForeground); background: var(--vscode-editor-background); font-size: 10px; }
    .chip span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .chip button { border: 0; padding: 0; color: inherit; background: transparent; }
    .composer { border: 1px solid var(--vscode-input-border, var(--vscode-widget-border)); border-radius: 7px; background: var(--vscode-input-background); overflow: hidden; }
    textarea { display: block; width: 100%; min-height: 54px; max-height: 180px; resize: none; border: 0; outline: 0; padding: 8px; color: var(--vscode-input-foreground); background: transparent; line-height: 1.4; }
    .composer-bar { display: flex; justify-content: space-between; align-items: center; gap: 6px; padding: 4px 5px; }
    .hint { color: var(--vscode-descriptionForeground); font-size: 10px; }
    .send { min-width: 64px; border: 0; border-radius: 4px; padding: 5px 9px; color: var(--vscode-button-foreground); background: var(--vscode-button-background); }
    .send:hover { background: var(--vscode-button-hoverBackground); }
    .send:disabled { opacity: .55; cursor: default; }
    .sessions-backdrop { position: fixed; z-index: 20; inset: 0; display: grid; grid-template-columns: minmax(0, 1fr) 42px; background: color-mix(in srgb, var(--vscode-editor-background) 45%, transparent); }
    .sessions-backdrop[hidden] { display: none; }
    .sessions-panel { min-width: 0; display: grid; grid-template-rows: auto auto minmax(0,1fr); background: var(--vscode-sideBar-background); border-right: 1px solid var(--vscode-panel-border); box-shadow: 3px 0 12px var(--vscode-widget-shadow); }
    .sessions-head { display: flex; align-items: center; justify-content: space-between; min-height: 42px; padding: 7px 10px; border-bottom: 1px solid var(--vscode-panel-border); }
    .sessions-head strong { font-size: 12px; text-transform: uppercase; letter-spacing: .04em; }
    .session-search { margin: 8px; width: calc(100% - 16px); border: 1px solid var(--vscode-input-border, transparent); border-radius: 4px; outline: 0; padding: 6px 8px; color: var(--vscode-input-foreground); background: var(--vscode-input-background); }
    .session-search:focus { border-color: var(--vscode-focusBorder); }
    .session-list { min-height: 0; overflow-y: auto; padding: 0 5px 10px; }
    .session-row { position: relative; display: grid; grid-template-columns: minmax(0,1fr) auto; gap: 6px; align-items: center; border-radius: 5px; padding: 7px 5px 7px 8px; }
    .session-row:hover, .session-row.active { background: var(--vscode-list-hoverBackground); }
    .session-row.opening { opacity: .6; pointer-events: none; }
    .session-row.active { color: var(--vscode-list-activeSelectionForeground); background: var(--vscode-list-activeSelectionBackground); }
    .session-select { min-width: 0; border: 0; padding: 0; color: inherit; background: transparent; text-align: left; }
    .session-name, .session-preview { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .session-name { font-weight: 600; }
    .session-preview { margin-top: 2px; color: var(--vscode-descriptionForeground); font-size: 11px; }
    .session-actions { display: flex; opacity: 0; }
    .session-row:hover .session-actions, .session-row:focus-within .session-actions, .session-row.active .session-actions { opacity: 1; }
    .session-action { width: 23px; height: 23px; border: 0; border-radius: 4px; padding: 0; color: inherit; background: transparent; }
    .session-action:hover { background: var(--vscode-toolbar-hoverBackground); }
    .session-empty { padding: 20px 10px; color: var(--vscode-descriptionForeground); text-align: center; }
    .session-list-status { padding: 6px 8px; color: var(--vscode-descriptionForeground); font-size: 11px; }
    .rename-row { grid-column: 1 / -1; display: flex; gap: 5px; }
    .rename-input { min-width: 0; flex: 1; border: 1px solid var(--vscode-focusBorder); outline: 0; padding: 4px 6px; color: var(--vscode-input-foreground); background: var(--vscode-input-background); }
    .toast { position: fixed; z-index: 30; left: 50%; bottom: 84px; transform: translateX(-50%); border: 1px solid var(--vscode-widget-border); border-radius: 5px; padding: 5px 9px; color: var(--vscode-notifications-foreground); background: var(--vscode-notifications-background); box-shadow: 0 2px 8px var(--vscode-widget-shadow); font-size: 11px; }
    .toast[hidden] { display: none; }
    .scroll-latest { position: sticky; bottom: 0; display: block; width: max-content; margin: 8px 0 0 auto; border: 1px solid var(--vscode-widget-border); border-radius: 999px; padding: 4px 9px; color: var(--vscode-foreground); background: var(--vscode-button-secondaryBackground); box-shadow: 0 2px 6px var(--vscode-widget-shadow); font-size: 11px; }
    .scroll-latest[hidden] { display: none; }
    @keyframes spin { to { transform: rotate(360deg); } }
    @keyframes pulse { 50% { opacity: .35; } }
    @media (prefers-reduced-motion: reduce) { *, *::before, *::after { animation-duration: .001ms !important; scroll-behavior: auto !important; } }
    @media (hover: none) { .turn-actions, .session-actions { opacity: 1; } }
  </style>
</head><body>
<div class="shell">
  <header>
    <div class="identity"><div class="mark">M</div><button class="title-button identity-copy" id="open-sessions" title="Conversation history"><div class="title" id="conversation-title">Medousa</div><div class="connection"><span id="connection-dot" class="dot checking"></span><span id="connection-label">Checking workshop…</span></div></button></div>
    <button class="icon-button" id="new-session" title="New conversation" aria-label="New conversation">＋</button>
    <button class="icon-button" id="open-home" title="Open Medousa" aria-label="Open Medousa">↗</button>
    <button class="icon-button" id="configure" title="Configure connection" aria-label="Configure connection">⋯</button>
  </header>
  <main id="scroll">
    <section id="empty" class="empty"><div class="empty-mark">M</div><h2>What are we working on?</h2><p>Ask about the active file, your selection, diagnostics, or anything in your workshop.</p><div id="suggestions" class="suggestions"></div></section>
    <div id="messages"></div>
    <button id="scroll-latest" class="scroll-latest" title="Jump to latest message" hidden>↓ Latest</button>
  </main>
  <footer>
    <div id="context"></div>
    <div class="composer"><textarea id="prompt" aria-label="Message Medousa" placeholder="Message Medousa…"></textarea><div class="composer-bar"><span class="hint">Enter to send · Shift+Enter for newline</span><button id="send" class="send">Send</button></div></div>
  </footer>
</div>
<div id="sessions-backdrop" class="sessions-backdrop" hidden>
  <aside class="sessions-panel" aria-label="Conversation history">
    <div class="sessions-head"><strong>Conversations</strong><div><button class="icon-button" id="sessions-new" title="New conversation" aria-label="New conversation">＋</button><button class="icon-button" id="sessions-close" title="Close history" aria-label="Close history">×</button></div></div>
    <input id="session-search" class="session-search" type="search" placeholder="Search conversations…" aria-label="Search conversations">
    <div id="session-list" class="session-list"></div>
  </aside>
</div>
<div id="toast" class="toast" role="status" hidden></div>
<script nonce="${nonce}">
  const vscode = acquireVsCodeApi();
  const persisted = vscode.getState() || { messages: [], drafts: {}, scrollPositions: {}, activeSessionId: null };
  const messages = document.getElementById("messages");
  const empty = document.getElementById("empty");
  const scroll = document.getElementById("scroll");
  const prompt = document.getElementById("prompt");
  const send = document.getElementById("send");
  const contextRow = document.getElementById("context");
  const suggestions = document.getElementById("suggestions");
  const sessionsBackdrop = document.getElementById("sessions-backdrop");
  const sessionList = document.getElementById("session-list");
  const sessionSearch = document.getElementById("session-search");
  const conversationTitle = document.getElementById("conversation-title");
  const scrollLatest = document.getElementById("scroll-latest");
  let busy = false;
  let assistant = null;
  let assistantRaw = "";
  let statusNode = null;
  const tools = new Map();
  let sessions = [];
  let activeSessionId = null;
  let currentSessionId = persisted.activeSessionId || null;
  let drafts = persisted.drafts || {};
  let scrollPositions = persisted.scrollPositions || {};
  let sessionsLoading = false;
  let sessionsError = "";
  let toastTimer = null;
  let scrollPersistTimer = null;
  let pendingLibraryButton = null;

  function persist() { vscode.setState({ messages: Array.from(messages.querySelectorAll(".turn")).map(function(node) { return { role: node.dataset.role, content: node.dataset.raw || "" }; }), drafts: drafts, scrollPositions: scrollPositions, activeSessionId: currentSessionId }); }
  function atBottom() { return scroll.scrollHeight - scroll.scrollTop - scroll.clientHeight < 70; }
  function updateScrollAffordance() { scrollLatest.hidden = atBottom(); }
  function pin(wasBottom) { requestAnimationFrame(function() { if (wasBottom) scroll.scrollTop = scroll.scrollHeight; updateScrollAffordance(); }); }
  function saveDraft() { if (!currentSessionId) return; const value = prompt.value; if (value) drafts[currentSessionId] = value; else delete drafts[currentSessionId]; persist(); }
  function saveConversationPosition() { if (currentSessionId) scrollPositions[currentSessionId] = atBottom() ? "bottom" : scroll.scrollTop; }
  function saveConversationState() { saveConversationPosition(); saveDraft(); }
  function updateComposerState() { send.disabled = !busy && !prompt.value.trim(); send.setAttribute("aria-label", busy ? "Stop response" : "Send message"); }
  function restoreDraft() { prompt.value = currentSessionId ? drafts[currentSessionId] || "" : ""; resize(); updateComposerState(); }
  function escapeHtml(value) { return String(value).replace(/[&<>"']/g, function(ch) { if (ch === "&") return "&amp;"; if (ch === "<") return "&lt;"; if (ch === ">") return "&gt;"; if (ch === '"') return "&quot;"; return "&#39;"; }); }
  function safeUrl(value) { try { const url = new URL(value); return ["http:","https:","medousa:"].includes(url.protocol) ? value : ""; } catch { return ""; } }
  function renderProse(value) {
    const lines = escapeHtml(value).split(/\n/); let html = ""; let list = false;
    function closeList() { if (list) { html += "</ul>"; list = false; } }
    lines.forEach(function(line) {
      line = line.replace(/\[([^\]]+)\]\(([^)]+)\)/g, function(_, label, href) { const safe = safeUrl(href); return safe ? '<a href="#" data-href="' + escapeHtml(safe) + '">' + label + '</a>' : label; });
      line = line.replace(/\`([^\`]+)\`/g, '<code class="inline">$1</code>').replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
      const heading = /^(#{1,3})\s+(.+)$/.exec(line); const item = /^[-*]\s+(.+)$/.exec(line);
      if (heading) { closeList(); html += '<h' + heading[1].length + '>' + heading[2] + '</h' + heading[1].length + '>'; }
      else if (item) { if (!list) { html += "<ul>"; list = true; } html += "<li>" + item[1] + "</li>"; }
      else { closeList(); if (line.trim()) html += "<p>" + line + "</p>"; }
    }); closeList(); return html;
  }
  function renderMarkdown(value) {
    let html = ""; let cursor = 0; const pattern = /\`\`\`([\w+-]*)\n([\s\S]*?)\`\`\`/g; let match;
    while ((match = pattern.exec(value))) { html += renderProse(value.slice(cursor, match.index)); const language = match[1] || "code"; const encoded = encodeURIComponent(match[2]); html += '<div class="code-block"><div class="code-head"><span>' + escapeHtml(language) + '</span><div class="code-actions"><button data-copy-code="' + encoded + '">Copy</button><button data-insert-code="' + encoded + '">Insert</button></div></div><pre><code>' + escapeHtml(match[2]) + '</code></pre></div>'; cursor = pattern.lastIndex; }
    html += renderProse(value.slice(cursor)); return html;
  }
  function showEmpty() { empty.hidden = messages.children.length > 0; }
  function addTurn(role, content, settled) { const wasBottom = atBottom(); const turn = document.createElement("section"); turn.className = "turn " + role + (role === "assistant" && settled === false ? " streaming" : ""); turn.dataset.role = role; turn.dataset.raw = content; const bubble = document.createElement("div"); bubble.className = "bubble"; if (role === "assistant") bubble.innerHTML = renderMarkdown(content); else bubble.textContent = content; turn.appendChild(bubble); if (role === "assistant") { const actions = document.createElement("div"); actions.className = "turn-actions"; actions.innerHTML = '<button class="turn-action" data-turn-action="copy" title="Copy reply">Copy</button><button class="turn-action" data-turn-action="share" title="Share reply">Share</button><button class="turn-action" data-turn-action="library" title="Save to Library">Library</button>'; turn.appendChild(actions); } messages.appendChild(turn); showEmpty(); pin(wasBottom); persist(); return { turn: turn, bubble: bubble }; }
  function appendAssistant(text) { const wasBottom = atBottom(); if (!assistant) { const created = addTurn("assistant", "", false); assistant = created; assistantRaw = ""; } assistantRaw += text; assistant.turn.dataset.raw = assistantRaw; assistant.bubble.innerHTML = renderMarkdown(assistantRaw); pin(wasBottom); persist(); }
  function replaceAssistant(text) { if (!assistant) assistant = addTurn("assistant", "", false); assistantRaw = text; assistant.turn.dataset.raw = text; assistant.bubble.innerHTML = renderMarkdown(text); persist(); pin(true); }
  function setStatus(text, working) { const wasBottom = atBottom(); if (!statusNode) { statusNode = document.createElement("div"); statusNode.className = "status-line"; statusNode.setAttribute("role", "status"); messages.appendChild(statusNode); } statusNode.innerHTML = (working ? '<span class="spinner"></span>' : "") + '<span>' + escapeHtml(text) + '</span>'; pin(wasBottom); }
  function clearStatus() { if (statusNode) statusNode.remove(); statusNode = null; }
  function setBusy(value) { busy = value; send.textContent = value ? "Stop" : "Send"; send.classList.toggle("secondary", value); prompt.disabled = value; updateComposerState(); }
  function addTool(message) { const wasBottom = atBottom(); let node = tools.get(message.runId); if (!node) { const details = document.createElement("details"); details.className = "tool running"; details.innerHTML = '<summary><span class="tool-dot"></span><span class="tool-name"></span><span class="tool-state"></span></summary><div class="tool-detail"></div>'; messages.appendChild(details); tools.set(message.runId, details); node = details; } const state = message.status || "running"; node.querySelector(".tool-name").textContent = message.name; node.querySelector(".tool-state").textContent = state === "running" ? "Running" : state === "succeeded" ? "Done" : state; node.querySelector(".tool-detail").textContent = message.summary || ""; node.className = "tool " + state; node.setAttribute("aria-label", message.name + " · " + state); pin(wasBottom); }
  function addAttention(message) { const node = document.createElement("div"); node.className = "attention"; node.textContent = message.text; const actions = document.createElement("div"); actions.className = "attention-actions"; ["Approve", "Deny"].forEach(function(label) { const button = document.createElement("button"); button.textContent = label; button.addEventListener("click", function() { vscode.postMessage({ type: message.kind === "budget" ? "budget" : "permission", requestId: message.requestId, approve: label === "Approve", rounds: message.rounds }); node.remove(); }); actions.appendChild(button); }); node.appendChild(actions); messages.appendChild(node); }
  function addError(text) { clearStatus(); if (assistant) { assistant.turn.classList.remove("streaming"); assistant = null; } if (pendingLibraryButton) { pendingLibraryButton.disabled = false; pendingLibraryButton.textContent = "Library"; pendingLibraryButton = null; } const node = document.createElement("div"); node.className = "error"; const copy = document.createElement("div"); copy.textContent = text; const retry = document.createElement("button"); retry.textContent = "Retry"; retry.className = "send"; retry.style.marginTop = "7px"; retry.addEventListener("click", function() { node.remove(); setBusy(true); vscode.postMessage({ type: "retry" }); }); node.appendChild(copy); node.appendChild(retry); messages.appendChild(node); setBusy(false); updateScrollAffordance(); }
  function restoreHistory(turns) { messages.innerHTML = ""; assistant = null; tools.clear(); (turns || []).forEach(function(turn) { if (["user","assistant"].includes(turn.role) && turn.content) addTurn(turn.role, turn.content, true); }); showEmpty(); requestAnimationFrame(function() { const saved = currentSessionId ? scrollPositions[currentSessionId] : null; scroll.scrollTop = typeof saved === "number" ? saved : scroll.scrollHeight; updateScrollAffordance(); }); }
  function showToast(text) { const node = document.getElementById("toast"); node.textContent = text; node.hidden = false; if (pendingLibraryButton) { pendingLibraryButton.disabled = false; pendingLibraryButton.textContent = "Library"; pendingLibraryButton = null; } if (toastTimer) clearTimeout(toastTimer); toastTimer = setTimeout(function() { node.hidden = true; }, 1800); }
  function formatSessionTime(value) { if (!value) return ""; const date = new Date(value); if (Number.isNaN(date.getTime())) return ""; const today = new Date(); return date.toDateString() === today.toDateString() ? date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }) : date.toLocaleDateString([], { month: "short", day: "numeric" }); }
  function renderSessions() { const query = sessionSearch.value.trim().toLowerCase(); const visible = sessions.filter(function(item) { return !query || item.displayName.toLowerCase().includes(query) || item.preview.toLowerCase().includes(query); }); sessionList.innerHTML = ""; if (sessionsLoading) { const status = document.createElement("div"); status.className = "session-list-status"; status.textContent = sessions.length ? "Updating…" : "Loading conversations…"; sessionList.appendChild(status); } if (sessionsError) { const status = document.createElement("div"); status.className = "session-list-status"; status.textContent = sessionsError; sessionList.appendChild(status); } if (!visible.length && !sessionsLoading) { const emptyNode = document.createElement("div"); emptyNode.className = "session-empty"; emptyNode.textContent = query ? "No matching conversations." : "No conversations yet."; sessionList.appendChild(emptyNode); return; } visible.forEach(function(item) { const row = document.createElement("div"); row.className = "session-row" + (item.sessionId === activeSessionId ? " active" : ""); row.dataset.sessionId = item.sessionId; const select = document.createElement("button"); select.className = "session-select"; select.dataset.sessionAction = "select"; const name = document.createElement("div"); name.className = "session-name"; name.textContent = item.displayName; const preview = document.createElement("div"); preview.className = "session-preview"; preview.textContent = [item.preview, formatSessionTime(item.lastTimestamp)].filter(Boolean).join(" · "); select.appendChild(name); select.appendChild(preview); const actions = document.createElement("div"); actions.className = "session-actions"; actions.innerHTML = '<button class="session-action" data-session-action="rename" title="Rename" aria-label="Rename">✎</button><button class="session-action" data-session-action="delete" title="Delete" aria-label="Delete">×</button>'; row.appendChild(select); row.appendChild(actions); sessionList.appendChild(row); }); const active = sessions.find(function(item) { return item.sessionId === activeSessionId; }); conversationTitle.textContent = active ? active.displayName : "Medousa"; }
  function startRename(row) { const item = sessions.find(function(entry) { return entry.sessionId === row.dataset.sessionId; }); if (!item) return; row.innerHTML = '<form class="rename-row"><input class="rename-input" maxlength="80" aria-label="Conversation name"><button class="session-action" type="submit" title="Save">✓</button><button class="session-action" type="button" data-session-action="cancelRename" title="Cancel">×</button></form>'; const input = row.querySelector("input"); input.value = item.displayName; input.focus(); input.select(); }
  function pairedUserTurn(turn) { let previous = turn.previousElementSibling; while (previous) { if (previous.classList.contains("turn")) { if (previous.dataset.role === "user") return previous; if (previous.dataset.role === "assistant") return null; } previous = previous.previousElementSibling; } return null; }
  function submit(text) { const value = (text || prompt.value).trim(); if (!value || busy) return; addTurn("user", value, true); assistant = null; assistantRaw = ""; prompt.value = ""; if (currentSessionId) delete drafts[currentSessionId]; resize(); updateComposerState(); persist(); setBusy(true); vscode.postMessage({ type: "send", text: value }); }
  function resize() { prompt.style.height = "auto"; prompt.style.height = Math.min(prompt.scrollHeight, 180) + "px"; }
  send.addEventListener("click", function() { if (busy) vscode.postMessage({ type: "cancel" }); else submit(); });
  prompt.addEventListener("input", function() { resize(); updateComposerState(); saveDraft(); });
  prompt.addEventListener("keydown", function(event) { if (event.key === "Enter" && ((!event.shiftKey && !event.ctrlKey && !event.metaKey) || event.ctrlKey || event.metaKey)) { event.preventDefault(); submit(); } });
  scroll.addEventListener("scroll", function() { updateScrollAffordance(); if (scrollPersistTimer) clearTimeout(scrollPersistTimer); scrollPersistTimer = setTimeout(function() { saveConversationPosition(); persist(); }, 120); });
  scrollLatest.addEventListener("click", function() { scroll.scrollTo({ top: scroll.scrollHeight, behavior: "smooth" }); });
  suggestions.addEventListener("click", function(event) { const button = event.target.closest(".suggestion"); if (button) submit(button.textContent); });
  document.getElementById("configure").addEventListener("click", function() { vscode.postMessage({ type: "configure" }); });
  document.getElementById("new-session").addEventListener("click", function() { saveConversationState(); vscode.postMessage({ type: "newSession" }); });
  document.getElementById("open-sessions").addEventListener("click", function() { vscode.postMessage({ type: "openSessions" }); });
  document.getElementById("sessions-close").addEventListener("click", function() { sessionsBackdrop.hidden = true; });
  document.getElementById("sessions-new").addEventListener("click", function() { saveConversationState(); sessionsBackdrop.hidden = true; vscode.postMessage({ type: "newSession" }); });
  sessionsBackdrop.addEventListener("click", function(event) { if (event.target === sessionsBackdrop) sessionsBackdrop.hidden = true; });
  sessionSearch.addEventListener("input", renderSessions);
  document.addEventListener("keydown", function(event) { if (event.key === "Escape" && !sessionsBackdrop.hidden) { sessionsBackdrop.hidden = true; prompt.focus(); } });
  sessionList.addEventListener("click", function(event) { const action = event.target.closest("[data-session-action]"); if (!action) return; const row = action.closest(".session-row"); const sessionId = row && row.dataset.sessionId; if (action.dataset.sessionAction === "select" && sessionId) { if (sessionId === activeSessionId) { sessionsBackdrop.hidden = true; prompt.focus(); return; } if (busy) { showToast("Cancel or finish the current response before switching"); return; } saveConversationState(); row.classList.add("opening"); row.querySelector(".session-preview").textContent = "Opening…"; vscode.postMessage({ type: "switchSession", sessionId: sessionId }); } else if (action.dataset.sessionAction === "rename" && row) startRename(row); else if (action.dataset.sessionAction === "delete" && sessionId) { const item = sessions.find(function(entry) { return entry.sessionId === sessionId; }); vscode.postMessage({ type: "deleteSession", sessionId: sessionId, text: item ? item.displayName : "" }); } else if (action.dataset.sessionAction === "cancelRename") renderSessions(); });
  sessionList.addEventListener("submit", function(event) { event.preventDefault(); const row = event.target.closest(".session-row"); const input = event.target.querySelector("input"); if (row && input && input.value.trim()) { input.disabled = true; event.target.querySelector('button[type="submit"]').disabled = true; vscode.postMessage({ type: "renameSession", sessionId: row.dataset.sessionId, text: input.value.trim() }); } });
  document.getElementById("open-home").addEventListener("click", function() { vscode.postMessage({ type: "openHome" }); });
  contextRow.addEventListener("click", function(event) { const button = event.target.closest("button[data-context-key]"); if (button) vscode.postMessage({ type: "removeContext", key: button.dataset.contextKey }); });
  messages.addEventListener("click", function(event) { const link = event.target.closest("a[data-href]"); if (link) { event.preventDefault(); vscode.postMessage({ type: "openLink", href: link.dataset.href }); } const copy = event.target.closest("button[data-copy-code]"); if (copy) vscode.postMessage({ type: "copyText", text: decodeURIComponent(copy.dataset.copyCode) }); const insert = event.target.closest("button[data-insert-code]"); if (insert) vscode.postMessage({ type: "insertCode", text: decodeURIComponent(insert.dataset.insertCode) }); const turnAction = event.target.closest("button[data-turn-action]"); if (turnAction) { const turn = turnAction.closest(".turn"); if (turn) { let type = "copyText"; if (turnAction.dataset.turnAction === "share") type = "shareText"; if (turnAction.dataset.turnAction === "library") { type = "saveToLibrary"; turnAction.disabled = true; turnAction.textContent = "Saving…"; pendingLibraryButton = turnAction; } const previous = pairedUserTurn(turn); vscode.postMessage({ type: type, text: turn.dataset.raw || "", userText: previous ? previous.dataset.raw || "" : "" }); } } });
  window.addEventListener("message", function(event) { const message = event.data;
    if (message.type === "history") { saveConversationState(); currentSessionId = message.sessionId; sessionsBackdrop.hidden = true; restoreHistory(message.turns); restoreDraft(); prompt.focus(); persist(); }
    else if (message.type === "sessions") { sessionsLoading = false; sessionsError = ""; sessions = message.sessions || []; activeSessionId = message.activeSessionId; renderSessions(); }
    else if (message.type === "sessionsOpen") { sessionsBackdrop.hidden = false; sessionSearch.focus(); }
    else if (message.type === "sessionsLoading") { sessionsLoading = true; sessionsError = ""; renderSessions(); }
    else if (message.type === "sessionsError") { sessionsLoading = false; sessionsError = message.text; renderSessions(); }
    else if (message.type === "toast") showToast(message.text);
    else if (message.type === "user") addTurn("user", message.text);
    else if (message.type === "assistantDelta") appendAssistant(message.text);
    else if (message.type === "assistantReplace") replaceAssistant(message.text);
    else if (message.type === "status") setStatus(message.text, message.working !== false);
    else if (message.type === "toolStarted") addTool({ ...message, status: "running" });
    else if (message.type === "toolFinished") addTool(message);
    else if (message.type === "attention") addAttention(message);
    else if (message.type === "error") addError(message.text);
    else if (message.type === "done") { clearStatus(); setBusy(false); if (assistant) assistant.turn.classList.remove("streaming"); assistant = null; updateScrollAffordance(); prompt.focus(); }
    else if (message.type === "busy") setBusy(message.value);
    else if (message.type === "connection") { const dot = document.getElementById("connection-dot"); dot.className = "dot " + message.state; document.getElementById("connection-label").textContent = message.label; }
    else if (message.type === "context") { contextRow.innerHTML = ""; message.chips.forEach(function(chip) { const node = document.createElement("div"); node.className = "chip"; node.title = chip.detail || chip.label; node.innerHTML = '<span></span><button data-context-key="' + escapeHtml(chip.key) + '" aria-label="Remove context">×</button>'; node.querySelector("span").textContent = chip.label; contextRow.appendChild(node); }); if (message.canReset) { const reset = document.createElement("button"); reset.className = "chip"; reset.textContent = "Restore context"; reset.addEventListener("click", function() { vscode.postMessage({ type: "resetContext" }); }); contextRow.appendChild(reset); } suggestions.innerHTML = ""; (message.suggestions || []).forEach(function(label) { const button = document.createElement("button"); button.className = "suggestion"; button.textContent = label; suggestions.appendChild(button); }); }
    else if (message.type === "reset") { saveConversationState(); currentSessionId = message.sessionId; messages.innerHTML = ""; assistant = null; tools.clear(); clearStatus(); setBusy(false); showEmpty(); restoreDraft(); prompt.focus(); persist(); }
  });
  if (persisted.messages && persisted.messages.length) persisted.messages.forEach(function(item) { addTurn(item.role, item.content, true); });
  showEmpty(); restoreDraft(); updateScrollAffordance(); vscode.postMessage({ type: "ready" });
</script></body></html>`;
}

export function createNonce(): string {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  let value = "";
  for (let index = 0; index < 32; index += 1) value += alphabet[Math.floor(Math.random() * alphabet.length)];
  return value;
}
