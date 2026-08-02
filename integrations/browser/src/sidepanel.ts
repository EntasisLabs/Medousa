import {
  boundContext,
  contextSupplement,
  isBackgroundHandoffEvent,
  MedousaClient,
  MedousaHttpError,
  type ClientToolDefinition,
  type ClientToolRequest,
  type ClientToolResultRequest,
  type InteractiveTurnRequest,
  type InteractiveTurnResponse,
  type MedousaContext,
  type SessionHistoryResponse,
  type SessionSummary,
} from "@medousa/client";
import { captureActivePage } from "./pageContext.js";
import {
  DEFAULT_ENDPOINT,
  loadSession,
  loadClientId,
  loadSettings,
  saveSession,
  saveSettings,
  takePendingContext,
} from "./storage.js";
import {
  createProjectionState,
  projectStreamEvent,
  type ProjectedEvent,
} from "./projection.js";
import type { BrowserChatMessage, BrowserPageSnapshot, BrowserSettings, PendingContext } from "./types.js";

type ConnectionState = "checking" | "connected" | "recovering" | "error";
type Attention =
  | { kind: "budget"; requestId: string; message: string; rounds: number }
  | { kind: "permission"; requestId: string; message: string };

const $ = <T extends Element>(id: string): T => {
  const element = document.getElementById(id);
  if (!element) throw new Error(`Medousa browser UI is missing #${id}`);
  return element as unknown as T;
};

const transcript = $("transcript") as HTMLElement;
const emptyState = $("empty-state") as HTMLElement;
const messagesNode = $("messages") as HTMLElement;
const suggestionsNode = $("suggestions") as HTMLElement;
const streamStatus = $("stream-status") as HTMLElement;
const streamStatusText = $("stream-status-text") as HTMLElement;
const promptNode = $("prompt") as HTMLTextAreaElement;
const sendButton = $("send-button") as HTMLButtonElement;
const stopButton = $("stop-button") as HTMLButtonElement;
const sessionTitleNode = $("session-title") as HTMLElement;
const connectionDot = $("connection-dot") as HTMLElement;
const connectionLabel = $("connection-label") as HTMLElement;
const connectionBanner = $("connection-banner") as HTMLElement;
const bannerTitle = $("banner-title") as HTMLElement;
const bannerMessage = $("banner-message") as HTMLElement;
const contextTitle = $("context-title") as HTMLElement;
const contextDetail = $("context-detail") as HTMLElement;
const composerContext = $("composer-context") as HTMLElement;
const includePage = $("include-page") as HTMLInputElement;
const toast = $("toast") as HTMLElement;
const historyDialog = $("history-dialog") as HTMLDialogElement;
const sessionSearch = $("session-search") as HTMLInputElement;
const sessionList = $("session-list") as HTMLElement;
const settingsDialog = $("settings-dialog") as HTMLDialogElement;
const settingsForm = $("settings-form") as HTMLFormElement;
const endpointNode = $("endpoint") as HTMLInputElement;
const tokenNode = $("token") as HTMLInputElement;

let settings: BrowserSettings = { endpoint: DEFAULT_ENDPOINT, token: "" };
let client: MedousaClient | null = null;
let clientId = "";
let sessionId: string | null = null;
let sessionName: string | null = null;
let page: BrowserPageSnapshot = {
  title: "Current page",
  url: "",
  selection: "",
  text: "",
};
let messages: BrowserChatMessage[] = [];
let sessions: SessionSummary[] = [];
let busy = false;
let streamingText = "";
let statusText: string | null = null;
let statusWorking = false;
let attention: Attention | null = null;
let tools = new Map<string, { name: string; status: string }>();
let activeAbort: AbortController | null = null;
let activeTurn: InteractiveTurnResponse | null = null;
let pendingHistory: SessionHistoryResponse | null = null;
let lastPrompt: string | null = null;
let toastTimer: number | undefined;
const workshopWatchers = new Map<string, AbortController>();
let toolPumpAbort: AbortController | null = null;

const CLIENT_TOOL_DEFINITIONS: ClientToolDefinition[] = [
  {
    name: "browser_page_snapshot",
    description: "Read the current browser tab, including its title, URL, selection, and visible page text.",
    input_schema: {
      type: "object",
      properties: {
        include_text: {
          type: "boolean",
          description: "Include readable page text in the snapshot. Defaults to true.",
        },
      },
      additionalProperties: false,
    },
    effect_class: "external_read",
  },
];

