import * as path from "node:path";
import * as vscode from "vscode";
import {
  boundContext,
  contextSupplement,
  MedousaClient,
  MedousaHttpError,
  type Diagnostic,
  type InteractiveTurnRequest,
  type MedousaContext,
  type SessionHistoryResponse,
} from "@medousa/client";
import { chatHtml, createNonce } from "./chatHtml.js";
import {
  createProjectionState,
  projectStreamEvent,
  type ProjectedEvent,
} from "./streamProjection.js";

const TOKEN_KEY = "medousa.bearerToken";
const SESSION_KEY = "medousa.sessionId";

export function activate(context: vscode.ExtensionContext): void {
  const chat = new MedousaChatView(context);
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider("medousa.chat", chat, {
      webviewOptions: { retainContextWhenHidden: true },
    }),
    vscode.commands.registerCommand("medousa.ask", async () => {
      await vscode.commands.executeCommand("workbench.view.extension.medousa");
      chat.show();
      const prompt = await vscode.window.showInputBox({
        prompt: "Ask Medousa",
        placeHolder: "Explain this, find the bug, or connect it to my vault…",
        ignoreFocusOut: true,
      });
      if (prompt?.trim()) await chat.sendPrompt(prompt.trim(), true);
    }),
    vscode.commands.registerCommand("medousa.configureConnection", async () => {
      await configureConnection(context);
      await chat.refresh();
    }),
    vscode.commands.registerCommand("medousa.newConversation", () => chat.newSession()),
    vscode.window.onDidChangeActiveTextEditor(() => chat.refreshContext()),
    vscode.window.onDidChangeTextEditorSelection(() => chat.refreshContext()),
    vscode.languages.onDidChangeDiagnostics(() => chat.refreshContext()),
    { dispose: () => chat.dispose() },
  );
}

export function deactivate(): void {}

class MedousaChatView implements vscode.WebviewViewProvider {
  private view: vscode.WebviewView | null = null;
  private client: MedousaClient | null = null;
  private sessionId: string | null = null;
  private abortController: AbortController | null = null;
  private disabledContext = new Set<string>();
  private lastPrompt: string | null = null;

  constructor(private readonly context: vscode.ExtensionContext) {}

  resolveWebviewView(view: vscode.WebviewView): void {
    this.view = view;
    view.webview.options = { enableScripts: true };
    view.webview.html = chatHtml(createNonce());
    view.webview.onDidReceiveMessage(
      (message: unknown) => this.handleMessage(message),
      null,
      this.context.subscriptions,
    );
  }

  show(): void {
    this.view?.show(true);
  }

  dispose(): void {
    this.abortController?.abort();
  }

  async refresh(): Promise<void> {
    if (!this.view) return;
    this.post({ type: "connection", state: "checking", label: "Checking workshop…" });
    try {
      this.client = await createClient(this.context);
      const restored = await this.restoreOrCreateSession(this.client);
      this.sessionId = restored.session_id;
      this.post({
        type: "history",
        turns: restored.turns.map((turn) => ({
          ...turn,
          content: stripContextSupplement(turn.content),
        })),
      });
      this.post({ type: "connection", state: "connected", label: endpointLabel() });
      this.refreshContext();
    } catch (error) {
      this.client = null;
      this.sessionId = null;
      this.post({ type: "connection", state: connectionState(error), label: friendlyConnectionError(error) });
    }
  }

  refreshContext(): void {
    if (!this.view) return;
    this.post({
      type: "context",
      chips: contextChips(vscode.window.activeTextEditor, this.disabledContext),
      canReset: this.disabledContext.size > 0,
    });
  }

