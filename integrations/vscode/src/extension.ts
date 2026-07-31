import * as vscode from "vscode";
import {
  boundContext,
  contextSupplement,
  MedousaClient,
  type Diagnostic,
  type InteractiveTurnRequest,
  type MedousaContext,
} from "@medousa/client";

const TOKEN_KEY = "medousa.bearerToken";
const SESSION_KEY = "medousa.sessionId";

export function activate(context: vscode.ExtensionContext): void {
  const chat = new MedousaChatView(context);
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider("medousa.chat", chat, {
      webviewOptions: { retainContextWhenHidden: true },
    }),
    vscode.commands.registerCommand("medousa.ask", async () => {
      const prompt = await vscode.window.showInputBox({
        prompt: "Ask Medousa",
        placeHolder: "Explain this, find the bug, or connect it to my vault…",
        ignoreFocusOut: true,
      });
      if (prompt?.trim()) await chat.sendPrompt(prompt.trim());
    }),
    vscode.commands.registerCommand("medousa.configureConnection", () =>
      configureConnection(context),
    ),
  );
}

export function deactivate(): void {
  activeAbortController?.abort();
}

let activeAbortController: AbortController | null = null;

class MedousaChatView implements vscode.WebviewViewProvider {
  private view: vscode.WebviewView | null = null;

  constructor(private readonly context: vscode.ExtensionContext) {}

  resolveWebviewView(view: vscode.WebviewView): void {
    this.view = view;
    view.webview.options = { enableScripts: true };
    view.webview.html = chatHtml();
    view.webview.onDidReceiveMessage(async (message: unknown) => {
      if (!isMessage(message)) return;
      if (message.type === "send" && message.text.trim()) await this.sendPrompt(message.text.trim());
      if (message.type === "cancel") activeAbortController?.abort();
      if (message.type === "configure") await configureConnection(this.context);
    }, null, this.context.subscriptions);
  }

  async sendPrompt(prompt: string): Promise<void> {
    this.post({ type: "user", text: prompt });
    this.post({ type: "status", text: "Connecting to Medousa…" });

    const client = await createClient(this.context, (text) => this.post({ type: "error", text }));
    if (!client) return;

    try {
      const sessionId = await ensureSession(client, this.context);
      const defaults = await client.runtimeDefaults();
      const editorContext = vscode.window.activeTextEditor
        ? contextFromEditor(vscode.window.activeTextEditor)
        : ({ surface: "vscode" } satisfies MedousaContext);
      const request: InteractiveTurnRequest = {
        model: defaults.model,
        persist_user_turn: true,
        prompt: `${prompt}\n\n${contextSupplement(editorContext)}`,
        provider: defaults.provider,
        response_depth_mode: defaults.response_depth_mode,
        reasoning_effort: defaults.reasoning_effort,
        session_id: sessionId,
        stage_routing: defaults.stage_routing as unknown as InteractiveTurnRequest["stage_routing"],
        media_refs: [],
        surface: {
          channel_surface: "vscode",
          supports_browser_host: false,
          supports_ui_artifacts: false,
        },
      };

      activeAbortController?.abort();
      activeAbortController = new AbortController();
      this.post({ type: "status", text: "Medousa is thinking…" });
      const turn = await client.startTurn(request, { signal: activeAbortController.signal });

      for await (const event of client.streamTurn(turn, { signal: activeAbortController.signal })) {
        if (event.content_delta) this.post({ type: "assistantDelta", text: event.content_delta });
        if (event.terminal && event.final_text && !event.content_delta) {
          this.post({ type: "assistantDelta", text: event.final_text });
        }
        if (!event.terminal && event.operator_message) {
          this.post({ type: "status", text: event.operator_message });
        }
      }
      this.post({ type: "done" });
    } catch (error) {
      if (!activeAbortController?.signal.aborted) this.post({ type: "error", text: errorMessage(error) });
      else this.post({ type: "status", text: "Cancelled" });
    } finally {
      activeAbortController = null;
    }
  }

  private post(message: ChatMessage): void {
    void this.view?.webview.postMessage(message);
  }
}

async function configureConnection(context: vscode.ExtensionContext): Promise<void> {
  const config = vscode.workspace.getConfiguration("medousa");
  const currentEndpoint = config.get<string>("endpoint", "http://127.0.0.1:7419");
  const endpoint = await vscode.window.showInputBox({
    prompt: "Medousa workshop URL",
    value: currentEndpoint,
    ignoreFocusOut: true,
  });
  if (!endpoint?.trim()) return;
  await config.update("endpoint", endpoint.trim(), vscode.ConfigurationTarget.Global);

  const token = await vscode.window.showInputBox({
    prompt: "Bearer token (leave blank to keep the current token)",
    password: true,
    ignoreFocusOut: true,
  });
  if (token?.trim()) await context.secrets.store(TOKEN_KEY, token.trim());
  vscode.window.showInformationMessage("Medousa connection saved.");
}