function setConnection(state: ConnectionState, label: string): void {
  connectionDot.className = `status-dot ${state}`;
  connectionLabel.textContent = label;
}

function showConnectionError(error: unknown): void {
  setConnection("error", friendlyConnectionError(error));
  bannerTitle.textContent = error instanceof MedousaHttpError && [401, 403].includes(error.status)
    ? "Authorization needs attention"
    : "Connection needs attention";
  bannerMessage.textContent = friendlyConnectionError(error);
  connectionBanner.hidden = false;
}

function clearConnectionError(): void {
  connectionBanner.hidden = true;
}

function setStatus(text: string | null, working = false): void {
  statusText = text;
  statusWorking = working;
  streamStatus.hidden = !text;
  streamStatusText.textContent = text ?? "";
  const spinner = streamStatus.querySelector<HTMLElement>(".spinner");
  if (spinner) spinner.hidden = !working;
}

function showToast(message: string): void {
  toast.textContent = message;
  toast.hidden = false;
  if (toastTimer !== undefined) window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    toast.hidden = true;
  }, 2600);
}

function friendlyConnectionError(error: unknown): string {
  if (error instanceof MedousaHttpError) {
    if (error.status === 401 || error.status === 403) return "The workshop accepted the connection but requires a current bearer token.";
    return `Workshop request failed (${error.status}).`;
  }
  if (error instanceof Error && error.message && !error.message.toLowerCase().includes("failed to fetch")) {
    return error.message;
  }
  return "Could not reach the Medousa workshop. Check that the daemon is running.";
}

function isNotFound(error: unknown): boolean {
  return error instanceof MedousaHttpError && error.status === 404;
}

function sessionIdOf(session: SessionSummary): string | null {
  const id = session.session_id ?? session.id;
  return typeof id === "string" && id.trim() ? id : null;
}

function sessionDisplayName(session: SessionSummary): string {
  const display = typeof session.display_name === "string" ? session.display_name.trim() : "";
  if (display) return display;
  const preview = typeof session.preview === "string" ? stripContextSupplement(session.preview).trim() : "";
  return firstLine(preview) || "New conversation";
}

function firstLine(value: string): string {
  return value.split("\n").map((line) => line.trim()).find(Boolean) ?? "";
}

function stripContextSupplement(value: string): string {
  return value.replace(/\n*<medousa-context>[\s\S]*?<\/medousa-context>\s*$/i, "").trim();
}

function historySignature(history: SessionHistoryResponse): string {
  return history.turns.map((turn) => `${turn.role}\0${turn.timestamp}\0${turn.content}`).join("\x01");
}

function historyMessages(history: SessionHistoryResponse): BrowserChatMessage[] {
  return history.turns
    .filter((turn) => turn.role === "user" || turn.role === "assistant")
    .map((turn) => ({
      role: turn.role as "user" | "assistant",
      content: stripContextSupplement(turn.content),
      contextLabel: turn.role === "user" ? "Current tab" : undefined,
    }));
}

function isNearBottom(): boolean {
  return transcript.scrollHeight - transcript.scrollTop - transcript.clientHeight < 100;
}

function render(): void {
  const stickToBottom = isNearBottom();
  sessionTitleNode.textContent = sessionName?.trim() || "Medousa";
  renderContext();
  renderMessages();
  renderSuggestions();
  sendButton.disabled = busy || !promptNode.value.trim();
  stopButton.hidden = !busy;
  promptNode.disabled = busy;
  if (busy) {
    promptNode.placeholder = "Medousa is working…";
  } else if (page.selection) {
    promptNode.placeholder = "Ask about this selection…";
  } else {
    promptNode.placeholder = "Ask Medousa about this page…";
  }
  if (stickToBottom) transcript.scrollTop = transcript.scrollHeight;
}

function renderContext(): void {
  contextTitle.textContent = page.title || "Current page";
  const details: string[] = [];
  if (page.selection) details.push(`Selection · ${page.selection.length.toLocaleString()} chars`);
  if (page.text) details.push(`Page text · ${page.text.length.toLocaleString()} chars`);
  if (page.url) details.push(page.url);
  contextDetail.textContent = details.join(" · ") || "Page context is captured when you ask.";
  composerContext.textContent = page.selection ? "Selection + current tab" : "Current tab";
}