  async sendPrompt(prompt: string, echoUser = false): Promise<void> {
    if (this.abortController) return;
    this.lastPrompt = prompt;
    if (echoUser) this.post({ type: "user", text: prompt });
    this.post({ type: "busy", value: true });
    this.post({ type: "status", text: "Connecting to Medousa…", working: true });

    if (!this.client || !this.sessionId) await this.refresh();
    const client = this.client;
    const sessionId = this.sessionId;
    if (!client || !sessionId) {
      this.post({ type: "error", text: "Medousa is unavailable. Check the workshop connection and try again." });
      this.post({ type: "busy", value: false });
      return;
    }

    this.abortController = new AbortController();
    try {
      const defaults = await client.runtimeDefaults({ signal: this.abortController.signal });
      const editorContext = currentContext(vscode.window.activeTextEditor, this.disabledContext);
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

      this.post({ type: "status", text: "Medousa is thinking…", working: true });
      const turn = await client.startTurn(request, { signal: this.abortController.signal });
      const projection = createProjectionState(showEngineDetails());
      for await (const event of client.streamTurn(turn, { signal: this.abortController.signal })) {
        for (const projected of projectStreamEvent(event, projection)) this.postProjected(projected);
      }
      this.post({ type: "done" });
      this.post({ type: "connection", state: "connected", label: endpointLabel() });
    } catch (error) {
      if (this.abortController.signal.aborted) {
        this.post({ type: "status", text: "Cancelled", working: false });
        this.post({ type: "done" });
      } else {
        this.post({ type: "error", text: errorMessage(error) });
        if (isConnectionError(error)) {
          this.post({ type: "connection", state: "reconnecting", label: "Connection interrupted" });
        }
      }
    } finally {
      this.abortController = null;
    }
  }

  async newSession(): Promise<void> {
    await this.cancel();
    try {
      if (!this.client) this.client = await createClient(this.context);
      const created = await this.client.createSession({
        catalog: "single",
        display_name: sessionDisplayName(),
      });
      this.sessionId = created.session_id;
      await this.context.workspaceState.update(SESSION_KEY, created.session_id);
      this.disabledContext.clear();
      this.post({ type: "reset" });
      this.refreshContext();
    } catch (error) {
      this.post({ type: "error", text: errorMessage(error) });
    }
  }

  private async cancel(): Promise<void> {
    this.abortController?.abort();
    if (this.client && this.sessionId) {
      try {
        await this.client.cancelTurn(this.sessionId);
      } catch {
        // Local stream cancellation still succeeds when the daemon is already terminal.
      }
    }
    this.abortController = null;
  }

  private async restoreOrCreateSession(client: MedousaClient): Promise<SessionHistoryResponse> {
    const existing = this.context.workspaceState.get<string>(SESSION_KEY);
    if (existing) {
      try {
        return await client.sessionHistory(existing);
      } catch (error) {
        if (!(error instanceof MedousaHttpError) || error.status !== 404) throw error;
        await this.context.workspaceState.update(SESSION_KEY, undefined);
      }
    }
    const created = await client.createSession({ catalog: "single", display_name: sessionDisplayName() });
    await this.context.workspaceState.update(SESSION_KEY, created.session_id);
    return { session_id: created.session_id, turns: [] };
  }

  private async handleMessage(message: unknown): Promise<void> {
    if (!isInboundMessage(message)) return;
    switch (message.type) {
      case "ready":
        await this.refresh();
        break;
      case "send":
        if (message.text?.trim()) await this.sendPrompt(message.text.trim());
        break;
      case "cancel":
        await this.cancel();
        this.post({ type: "done" });
        break;
      case "configure":
        await configureConnection(this.context);
        await this.refresh();
        break;
      case "newSession":
        await this.newSession();
        break;
      case "retry":
        if (this.lastPrompt) await this.sendPrompt(this.lastPrompt);
        break;
      case "openHome":
        await vscode.env.openExternal(vscode.Uri.parse("medousa://chat"));
        break;
      case "removeContext":
        if (message.key) this.disabledContext.add(message.key);
        this.refreshContext();
        break;
      case "resetContext":
        this.disabledContext.clear();
        this.refreshContext();
        break;
      case "insertCode":
        if (message.text) await insertAtSelection(message.text);
        break;
      case "openLink":
        if (message.href) await openSafeLink(message.href);
        break;
      case "budget":
        if (this.client && message.requestId) {
          if (message.approve) await this.client.approveBudget(message.requestId, message.rounds);
          else await this.client.denyBudget(message.requestId);
          this.post({ type: "status", text: message.approve ? "Approved — continuing…" : "Request denied", working: Boolean(message.approve) });
        }
        break;
      case "permission":
        if (this.client && message.requestId) {
          await this.client.resolvePermission(message.requestId, Boolean(message.approve));
          this.post({ type: "status", text: message.approve ? "Permission approved — continuing…" : "Permission denied", working: Boolean(message.approve) });
        }
        break;
    }
  }

