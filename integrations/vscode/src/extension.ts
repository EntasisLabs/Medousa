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
  context.subscriptions.push(
    vscode.commands.registerCommand("medousa.ask", () => askAboutActiveEditor(context)),
    vscode.commands.registerCommand("medousa.configureConnection", () => configureConnection(context)),
  );
}

export function deactivate(): void {
  activeAbortController?.abort();
}

let activeAbortController: AbortController | null = null;

async function askAboutActiveEditor(context: vscode.ExtensionContext): Promise<void> {
  const prompt = await vscode.window.showInputBox({
    prompt: "What should Medousa help with?",
    placeHolder: "Explain this, find the bug, or connect it to my vault…",
    ignoreFocusOut: true,
  });
  if (!prompt?.trim()) return;

  const editor = vscode.window.activeTextEditor;
  const medousaContext = editor ? contextFromEditor(editor) : { surface: "vscode" as const };
  const client = await createClient(context);
  if (!client) return;

  const sessionId = await ensureSession(client, context);
  const defaults = await client.runtimeDefaults();
  const request: InteractiveTurnRequest = {
    model: defaults.model,
    persist_user_turn: true,
    prompt: `${prompt.trim()}\n\n${contextSupplement(medousaContext)}`,
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

  const panel = vscode.window.createWebviewPanel(
    "medousaResponse",
    "Medousa",
    vscode.ViewColumn.Beside,
    { enableScripts: true },
  );
  panel.webview.html = responseHtml();
  activeAbortController?.abort();
  activeAbortController = new AbortController();
  panel.onDidDispose(() => activeAbortController?.abort(), null, context.subscriptions);

  try {
    const turn = await client.startTurn(request, { signal: activeAbortController.signal });
    for await (const event of client.streamTurn(turn, { signal: activeAbortController.signal })) {
      panel.webview.postMessage({
        type: event.terminal ? "complete" : "event",
        text: event.content_delta ?? event.final_text ?? event.message,
      });
    }
  } catch (error) {
    if (!activeAbortController.signal.aborted) {
      panel.webview.postMessage({ type: "error", text: errorMessage(error) });
    }
  } finally {
    activeAbortController = null;
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

async function createClient(context: vscode.ExtensionContext): Promise<MedousaClient | null> {
  const endpoint = vscode.workspace
    .getConfiguration("medousa")
    .get<string>("endpoint", "http://127.0.0.1:7419");
  const token = await context.secrets.get(TOKEN_KEY);
  const client = new MedousaClient({ baseUrl: endpoint, bearerToken: token });
  try {
    await client.health();
    return client;
  } catch (error) {
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
  const diagnostics: Diagnostic[] = vscode.languages
    .getDiagnostics(document.uri)
    .map((item) => ({
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

function responseHtml(): string {
  return `<!doctype html><html><body><pre id="output">Medousa is thinking…</pre><script>
    const output = document.getElementById("output");
    window.addEventListener("message", event => {
      const message = event.data;
      if (message.type === "error") output.textContent += "\\n\\nError: " + message.text;
      else if (message.type === "event") output.textContent += message.text || "";
      else if (message.type === "complete") output.textContent += message.text || "";
    });
  </script></body></html>`;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