function renderSuggestions(): void {
  const visible = messages.length === 0 && !streamingText && !attention;
  emptyState.hidden = !visible;
  if (!visible) return;
  const suggestions = page.selection
    ? ["Explain this selection", "Summarize the selection", "Rewrite this clearly"]
    : ["Summarize this page", "What are the key takeaways?", "Extract action items"];
  suggestionsNode.replaceChildren(...suggestions.map((suggestion) => {
    const button = document.createElement("button");
    button.className = "suggestion";
    button.type = "button";
    button.textContent = suggestion;
    button.addEventListener("click", () => void sendPrompt(suggestion));
    return button;
  }));
}

function renderMessages(): void {
  messagesNode.replaceChildren();
  messages.forEach((message, index) => messagesNode.append(renderMessage(message, index)));
  if (streamingText) messagesNode.append(renderMessage({ role: "assistant", content: streamingText }, -1, true));

  if (tools.size > 0) {
    const list = document.createElement("div");
    list.className = "tool-list";
    for (const [runId, tool] of tools) {
      const row = document.createElement("div");
      row.className = `tool-row ${tool.status === "failed" ? "failed" : tool.status === "running" ? "" : "done"}`;
      row.innerHTML = "<span class=\"tool-dot\"></span>";
      const name = document.createElement("span");
      name.textContent = tool.name;
      const state = document.createElement("span");
      state.className = "tool-state";
      state.textContent = tool.status;
      row.append(name, state);
      row.dataset.runId = runId;
      list.append(row);
    }
    messagesNode.append(list);
  }

  if (attention) messagesNode.append(renderAttention(attention));
}

function renderMessage(message: BrowserChatMessage, index: number, streaming = false): HTMLElement {
  const article = document.createElement("article");
  article.className = `message ${message.role}${streaming ? " streaming" : ""}`;
  const label = document.createElement("div");
  label.className = "message-label";
  label.textContent = message.role === "user" ? `You · ${message.contextLabel ?? "Current tab"}` : message.role === "error" ? "Attention" : "Medousa";
  const bubble = document.createElement("div");
  bubble.className = "bubble";
  if (message.role === "assistant" && !streaming) {
    renderRichText(bubble, message.content);
  } else {
    bubble.textContent = message.content;
  }
  article.append(label, bubble);

  if (message.role === "assistant" && !streaming && index >= 0) {
    const actions = document.createElement("div");
    actions.className = "message-actions";
    const copy = document.createElement("button");
    copy.className = "message-action";
    copy.type = "button";
    copy.textContent = "Copy";
    copy.dataset.action = "copy-message";
    copy.dataset.index = String(index);
    actions.append(copy);
    article.append(actions);
  }
  if (message.role === "error" && lastPrompt && !busy) {
    const actions = document.createElement("div");
    actions.className = "message-actions";
    const retry = document.createElement("button");
    retry.className = "message-action";
    retry.type = "button";
    retry.textContent = "Retry";
    retry.dataset.action = "retry";
    actions.append(retry);
    article.append(actions);
  }
  return article;
}

function renderAttention(value: Attention): HTMLElement {
  const card = document.createElement("div");
  card.className = "attention-card";
  const title = document.createElement("strong");
  title.textContent = value.kind === "budget" ? "Medousa needs more tool rounds" : "Medousa needs your permission";
  const body = document.createElement("p");
  body.textContent = value.message;
  const actions = document.createElement("div");
  actions.className = "attention-actions";
  for (const [label, approve] of value.kind === "budget" ? [["Approve", true], ["Deny", false]] as const : [["Allow", true], ["Deny", false]] as const) {
    const button = document.createElement("button");
    button.className = approve ? "primary-button" : "secondary-button";
    button.type = "button";
    button.textContent = label;
    button.dataset.action = "attention";
    button.dataset.approve = String(approve);
    actions.append(button);
  }
  card.append(title, body, actions);
  return card;
}