  private post(message: OutboundMessage): void {
    void this.view?.webview.postMessage(message);
  }

  private postProjected(event: ProjectedEvent): void {
    switch (event.kind) {
      case "answer_delta":
        this.post({ type: "assistantDelta", text: event.text });
        break;
      case "answer_replace":
        this.post({ type: "assistantReplace", text: event.text });
        break;
      case "status":
        this.post({ type: "status", text: event.text, working: true });
        break;
      case "tool_started":
        this.post({ type: "toolStarted", runId: event.runId, name: event.name, summary: event.summary });
        break;
      case "tool_finished":
        this.post({ type: "toolFinished", runId: event.runId, name: event.name, status: event.status, summary: event.summary });
        break;
      case "terminal":
        if (event.error && event.text) this.post({ type: "error", text: event.text });
        break;
      case "budget_request":
        this.post({ type: "attention", kind: "budget", requestId: event.requestId, rounds: event.rounds, text: `Medousa needs ${event.rounds} more tool round${event.rounds === 1 ? "" : "s"} to finish.` });
        break;
      case "permission_request":
        this.post({ type: "attention", kind: "permission", requestId: event.requestId, text: event.message });
        break;
    }
  }
}

async function configureConnection(context: vscode.ExtensionContext): Promise<void> {
  const config = vscode.workspace.getConfiguration("medousa");
  const currentEndpoint = config.get<string>("endpoint", "http://127.0.0.1:7419");
  const endpoint = await vscode.window.showInputBox({ prompt: "Medousa workshop URL", value: currentEndpoint, ignoreFocusOut: true });
  if (!endpoint?.trim()) return;
  await config.update("endpoint", endpoint.trim(), vscode.ConfigurationTarget.Global);
  const token = await vscode.window.showInputBox({ prompt: "Bearer token (leave blank to keep the current token)", password: true, ignoreFocusOut: true });
  if (token?.trim()) await context.secrets.store(TOKEN_KEY, token.trim());
  vscode.window.showInformationMessage("Medousa connection saved.");
}

async function createClient(context: vscode.ExtensionContext): Promise<MedousaClient> {
  const endpoint = vscode.workspace.getConfiguration("medousa").get<string>("endpoint", "http://127.0.0.1:7419");
  const token = await context.secrets.get(TOKEN_KEY);
  const client = new MedousaClient({ baseUrl: endpoint, bearerToken: token });
  await client.health();
  return client;
}

function currentContext(editor: vscode.TextEditor | undefined, disabled: Set<string>): MedousaContext {
  if (!editor) return { surface: "vscode" };
  const document = editor.document;
  const selection = editor.selection;
  const diagnostics: Diagnostic[] = disabled.has("diagnostics") ? [] : vscode.languages.getDiagnostics(document.uri).map((item) => ({
    message: item.message,
    severity: item.severity === vscode.DiagnosticSeverity.Error ? "error" : item.severity === vscode.DiagnosticSeverity.Warning ? "warning" : "info",
    source: item.source,
    range: { start: { line: item.range.start.line, character: item.range.start.character }, end: { line: item.range.end.line, character: item.range.end.character } },
  }));
  return boundContext({
    surface: "vscode",
    workspace: disabled.has("workspace") ? undefined : vscode.workspace.workspaceFile?.fsPath ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath,
    file: disabled.has("file") ? undefined : document.uri.fsPath,
    language: disabled.has("file") ? undefined : document.languageId,
    selection: disabled.has("selection") || selection.isEmpty ? undefined : {
      text: document.getText(selection),
      start: { line: selection.start.line, character: selection.start.character },
      end: { line: selection.end.line, character: selection.end.character },
    },
    diagnostics,
  });
}

function contextChips(editor: vscode.TextEditor | undefined, disabled: Set<string>): ContextChip[] {
  if (!editor) return [];
  const chips: ContextChip[] = [];
  const workspacePath = vscode.workspace.workspaceFile?.fsPath ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  if (workspacePath && !disabled.has("workspace")) chips.push({ key: "workspace", label: path.basename(workspacePath), detail: workspacePath });
  if (!disabled.has("file")) chips.push({ key: "file", label: path.basename(editor.document.uri.fsPath), detail: editor.document.uri.fsPath });
  if (!editor.selection.isEmpty && !disabled.has("selection")) chips.push({ key: "selection", label: `${editor.document.getText(editor.selection).length} selected`, detail: "Selected editor text" });
  const diagnosticCount = vscode.languages.getDiagnostics(editor.document.uri).length;
  if (diagnosticCount > 0 && !disabled.has("diagnostics")) chips.push({ key: "diagnostics", label: `${diagnosticCount} diagnostic${diagnosticCount === 1 ? "" : "s"}`, detail: "Problems for the active file" });
  return chips;
}