async function createClient(
  context: vscode.ExtensionContext,
  reportError: (text: string) => void,
): Promise<MedousaClient | null> {
  const endpoint = vscode.workspace
    .getConfiguration("medousa")
    .get<string>("endpoint", "http://127.0.0.1:7419");
  const token = await context.secrets.get(TOKEN_KEY);
  const client = new MedousaClient({ baseUrl: endpoint, bearerToken: token });
  try {
    await client.health();
    return client;
  } catch (error) {
    reportError(`Medousa is unavailable: ${errorMessage(error)}`);
    const action = await vscode.window.showErrorMessage(
      `Medousa is unavailable: ${errorMessage(error)}`,
      "Configure Connection",
    );
    if (action === "Configure Connection") await configureConnection(context);
    return null;
  }
}

async function ensureSession(
  client: MedousaClient,
  context: vscode.ExtensionContext,
): Promise<string> {
  const existing = context.workspaceState.get<string>(SESSION_KEY);
  if (existing) return existing;
  const created = await client.createSession({
    catalog: "single",
    display_name: vscode.workspace.name ? `VS Code — ${vscode.workspace.name}` : "VS Code",
  });
  await context.workspaceState.update(SESSION_KEY, created.session_id);
  return created.session_id;
}

function contextFromEditor(editor: vscode.TextEditor): MedousaContext {
  const document = editor.document;
  const selection = editor.selection;
  const diagnostics: Diagnostic[] = vscode.languages.getDiagnostics(document.uri).map((item) => ({
    message: item.message,
    severity: item.severity === vscode.DiagnosticSeverity.Error
      ? "error"
      : item.severity === vscode.DiagnosticSeverity.Warning
        ? "warning"
        : "info",
    source: item.source,
    range: {
      start: { line: item.range.start.line, character: item.range.start.character },
      end: { line: item.range.end.line, character: item.range.end.character },
    },
  }));

  return boundContext({
    surface: "vscode",
    workspace: vscode.workspace.workspaceFile?.fsPath ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath,
    file: document.uri.fsPath,
    language: document.languageId,
    selection: {
      text: document.getText(selection),
      start: { line: selection.start.line, character: selection.start.character },
      end: { line: selection.end.line, character: selection.end.character },
    },
    diagnostics,
  });
}

type ChatMessage =
  | { type: "user"; text: string }
  | { type: "assistantDelta"; text: string }
  | { type: "status"; text: string }
  | { type: "error"; text: string }
  | { type: "done" };

function isMessage(value: unknown): value is { type: "send" | "cancel" | "configure"; text: string } {
  return Boolean(value && typeof value === "object" && "type" in value &&
    (["send", "cancel", "configure"] as string[]).includes(String(value.type)));
}

function chatHtml(): string {
  return `<!doctype html>
<html><head><meta charset="UTF-8"><style>
  :root { color-scheme: light dark; }
  body { padding: 0 10px 10px; color: var(--vscode-foreground); font-family: var(--vscode-font-family); font-size: var(--vscode-font-size); }
  #messages { display: flex; flex-direction: column; gap: 10px; padding-bottom: 10px; }
  .message { white-space: pre-wrap; overflow-wrap: anywhere; border-radius: 6px; padding: 8px 10px; }
  .user { background: var(--vscode-textBlockQuote-background); border-left: 2px solid var(--vscode-textLink-foreground); }
  .assistant { background: var(--vscode-editor-inactiveSelectionBackground); }
  .status { color: var(--vscode-descriptionForeground); font-size: 0.9em; }
  .error { color: var(--vscode-errorForeground); }
  textarea { box-sizing: border-box; width: 100%; min-height: 70px; resize: vertical; color: var(--vscode-input-foreground); background: var(--vscode-input-background); border: 1px solid var(--vscode-input-border, transparent); padding: 8px; }
  .actions { display: flex; gap: 6px; margin-top: 6px; }
  button { flex: 1; padding: 6px 8px; color: var(--vscode-button-foreground); background: var(--vscode-button-background); border: 0; cursor: pointer; }
  button:hover { background: var(--vscode-button-hoverBackground); }
  button.secondary { color: var(--vscode-foreground); background: var(--vscode-editor-inactiveSelectionBackground); }
</style></head><body>
  <div id="messages"><div class="status">Ask Medousa about your code, workspace, or vault.</div></div>
  <textarea id="prompt" aria-label="Message Medousa" placeholder="Message Medousa…"></textarea>
  <div class="actions"><button id="send">Send</button><button id="cancel" class="secondary">Cancel</button></div>
  <script>
    const vscode = acquireVsCodeApi();
    const messages = document.getElementById("messages");
    const prompt = document.getElementById("prompt");
    let assistant = null;
    function add(kind, text) { const node = document.createElement("div"); node.className = "message " + kind; node.textContent = text; messages.appendChild(node); return node; }
    document.getElementById("send").addEventListener("click", () => { const text = prompt.value.trim(); if (!text) return; add("user", text); prompt.value = ""; assistant = null; vscode.postMessage({ type: "send", text }); });
    document.getElementById("cancel").addEventListener("click", () => vscode.postMessage({ type: "cancel", text: "" }));
    prompt.addEventListener("keydown", event => { if ((event.metaKey || event.ctrlKey) && event.key === "Enter") document.getElementById("send").click(); });
    window.addEventListener("message", event => { const message = event.data; if (message.type === "assistantDelta") { if (!assistant) assistant = add("assistant", ""); assistant.textContent += message.text; } else if (message.type === "status") add("status", message.text); else if (message.type === "error") add("error", message.text); });
  </script></body></html>`;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