function renderRichText(container: HTMLElement, value: string): void {
  const parts = value.split("```");
  parts.forEach((part, index) => {
    if (index % 2 === 1) {
      const lines = part.replace(/^\n/, "").split("\n");
      const language = /^[\w+-]+$/.test(lines[0] ?? "") ? lines.shift() ?? "" : "";
      const block = document.createElement("div");
      block.className = "code-block";
      const header = document.createElement("div");
      header.className = "code-header";
      const name = document.createElement("span");
      name.textContent = language || "code";
      const copy = document.createElement("button");
      copy.className = "code-copy";
      copy.type = "button";
      copy.textContent = "Copy";
      copy.addEventListener("click", () => void copyText(lines.join("\n")));
      header.append(name, copy);
      const pre = document.createElement("pre");
      pre.textContent = lines.join("\n");
      block.append(header, pre);
      container.append(block);
    } else if (part) {
      const paragraph = document.createElement("p");
      appendInlineText(paragraph, part);
      container.append(paragraph);
    }
  });
}

function appendInlineText(container: HTMLElement, value: string): void {
  const parts = value.split("`");
  parts.forEach((part, index) => {
    if (index % 2 === 1) {
      const code = document.createElement("code");
      code.className = "inline-code";
      code.textContent = part;
      container.append(code);
    } else {
      container.append(document.createTextNode(part));
    }
  });
}

async function copyText(value: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(value);
    showToast("Copied to clipboard");
  } catch {
    showToast("Could not access the clipboard");
  }
}

function stopClientToolPump(): void {
  toolPumpAbort?.abort();
  toolPumpAbort = null;
}

async function runClientToolPump(registeredClient: MedousaClient): Promise<void> {
  stopClientToolPump();
  const abort = new AbortController();
  toolPumpAbort = abort;
  while (!abort.signal.aborted && client === registeredClient) {
    try {
      const request = await registeredClient.nextClientToolRequest(clientId, 25_000, {
        signal: abort.signal,
      });
      if (!request) continue;
      const result = await handleClientToolRequest(request);
      if (abort.signal.aborted) break;
      await registeredClient.completeClientToolRequest(
        clientId,
        request.request_id,
        result,
        { signal: abort.signal },
      );
    } catch (error) {
      if (abort.signal.aborted) break;
      console.warn("Medousa client tool pump recovered after an error", error);
      await delay(1000, abort.signal);
    }
  }
  if (toolPumpAbort === abort) toolPumpAbort = null;
}

async function handleClientToolRequest(request: ClientToolRequest): Promise<ClientToolResultRequest> {
  if (request.tool_name !== "browser_page_snapshot") {
    return { error: `Unsupported browser client tool: ${request.tool_name}` };
  }
  try {
    const captured = await captureActivePage();
    const includeText = request.input.include_text !== false;
    page = captured;
    render();
    return {
      output: {
        ok: true,
        title: captured.title,
        url: captured.url,
        selection: captured.selection,
        text: includeText ? captured.text : "",
        captured_at_utc: new Date().toISOString(),
      },
    };
  } catch (error) {
    return {
      error: error instanceof Error ? error.message : "Could not capture the active browser page",
    };
  }
}

async function connectAndRestore(): Promise<void> {
  stopClientToolPump();
  setConnection("checking", "Checking workshop…");
  clearConnectionError();
  client = new MedousaClient({ baseUrl: settings.endpoint, bearerToken: settings.token || undefined });
  try {
    await client.health();
    await client.registerClient({
      client_id: clientId,
      channel_surface: "browser",
      supports_browser_host: false,
      tools: CLIENT_TOOL_DEFINITIONS,
    });
    const persisted = await loadSession();
    if (persisted.sessionId) {
      try {
        const history = await client.sessionHistory(persisted.sessionId);
        sessionId = persisted.sessionId;
        sessionName = persisted.sessionName;
        applyHistory(history);
      } catch (error) {
        if (!isNotFound(error)) throw error;
        await createSession();
      }
    } else {
      await createSession();
    }
    setConnection("connected", settings.endpoint);
    render();
    void runClientToolPump(client);
  } catch (error) {
    stopClientToolPump();
    client = null;
    showConnectionError(error);
    render();
  }
}

async function createSession(): Promise<void> {
  if (!client) throw new Error("Medousa is not connected");
  const created = await client.createSession({ catalog: "single" });
  sessionId = created.session_id;
  sessionName = created.display_name ?? null;
  messages = [];
  await saveSession({ sessionId, sessionName });
}

function applyHistory(history: SessionHistoryResponse): void {
  messages = historyMessages(history);
  streamingText = "";
  attention = null;
  tools.clear();
}

function buildContext(): MedousaContext {
  return boundContext({
    surface: "browser",
    title: page.title || undefined,
    url: page.url || undefined,
    pageText: includePage.checked ? page.text || undefined : undefined,
    selection: page.selection ? { text: page.selection } : undefined,
  });
}