async function insertAtSelection(text: string): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor) return;
  const choice = await vscode.window.showInformationMessage("Insert this code at the current selection?", { modal: true }, "Insert");
  if (choice === "Insert") await editor.edit((builder) => builder.replace(editor.selection, text));
}

async function openSafeLink(href: string): Promise<void> {
  let uri: vscode.Uri;
  try {
    uri = vscode.Uri.parse(href, true);
  } catch {
    return;
  }
  if (!["http", "https", "medousa"].includes(uri.scheme)) return;
  await vscode.env.openExternal(uri);
}

function sessionDisplayName(): string {
  return vscode.workspace.name ? `VS Code — ${vscode.workspace.name}` : "VS Code";
}

function endpointLabel(): string {
  const endpoint = vscode.workspace.getConfiguration("medousa").get<string>("endpoint", "http://127.0.0.1:7419");
  try {
    return new URL(endpoint).host;
  } catch {
    return "Connected";
  }
}

function showEngineDetails(): boolean {
  return vscode.workspace.getConfiguration("medousa").get<boolean>("showEngineDetails", false);
}

function connectionState(error: unknown): "unauthorized" | "unavailable" {
  return error instanceof MedousaHttpError && (error.status === 401 || error.status === 403) ? "unauthorized" : "unavailable";
}

function friendlyConnectionError(error: unknown): string {
  return connectionState(error) === "unauthorized" ? "Authorization required" : "Workshop unavailable";
}

function isConnectionError(error: unknown): boolean {
  return error instanceof TypeError || (error instanceof MedousaHttpError && error.status >= 500);
}

function errorMessage(error: unknown): string {
  if (error instanceof MedousaHttpError) {
    if (error.status === 401 || error.status === 403) return "This workshop needs a valid pairing token. Open connection settings and try again.";
    if (error.status === 404) return "The active Medousa session no longer exists. Start a new conversation and try again.";
  }
  return error instanceof Error ? error.message : String(error);
}

function stripContextSupplement(content: string): string {
  return content.replace(/\n*<medousa-context>[\s\S]*?<\/medousa-context>\s*$/i, "").trimEnd();
}

type ContextChip = { key: string; label: string; detail?: string };
type ConnectionState = "checking" | "connected" | "reconnecting" | "unavailable" | "unauthorized";

type OutboundMessage =
  | { type: "user"; text: string }
  | { type: "assistantDelta"; text: string }
  | { type: "assistantReplace"; text: string }
  | { type: "status"; text: string; working: boolean }
  | { type: "toolStarted"; runId: string; name: string; summary?: string }
  | { type: "toolFinished"; runId: string; name: string; status: string; summary?: string }
  | { type: "attention"; kind: "budget" | "permission"; requestId: string; text: string; rounds?: number }
  | { type: "error"; text: string }
  | { type: "done" }
  | { type: "busy"; value: boolean }
  | { type: "history"; turns: SessionHistoryResponse["turns"] }
  | { type: "connection"; state: ConnectionState; label: string }
  | { type: "context"; chips: ContextChip[]; canReset: boolean }
  | { type: "reset" };

type InboundMessage = {
  type: "ready" | "send" | "cancel" | "configure" | "newSession" | "retry" | "openHome" | "removeContext" | "resetContext" | "insertCode" | "openLink" | "budget" | "permission";
  text?: string;
  key?: string;
  href?: string;
  requestId?: string;
  approve?: boolean;
  rounds?: number;
};

function isInboundMessage(value: unknown): value is InboundMessage {
  return Boolean(value && typeof value === "object" && "type" in value && [
    "ready", "send", "cancel", "configure", "newSession", "retry", "openHome", "removeContext", "resetContext", "insertCode", "openLink", "budget", "permission",
  ].includes(String(value.type)));
}