async function sendPrompt(value?: string): Promise<void> {
  if (busy) return;
  const text = (value ?? promptNode.value).trim();
  if (!text) return;
  if (!client) {
    await connectAndRestore();
    if (!client) return;
  }
  if (!sessionId) await createSession();
  if (!sessionId || !client) return;

  const previousSelection = page.selection;
  const captured = await captureActivePage({ requestHostPermission: true });
  page = {
    ...captured,
    selection: captured.selection || previousSelection,
  };

  const turnSessionId = sessionId;
  lastPrompt = text;
  promptNode.value = "";
  messages.push({ role: "user", content: text, contextLabel: page.selection ? "Selection + current tab" : "Current tab" });
  streamingText = "";
  attention = null;
  tools.clear();
  busy = true;
  setStatus("Connecting to Medousa…", true);
  render();

  const abort = new AbortController();
  activeAbort = abort;
  let handedOff = false;
  try {
    const defaults = await client.runtimeDefaults({ signal: abort.signal });
    const context = buildContext();
    const request: InteractiveTurnRequest = {
      model: defaults.model,
      persist_user_turn: true,
      prompt: `${text}\n\n${contextSupplement(context)}`,
      provider: defaults.provider,
      response_depth_mode: defaults.response_depth_mode,
      reasoning_effort: defaults.reasoning_effort,
      session_id: turnSessionId,
      stage_routing: defaults.stage_routing as unknown as InteractiveTurnRequest["stage_routing"],
      media_refs: [],
      surface: {
        channel_surface: "browser",
        supports_browser_host: false,
        supports_ui_artifacts: false,
      },
    };
    setStatus("Medousa is thinking…", true);
    const response = await client.startTurn(request, { signal: abort.signal });
    activeTurn = response;
    const projection = createProjectionState();
    for await (const event of client.streamTurn(response, { signal: abort.signal, stopOnHandoff: true })) {
      for (const projected of projectStreamEvent(event, projection)) {
        if (projected.kind === "handoff") {
          handedOff = true;
          handleHandoff(projected);
        } else {
          handleProjected(projected);
        }
      }
      if (handedOff || event.terminal) break;
    }
    if (handedOff) {
      activeAbort = null;
      activeTurn = null;
      void followWorkshop(response, turnSessionId);
      return;
    }
  } catch (error) {
    if (!abort.signal.aborted) {
      streamingText = "";
      attention = null;
      messages.push({ role: "error", content: friendlyConnectionError(error) });
      setStatus(null);
      showConnectionError(error);
    }
  } finally {
    if (!handedOff) {
      busy = false;
      activeAbort = null;
      activeTurn = null;
      attention = null;
      tools.clear();
      setStatus(null);
      render();
      void refreshHistoryWhenIdle(turnSessionId);
    } else {
      render();
    }
  }
}

function handleProjected(event: Exclude<ProjectedEvent, { kind: "handoff" }>): void {
  switch (event.kind) {
    case "answer_delta":
      streamingText += event.text;
      break;
    case "answer_replace":
      streamingText = event.text;
      break;
    case "status":
      setStatus(event.text, true);
      break;
    case "tool_started":
      tools.set(event.runId, { name: event.name, status: "running" });
      break;
    case "tool_finished":
      tools.set(event.runId, { name: event.name, status: event.status });
      break;
    case "budget_request":
      attention = { kind: "budget", requestId: event.requestId, message: `Medousa needs ${event.rounds} more tool round${event.rounds === 1 ? "" : "s"} to finish.`, rounds: event.rounds };
      setStatus("Waiting for your budget decision", false);
      break;
    case "permission_request":
      attention = { kind: "permission", requestId: event.requestId, message: event.message };
      setStatus("Waiting for your permission decision", false);
      break;
    case "terminal":
      if (streamingText.trim()) messages.push({ role: "assistant", content: streamingText });
      if (event.error && event.text) messages.push({ role: "error", content: event.text });
      streamingText = "";
      attention = null;
      busy = false;
      setStatus(null);
      break;
  }
  render();
}

function handleHandoff(event: Extract<ProjectedEvent, { kind: "handoff" }>): void {
  streamingText = "";
  attention = null;
  busy = false;
  setStatus(`${event.text} · you can keep typing`, false);
  render();
}

async function followWorkshop(response: InteractiveTurnResponse, turnSessionId: string): Promise<void> {
  if (!client) return;
  const watcher = new AbortController();
  workshopWatchers.set(response.turn_id, watcher);
  try {
    for await (const event of client.streamTurn(response, { signal: watcher.signal, maxReconnectAttempts: 8 })) {
      if (isBackgroundHandoffEvent(event)) continue;
      if (event.terminal) {
        await reconcileWorkshopHistory(turnSessionId, watcher.signal, 8);
        return;
      }
    }
  } catch {
    if (!watcher.signal.aborted) await pollWorkshopHistory(turnSessionId, watcher.signal, 30);
  } finally {
    workshopWatchers.delete(response.turn_id);
  }
}

async function reconcileWorkshopHistory(turnSessionId: string, signal: AbortSignal, attempts: number): Promise<void> {
  if (!client || signal.aborted) return;
  for (let attempt = 0; attempt < attempts && !signal.aborted; attempt += 1) {
    try {
      const history = await client.sessionHistory(turnSessionId, { signal });
      if (sessionId !== turnSessionId || busy) {
        if (sessionId === turnSessionId) pendingHistory = history;
        return;
      }
      applyHistory(history);
      setStatus(null);
      setConnection("connected", settings.endpoint);
      render();
      return;
    } catch {
      if (attempt + 1 < attempts) await delay(500, signal);
    }
  }
}

async function pollWorkshopHistory(turnSessionId: string, signal: AbortSignal, attempts: number): Promise<void> {
  if (!client || signal.aborted) return;
  let previous = "";
  for (let attempt = 0; attempt < attempts && !signal.aborted; attempt += 1) {
    try {
      const history = await client.sessionHistory(turnSessionId, { signal });
      const signature = historySignature(history);
      if (signature !== previous && history.turns.some((turn) => turn.role === "assistant")) {
        previous = signature;
        if (sessionId === turnSessionId && !busy) {
          applyHistory(history);
          setStatus(null);
          render();
          return;
        }
        if (sessionId === turnSessionId) pendingHistory = history;
      }
    } catch {
      if (signal.aborted) return;
    }
    if (attempt + 1 < attempts) await delay(500, signal);
  }
}

async function refreshHistoryWhenIdle(turnSessionId: string): Promise<void> {
  if (!client || sessionId !== turnSessionId) return;
  pendingHistory = null;
  try {
    const history = await client.sessionHistory(turnSessionId);
    if (busy) pendingHistory = history;
    else applyHistory(history);
    render();
  } catch {
    // The local transcript remains usable if the reconciliation request fails.
  }
}

function delay(ms: number, signal: AbortSignal): Promise<void> {
  if (signal.aborted) return Promise.resolve();
  return new Promise((resolve) => {
    const timer = window.setTimeout(resolve, ms);
    signal.addEventListener("abort", () => {
      window.clearTimeout(timer);
      resolve();
    }, { once: true });
  });
}

async function stopActiveTurn(): Promise<void> {
  const abort = activeAbort;
  const currentSession = sessionId;
  activeAbort = null;
  if (abort) abort.abort();
  busy = false;
  attention = null;
  streamingText = "";
  tools.clear();
  messages.push({ role: "assistant", content: "Response stopped." });
  setStatus(null);
  render();
  if (client && currentSession) {
    try {
      await client.cancelTurn(currentSession);
    } catch {
      // The foreground stream is already stopped; cancellation can race a terminal event.
    }
  }
}

async function refreshPageContext(): Promise<void> {
  const previousSelection = page.selection;
  page = await captureActivePage({ requestHostPermission: true });
  if (!page.selection && previousSelection) page.selection = previousSelection;
  render();
  showToast(page.text ? "Page context refreshed" : "This page does not expose readable text");
}

async function openHistory(): Promise<void> {
  if (!client) {
    await connectAndRestore();
    if (!client) return;
  }
  try {
    sessions = await client.sessions(100);
    renderSessions();
    historyDialog.showModal();
  } catch (error) {
    showToast(friendlyConnectionError(error));
  }
}

function renderSessions(): void {
  const query = sessionSearch.value.trim().toLowerCase();
  const visible = sessions.filter((session) => {
    const name = sessionDisplayName(session).toLowerCase();
    const preview = typeof session.preview === "string" ? session.preview.toLowerCase() : "";
    return !query || name.includes(query) || preview.includes(query);
  });
  sessionList.replaceChildren();
  if (!visible.length) {
    const empty = document.createElement("div");
    empty.className = "session-empty";
    empty.textContent = query ? "No conversations match that search." : "No conversations yet.";
    sessionList.append(empty);
    return;
  }
  for (const session of visible) {
    const id = sessionIdOf(session);
    if (!id) continue;
    const row = document.createElement("div");
    row.className = `session-row ${id === sessionId ? "active" : ""}`;
    const select = document.createElement("button");
    select.className = "session-select";
    select.type = "button";
    select.dataset.action = "switch-session";
    select.dataset.sessionId = id;
    const name = document.createElement("span");
    name.className = "session-name";
    name.textContent = sessionDisplayName(session);
    const preview = document.createElement("span");
    preview.className = "session-preview";
    preview.textContent = firstLine(stripContextSupplement(typeof session.preview === "string" ? session.preview : "")) || "No messages yet";
    select.append(name, preview);
    const actions = document.createElement("div");
    actions.className = "session-actions";
    for (const [label, action] of [["Rename", "rename-session"], ["Delete", "delete-session"]] as const) {
      const button = document.createElement("button");
      button.className = "session-action";
      button.type = "button";
      button.textContent = label;
      button.title = label;
      button.dataset.action = action;
      button.dataset.sessionId = id;
      actions.append(button);
    }
    row.append(select, actions);
    sessionList.append(row);
  }
}

async function switchSession(nextId: string): Promise<void> {
  if (busy || !client || nextId === sessionId) return;
  try {
    const history = await client.sessionHistory(nextId);
    const selected = sessions.find((session) => sessionIdOf(session) === nextId);
    sessionId = nextId;
    sessionName = selected ? sessionDisplayName(selected) : null;
    await saveSession({ sessionId, sessionName });
    applyHistory(history);
    historyDialog.close();
    render();
  } catch (error) {
    showToast(friendlyConnectionError(error));
  }
}

async function renameSession(id: string): Promise<void> {
  if (!client) return;
  const current = sessions.find((session) => sessionIdOf(session) === id);
  const name = window.prompt("Name this conversation", current ? sessionDisplayName(current) : "")?.trim();
  if (!name) return;
  try {
    await client.renameSession(id, name);
    if (id === sessionId) {
      sessionName = name;
      await saveSession({ sessionId, sessionName });
    }
    sessions = await client.sessions(100);
    renderSessions();
    render();
  } catch (error) {
    showToast(friendlyConnectionError(error));
  }
}

async function deleteSession(id: string): Promise<void> {
  if (!client || busy || !window.confirm("Delete this conversation and its associated memory?")) return;
  try {
    await client.deleteSession(id, true);
    if (id === sessionId) {
      await createSession();
      messages = [];
    }
    sessions = await client.sessions(100);
    renderSessions();
    render();
  } catch (error) {
    showToast(friendlyConnectionError(error));
  }
}

async function createNewConversation(): Promise<void> {
  if (busy) {
    showToast("Stop the active response before starting a new conversation.");
    return;
  }
  if (!client) {
    await connectAndRestore();
    if (!client) return;
  }
  try {
    await createSession();
    messages = [];
    pendingHistory = null;
    historyDialog.close();
    setStatus(null);
    render();
    promptNode.focus();
  } catch (error) {
    showToast(friendlyConnectionError(error));
  }
}

function openSettings(): void {
  endpointNode.value = settings.endpoint;
  tokenNode.value = settings.token;
  settingsDialog.showModal();
}

async function saveConnectionSettings(): Promise<void> {
  const endpoint = endpointNode.value.trim().replace(/\/$/, "");
  let parsed: URL;
  try {
    parsed = new URL(endpoint);
    if (!/^https?:$/.test(parsed.protocol)) throw new Error("Workshop URL must use http or https.");
  } catch (error) {
    showToast(error instanceof Error ? error.message : "Enter a valid workshop URL.");
    return;
  }
  const isLocalWorkshop =
    parsed.protocol === "http:" &&
    (parsed.hostname === "localhost" || parsed.hostname === "127.0.0.1") &&
    parsed.port === "7419";
  if (!isLocalWorkshop && chrome.permissions?.request) {
    try {
      const granted = await chrome.permissions.request({
        origins: [`${parsed.protocol}//${parsed.host}/*`],
      });
      if (!granted) {
        showToast("Permission to reach that workshop was not granted.");
        return;
      }
    } catch {
      showToast("The browser could not grant access to that workshop.");
      return;
    }
  }
  settings = { endpoint, token: tokenNode.value };
  await saveSettings(settings);
  settingsDialog.close();
  await connectAndRestore();
}

function handleMessageClick(event: MouseEvent): void {
  const target = event.target instanceof Element ? event.target.closest<HTMLElement>("[data-action]") : null;
  if (!target) return;
  const action = target.dataset.action;
  if (action === "copy-message") {
    const index = Number(target.dataset.index);
    const message = messages[index];
    if (message) void copyText(message.content);
  } else if (action === "retry" && lastPrompt) {
    void sendPrompt(lastPrompt);
  } else if (action === "attention") {
    void resolveAttention(target.dataset.approve === "true");
  }
}

async function resolveAttention(approve: boolean): Promise<void> {
  if (!client || !attention) return;
  const current = attention;
  attention = null;
  setStatus(approve ? "Medousa is continuing…" : "Medousa was told to stop this path…", true);
  render();
  try {
    if (current.kind === "budget") {
      if (approve) await client.approveBudget(current.requestId, current.rounds, "browser");
      else await client.denyBudget(current.requestId, "browser");
    } else {
      await client.resolvePermission(current.requestId, approve, "browser");
    }
  } catch (error) {
    attention = current;
    setStatus("Waiting for your decision", false);
    showToast(friendlyConnectionError(error));
    render();
  }
}

function autoResizePrompt(): void {
  promptNode.style.height = "auto";
  promptNode.style.height = `${Math.min(promptNode.scrollHeight, 180)}px`;
  render();
}

async function applyPendingContext(pending: PendingContext): Promise<void> {
  const current = await captureActivePage();
  page = {
    ...current,
    ...pending.snapshot,
    text: current.text,
    selection: pending.snapshot.selection || current.selection,
  };
  if (pending.prompt && !busy) {
    promptNode.value = pending.prompt;
    promptNode.focus();
    autoResizePrompt();
  } else {
    render();
  }
}

async function initialize(): Promise<void> {
  settings = await loadSettings();
  clientId = await loadClientId();
  const pending = await takePendingContext();
  if (pending) {
    await applyPendingContext(pending);
  } else {
    page = await captureActivePage();
  }
  bindEvents();
  render();
  await connectAndRestore();
}

function bindEvents(): void {
  $("history-button").addEventListener("click", () => void openHistory());
  $("history-close").addEventListener("click", () => historyDialog.close());
  $("history-new").addEventListener("click", () => void createNewConversation());
  sessionSearch.addEventListener("input", renderSessions);
  sessionList.addEventListener("click", (event) => {
    const target = event.target instanceof Element ? event.target.closest<HTMLElement>("[data-action]") : null;
    const id = target?.dataset.sessionId;
    if (!target || !id) return;
    if (target.dataset.action === "switch-session") void switchSession(id);
    if (target.dataset.action === "rename-session") void renameSession(id);
    if (target.dataset.action === "delete-session") void deleteSession(id);
  });
  $("new-button").addEventListener("click", () => void createNewConversation());
  $("settings-button").addEventListener("click", openSettings);
  $("open-home-button").addEventListener("click", () => {
    window.open("medousa://chat", "_blank");
    showToast("Opening Medousa Home…");
  });
  $("settings-close").addEventListener("click", () => settingsDialog.close());
  $("settings-cancel").addEventListener("click", () => settingsDialog.close());
  settingsForm.addEventListener("submit", (event) => {
    event.preventDefault();
    void saveConnectionSettings();
  });
  $("retry-button").addEventListener("click", () => void connectAndRestore());
  $("refresh-context").addEventListener("click", () => void refreshPageContext());
  $("stop-button").addEventListener("click", () => void stopActiveTurn());
  chrome.storage.onChanged.addListener((changes, areaName) => {
    if (areaName !== "session" || !changes.pendingContext?.newValue) return;
    void takePendingContext().then((pending) => {
      if (pending) void applyPendingContext(pending);
    });
  });
  sendButton.addEventListener("click", () => void sendPrompt());
  promptNode.addEventListener("input", autoResizePrompt);
  promptNode.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void sendPrompt();
    }
  });
  messagesNode.addEventListener("click", handleMessageClick);
}

void initialize().catch((error) => {
  showConnectionError(error);
  render();
});
