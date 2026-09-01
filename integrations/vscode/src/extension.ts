import * as path from "node:path";
import * as vscode from "vscode";
import {
  boundContext,
  hostContext,
  isBackgroundHandoffEvent,
  isTurnStreamTerminal,
  MedousaClient,
  MedousaHttpError,
  type Diagnostic,
  type AgentModeAvailability,
  type AgentModeId,
  type AgentModeProposalResponse,
  type ForgeUndertaking,
  type InteractiveTurnRequest,
  type MedousaContext,
  type SessionHistoryResponse,
  type SessionSummary,
} from "@medousa/client";
import { chatHtml, createNonce } from "./chatHtml.js";
import { buildCodeIntentContext } from "./coderContext.js";
import {
  createProjectionState,
  projectStreamEvent,
  type ProjectedEvent,
} from "./streamProjection.js";

const TOKEN_KEY = "medousa.bearerToken";
const SESSION_KEY = "medousa.sessionId";

function agentModeLabel(mode: AgentModeId): string {
  if (mode === "coder") return "Coder";
  if (mode === "instant") return "Instant";
  return "General";
}

function agentModeQuickPick(mode: AgentModeId): { label: string; detail: string } {
  if (mode === "coder") {
    return {
      label: "$(code) Coder",
      detail: "Repository-aware engineering in a governed Forge worktree",
    };
  }
  if (mode === "instant") {
    return {
      label: "$(zap) Instant",
      detail: "Faster chat with focused recent context",
    };
  }
  return {
    label: "$(sparkle) General",
    detail: "Life, planning, research, and everyday work",
  };
}

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
    vscode.commands.registerCommand("medousa.selectMode", () => chat.selectMode()),
    vscode.commands.registerCommand("medousa.bindUndertaking", () => chat.selectUndertaking()),
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
  private readonly workshopWatchers = new Map<string, AbortController>();
  private pendingWorkshopRefreshSession: string | null = null;
  private disabledContext = new Set<string>();
  private lastPrompt: string | null = null;
  private modes: AgentModeAvailability[] = [];
  private activeMode: AgentModeId = "general";
  private undertakings: ForgeUndertaking[] = [];
  private boundWorkId: string | null = null;
  private boundUndertaking: ForgeUndertaking | null = null;
  private modePoll: ReturnType<typeof setInterval> | null = null;
  private runtimeRefreshInFlight = false;
  private lastProposalId: string | null = null;
  private pendingProposal: AgentModeProposalResponse | null = null;
  private nextCodeProjectSetupAuthorized = false;

  constructor(private readonly context: vscode.ExtensionContext) {}

  resolveWebviewView(view: vscode.WebviewView): void {
    this.view = view;
    const distRoot = vscode.Uri.joinPath(this.context.extensionUri, "dist");
    view.webview.options = { enableScripts: true, localResourceRoots: [distRoot] };
    const liquidScriptUri = view.webview.asWebviewUri(
      vscode.Uri.joinPath(distRoot, "liquidWebview.js"),
    ).toString();
    view.webview.html = chatHtml(createNonce(), {
      liquidScriptUri,
      cspSource: view.webview.cspSource,
    });
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
    for (const watcher of this.workshopWatchers.values()) watcher.abort();
    this.workshopWatchers.clear();
    if (this.modePoll) clearInterval(this.modePoll);
  }

  async refresh(): Promise<void> {
    if (!this.view) return;
    if (this.modePoll) {
      clearInterval(this.modePoll);
      this.modePoll = null;
    }
    this.modes = [];
    this.undertakings = [];
    this.boundWorkId = null;
    this.boundUndertaking = null;
    this.activeMode = "general";
    this.lastProposalId = null;
    this.pendingProposal = null;
    this.post({ type: "connection", state: "checking", label: "Checking workshop…" });
    try {
      this.client = await createClient(this.context);
      const restored = await this.restoreOrCreateSession(this.client);
      this.sessionId = restored.session_id;
      this.postHistory(restored);
      await this.refreshSessions();
      this.post({ type: "connection", state: "connected", label: endpointLabel() });
      this.refreshContext();
      await this.refreshRuntimeState();
      this.startRuntimePolling();
    } catch (error) {
      this.client = null;
      this.sessionId = null;
      this.boundWorkId = null;
      this.boundUndertaking = null;
      this.post({ type: "connection", state: connectionState(error), label: friendlyConnectionError(error) });
    }
  }

  private startRuntimePolling(): void {
    if (this.modePoll) clearInterval(this.modePoll);
    this.modePoll = setInterval(() => {
      if (!this.abortController) void this.refreshRuntimeState();
    }, 2_000);
  }

  private async refreshRuntimeState(): Promise<void> {
    if (this.runtimeRefreshInFlight) return;
    const client = this.client;
    const sessionId = this.sessionId;
    if (!client || !sessionId) return;
    this.runtimeRefreshInFlight = true;
    try {
      const [mode, binding, proposals, registry] = await Promise.all([
        client.sessionAgentMode(sessionId),
        client.sessionCodeBinding(sessionId),
        client.agentModeProposals(sessionId),
        this.modes.length ? Promise.resolve(null) : client.agentModes(),
      ]);
      if (client !== this.client || sessionId !== this.sessionId) return;
      let undertaking: ForgeUndertaking | null = null;
      if (binding.work_id) {
        try {
          undertaking = await client.forgeUndertaking(binding.work_id);
        } catch {
          // Preserve the binding in the UI, but require a valid ready item for Coder.
        }
      }
      if (client !== this.client || sessionId !== this.sessionId) return;
      if (registry) this.modes = registry.modes;
      this.activeMode = mode.effective_mode;
      this.boundWorkId = binding.work_id ?? null;
      this.boundUndertaking = undertaking;
      this.postRuntimeState();
      const pending = proposals.proposals.find((proposal) => proposal.status === "pending") ?? null;
      this.postProposal(pending);
    } catch {
      // Conversation and stream connectivity own the primary error state.
    } finally {
      this.runtimeRefreshInFlight = false;
    }
  }

  private postRuntimeState(): void {
    this.post({
      type: "runtimeState",
      mode: this.activeMode,
      modeLabel: this.modes.find((mode) => mode.mode === this.activeMode)?.label
        ?? agentModeLabel(this.activeMode),
      workId: this.boundWorkId,
      workTitle: this.boundUndertaking?.title ?? this.boundWorkId,
      coderReady: ["ready", "executing"].includes(this.boundUndertaking?.state.toLowerCase() ?? "")
        && Boolean(this.boundUndertaking?.environment?.worktree),
    });
  }

  private postProposal(proposal: AgentModeProposalResponse | null): void {
    if (!proposal) {
      if (this.lastProposalId) this.post({ type: "proposalClear" });
      this.lastProposalId = null;
      this.pendingProposal = null;
      return;
    }
    this.pendingProposal = proposal;
    if (proposal.proposal_id === this.lastProposalId) return;
    this.lastProposalId = proposal.proposal_id;
    this.post({
      type: "modeProposal",
      proposalId: proposal.proposal_id,
      toMode: proposal.to_mode,
      reason: proposal.reason,
      expiresAt: proposal.expires_at_utc,
    });
  }

  refreshContext(): void {
    if (!this.view) return;
    const editor = vscode.window.activeTextEditor;
    this.post({
      type: "context",
      chips: contextChips(editor, this.disabledContext),
      suggestions: contextSuggestions(editor, this.disabledContext),
      canReset: this.disabledContext.size > 0,
    });
  }

  async selectMode(): Promise<void> {
    if (!this.client || !this.sessionId) await this.refresh();
    const client = this.client;
    const sessionId = this.sessionId;
    if (!client || !sessionId) return;
    if (!this.modes.length) await this.refreshRuntimeState();
    const picked = await vscode.window.showQuickPick(
      this.modes.map((mode) => {
        const copy = agentModeQuickPick(mode.mode);
        return {
          label: copy.label,
          description: mode.mode === this.activeMode ? "Active" : undefined,
          detail: mode.available ? copy.detail : mode.unavailable_reason ?? "Unavailable",
          mode,
        };
      }),
      { placeHolder: "How should Medousa work in this conversation?" },
    );
    if (!picked || !picked.mode.available) return;
    await client.setSessionAgentMode(sessionId, picked.mode.mode);
    await this.refreshRuntimeState();
  }

  async selectUndertaking(allowAgentSetup = false): Promise<boolean> {
    if (!this.client || !this.sessionId) await this.refresh();
    const client = this.client;
    const sessionId = this.sessionId;
    if (!client || !sessionId) return false;
    this.undertakings = await client.forgeUndertakings();
    const ready = this.undertakings.filter(
      (item) => ["ready", "executing"].includes(item.state.toLowerCase()) && Boolean(item.environment?.worktree),
    );
    const choices: Array<{
      label: string;
      description: string | undefined;
      detail: string;
      action: "bind" | "create" | "agent" | "detach";
      item?: ForgeUndertaking;
    }> = [
      ...ready.map((item) => ({
        label: `$(repo) ${item.title}`,
        description: item.id === this.boundUndertaking?.id ? "Bound" : item.human_phase,
        detail: item.brief,
        action: "bind" as const,
        item,
      })),
      {
        label: "$(new-folder) Create a new project",
        description: "New governed codebase",
        detail: "Medousa initializes Git, provisions a Forge worktree, and binds this conversation.",
        action: "create",
      },
      ...(allowAgentSetup ? [{
        label: "$(sparkle) Let Medousa choose or create it",
        description: "Use this message as the project brief",
        detail: "Coder setup can list, bind, or create a project before full coding begins.",
        action: "agent" as const,
      }] : []),
      ...(this.boundUndertaking ? [{
        label: "$(close) Stop following this undertaking",
        description: "Return this conversation to an unbound state",
        detail: "Coder stays active and returns to project setup.",
        action: "detach" as const,
      }] : []),
    ];
    const picked = await vscode.window.showQuickPick(choices, {
      placeHolder: "Choose the governed project for this conversation",
    });
    if (!picked) return false;
    if (picked.action === "create") return this.createProject();
    if (picked.action === "agent") {
      this.nextCodeProjectSetupAuthorized = true;
      return true;
    }
    if (picked.action === "detach") {
      await client.clearSessionCodeBinding(sessionId);
      this.boundWorkId = null;
      this.boundUndertaking = null;
      await this.refreshRuntimeState();
      return false;
    }
    if (!picked.item) return false;
    await client.setSessionCodeBinding(sessionId, picked.item.id);
    this.boundWorkId = picked.item.id;
    this.boundUndertaking = picked.item;
    this.postRuntimeState();
    await this.offerOpenWorktree(picked.item);
    return true;
  }

  private async createProject(): Promise<boolean> {
    const client = this.client;
    const sessionId = this.sessionId;
    if (!client || !sessionId) return false;
    const title = await vscode.window.showInputBox({
      title: "Create a Medousa project",
      prompt: "Project name",
      placeHolder: "Personal finance dashboard",
      validateInput: (value) => value.trim() ? undefined : "Enter a project name",
    });
    if (!title?.trim()) return false;
    const brief = await vscode.window.showInputBox({
      title: `Create “${title.trim()}”`,
      prompt: "What should Medousa build?",
      placeHolder: "Describe the outcome (optional)",
    });
    if (brief === undefined) return false;
    const created = await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: `Creating ${title.trim()}…`,
        cancellable: false,
      },
      () => client.startSessionCodeProject(sessionId, {
        title: title.trim(),
        brief: brief.trim() || title.trim(),
        source: "blank",
      }),
    );
    this.boundWorkId = created.work_id;
    this.boundUndertaking = await client.forgeUndertaking(created.work_id);
    this.postRuntimeState();
    await this.offerOpenWorktree(this.boundUndertaking);
    return true;
  }

  private async ensureCoderBinding(): Promise<boolean> {
    if (
      ["ready", "executing"].includes(this.boundUndertaking?.state.toLowerCase() ?? "")
      && this.boundUndertaking?.environment?.worktree
    ) return true;
    return this.selectUndertaking(true);
  }

  private async offerOpenWorktree(undertaking: ForgeUndertaking): Promise<void> {
    const worktree = undertaking.environment?.worktree;
    if (!worktree || !isLocalEndpoint()) return;
    const folders = vscode.workspace.workspaceFolders ?? [];
    if (folders.some((folder) => path.resolve(folder.uri.fsPath) === path.resolve(worktree))) return;
    const action = await vscode.window.showInformationMessage(
      `“${undertaking.title}” is bound. Open its governed worktree so Coder edits appear directly in VS Code?`,
      "Open Worktree",
    );
    if (action === "Open Worktree") {
      await vscode.commands.executeCommand("vscode.openFolder", vscode.Uri.file(worktree), false);
    }
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

    if (this.activeMode === "coder" && !(await this.ensureCoderBinding())) {
      this.post({ type: "error", text: "Coder needs a ready Forge undertaking. Bind one and try again." });
      this.post({ type: "busy", value: false });
      return;
    }
    const codeProjectSetupAuthorized = this.nextCodeProjectSetupAuthorized;
    this.nextCodeProjectSetupAuthorized = false;

    this.abortController = new AbortController();
    try {
      const defaults = await client.runtimeDefaults({ signal: this.abortController.signal });
      const editorContext = currentContext(vscode.window.activeTextEditor, this.disabledContext);
      const codeContext = this.boundUndertaking
        ? buildCodeIntentContext(
            editorContext,
            this.boundUndertaking,
            vscode.window.visibleTextEditors.map((editor) => editor.document.uri.fsPath),
          )
        : undefined;
      const request: InteractiveTurnRequest = {
        model: defaults.model,
        persist_user_turn: true,
        prompt,
        host_context: hostContext(editorContext),
        code_context: codeContext,
        code_project_setup_authorized: codeProjectSetupAuthorized,
        provider: defaults.provider,
        response_depth_mode: defaults.response_depth_mode,
        reasoning_effort: defaults.reasoning_effort,
        session_id: sessionId,
        stage_routing: defaults.stage_routing as unknown as InteractiveTurnRequest["stage_routing"],
        media_refs: [],
        surface: {
          channel_surface: "vscode",
          supports_browser_host: false,
          supports_liquid_markdown: true,
          supports_ui_artifacts: false,
        },
      };

      this.post({ type: "status", text: "Medousa is thinking…", working: true });
      const turn = await client.startTurn(request, { signal: this.abortController.signal });
      const projection = createProjectionState(showEngineDetails());
      let handedOff = false;
      for await (const event of client.streamTurnV2(turn, {
        signal: this.abortController.signal,
        stopOnHandoff: true,
      })) {
        for (const projected of projectStreamEvent(event, projection)) {
          this.postProjected(projected);
          if (projected.kind === "handoff") handedOff = true;
        }
        if (handedOff) {
          void this.followWorkshop(turn, sessionId);
          break;
        }
      }
      this.post({ type: "done" });
      if (handedOff) {
        this.post({
          type: "status",
          text: "Workshop is running · you can keep typing",
          working: false,
        });
      }
      await this.refreshSessions();
      await this.refreshRuntimeState();
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
      if (this.pendingWorkshopRefreshSession) {
        const refreshSession = this.pendingWorkshopRefreshSession;
        this.pendingWorkshopRefreshSession = null;
        void this.refreshHistoryWhenIdle(refreshSession);
      }
    }
  }

  /** The host stream ends at handoff; follow the durable workshop result separately. */
  private async followWorkshop(
    response: Awaited<ReturnType<MedousaClient["startTurn"]>>,
    sessionId: string,
  ): Promise<void> {
    if (this.workshopWatchers.has(response.turn_id)) return;
    const watcher = new AbortController();
    this.workshopWatchers.set(response.turn_id, watcher);
    const client = this.client;
    if (!client) {
      this.workshopWatchers.delete(response.turn_id);
      return;
    }

    try {
      for await (const event of client.streamTurnV2(response, {
        signal: watcher.signal,
        maxReconnectAttempts: 8,
      })) {
        if (isBackgroundHandoffEvent(event)) continue;
        if (isTurnStreamTerminal(event)) {
          await this.pollWorkshopHistory(sessionId, watcher.signal, 8);
          if (!this.abortController && this.sessionId === sessionId) {
            this.post({ type: "status", text: "Connected", working: false });
          }
          return;
        }
      }
    } catch {
      if (!watcher.signal.aborted) await this.pollWorkshopHistory(sessionId, watcher.signal, 30);
    } finally {
      this.workshopWatchers.delete(response.turn_id);
    }
  }

  private async pollWorkshopHistory(
    sessionId: string,
    signal: AbortSignal,
    attempts: number,
  ): Promise<void> {
    const client = this.client;
    if (!client) return;
    let previousSignature = "";
    for (let attempt = 0; attempt < attempts && !signal.aborted; attempt += 1) {
      try {
        const history = await client.sessionHistory(sessionId, { signal });
        const signature = historySignature(history);
        if (signature !== previousSignature) {
          previousSignature = signature;
          this.deliverWorkshopHistory(history);
        }
      } catch {
        if (signal.aborted) return;
      }
      if (attempt + 1 < attempts) {
        try {
          await delay(500, signal);
        } catch {
          return;
        }
      }
    }
  }

  private deliverWorkshopHistory(history: SessionHistoryResponse): void {
    if (history.session_id !== this.sessionId) return;
    if (this.abortController) {
      this.pendingWorkshopRefreshSession = history.session_id;
      return;
    }
    this.postHistory(history);
  }

  private async refreshHistoryWhenIdle(sessionId: string): Promise<void> {
    if (this.sessionId !== sessionId || !this.client) return;
    try {
      const history = await this.client.sessionHistory(sessionId);
      this.deliverWorkshopHistory(history);
    } catch {
      // The next workshop stream event or a normal refresh can reconcile history.
    }
  }

  private postHistory(history: SessionHistoryResponse): void {
    this.post({
      type: "history",
      sessionId: history.session_id,
      turns: history.turns.map((turn) => ({
        ...turn,
        content: turn.content,
      })),
    });
  }

  async newSession(): Promise<void> {
    if (this.abortController) {
      const answer = await vscode.window.showWarningMessage(
        "Start a new conversation? The response in progress will be cancelled.",
        { modal: true },
        "Start new conversation",
      );
      if (answer !== "Start new conversation") return;
    }
    await this.cancel();
    try {
      if (!this.client) this.client = await createClient(this.context);
      const created = await this.client.createSession({
        catalog: "single",
      });
      this.sessionId = created.session_id;
      await this.context.workspaceState.update(SESSION_KEY, created.session_id);
      this.disabledContext.clear();
      this.post({ type: "reset", sessionId: created.session_id });
      await this.refreshSessions();
      this.refreshContext();
      await this.refreshRuntimeState();
    } catch (error) {
      this.post({ type: "error", text: errorMessage(error) });
    }
  }

  private async refreshSessions(): Promise<void> {
    if (!this.client) return;
    this.post({ type: "sessionsLoading" });
    const sessions = await this.client.sessions(100);
    this.post({
      type: "sessions",
      activeSessionId: this.sessionId,
      sessions: sessions.map(normalizeSessionSummary).filter((session): session is ChatSessionSummary => session !== null),
    });
  }

  private async switchSession(sessionId: string): Promise<void> {
    if (this.abortController || !this.client || sessionId === this.sessionId) return;
    const history = await this.client.sessionHistory(sessionId);
    this.sessionId = sessionId;
    this.lastPrompt = lastUserPrompt(history);
    await this.context.workspaceState.update(SESSION_KEY, sessionId);
    this.post({
      type: "history",
      sessionId,
      turns: history.turns,
    });
    await this.refreshSessions();
    await this.refreshRuntimeState();
  }

  private async renameSession(sessionId: string, displayName: string): Promise<void> {
    if (!this.client) return;
    const trimmed = displayName.trim();
    if (!trimmed) throw new Error("Conversation name must not be empty");
    await this.client.renameSession(sessionId, trimmed);
    await this.refreshSessions();
  }

  private async deleteSession(sessionId: string, displayName?: string): Promise<void> {
    if (!this.client) return;
    const answer = await vscode.window.showWarningMessage(
      `Delete “${displayName?.trim() || "this conversation"}” and its Medousa memory? This cannot be undone.`,
      { modal: true },
      "Delete",
    );
    if (answer !== "Delete") return;
    if (sessionId === this.sessionId) await this.cancel();
    await this.client.deleteSession(sessionId, true);
    if (sessionId === this.sessionId) await this.newSession();
    else await this.refreshSessions();
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
    const created = await client.createSession({ catalog: "single" });
    await this.context.workspaceState.update(SESSION_KEY, created.session_id);
    return { authority_id: created.authority_id, session_id: created.session_id, turns: [] };
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
        this.post({ type: "toast", text: "Response stopped" });
        break;
      case "configure":
        await configureConnection(this.context);
        await this.refresh();
        break;
      case "newSession":
        await this.newSession();
        break;
      case "openSessions":
        this.post({ type: "sessionsOpen" });
        try {
          await this.refreshSessions();
        } catch (error) {
          this.post({ type: "sessionsError", text: errorMessage(error) });
        }
        break;
      case "switchSession":
        if (message.sessionId) {
          try { await this.switchSession(message.sessionId); }
          catch (error) { this.post({ type: "sessionsError", text: errorMessage(error) }); }
        }
        break;
      case "renameSession":
        if (message.sessionId && message.text) {
          try { await this.renameSession(message.sessionId, message.text); }
          catch (error) { this.post({ type: "sessionsError", text: errorMessage(error) }); }
        }
        break;
      case "deleteSession":
        if (message.sessionId) {
          try { await this.deleteSession(message.sessionId, message.text); }
          catch (error) { this.post({ type: "sessionsError", text: errorMessage(error) }); }
        }
        break;
      case "copyText":
        if (message.text) {
          await vscode.env.clipboard.writeText(message.text);
          this.post({ type: "toast", text: "Copied" });
        }
        break;
      case "shareText":
        if (message.text) {
          await vscode.env.clipboard.writeText(message.text);
          void vscode.window.showInformationMessage("Medousa reply copied—ready to share.");
          this.post({ type: "toast", text: "Copied for sharing" });
        }
        break;
      case "saveToLibrary":
        if (this.client && this.sessionId && message.text) {
          try {
            const saved = await saveReplyToLibrary(this.client, this.sessionId, message.text, message.userText);
            this.post({ type: "toast", text: `Saved to Library · ${saved}` });
          } catch (error) {
            this.post({ type: "error", text: `Could not save to Library: ${errorMessage(error)}` });
          }
        }
        break;
      case "retry":
        if (this.lastPrompt) await this.sendPrompt(this.lastPrompt);
        break;
      case "openHome":
        await vscode.env.openExternal(vscode.Uri.parse("medousa://chat"));
        break;
      case "selectMode":
        await this.selectMode();
        break;
      case "selectUndertaking":
        await this.selectUndertaking();
        break;
      case "modeProposal":
        if (this.client && this.sessionId && message.requestId) {
          const proposal = this.pendingProposal?.proposal_id === message.requestId
            ? this.pendingProposal
            : null;
          if (!proposal) break;
          await this.client.decideAgentModeProposal(
            this.sessionId,
            message.requestId,
            Boolean(message.approve),
          );
          this.lastProposalId = null;
          this.pendingProposal = null;
          this.post({ type: "proposalClear" });
          await this.refreshRuntimeState();
        }
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
      case "handoff":
        this.post({
          type: "status",
          text: `${event.text} · you can keep typing`,
          working: false,
        });
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
    cursor: disabled.has("file") ? undefined : {
      line: selection.active.line,
      character: selection.active.character,
    },
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

function contextSuggestions(editor: vscode.TextEditor | undefined, disabled: Set<string>): string[] {
  const suggestions: string[] = [];
  if (editor && !editor.selection.isEmpty && !disabled.has("selection")) {
    suggestions.push("Explain this selection", "Find risks in this selection");
  } else if (editor && vscode.languages.getDiagnostics(editor.document.uri).length > 0 && !disabled.has("diagnostics")) {
    suggestions.push("Help me fix these diagnostics");
  }
  if (editor && !disabled.has("file")) suggestions.push("Explain the active file");
  suggestions.push("What should I work on next?");
  return suggestions.slice(0, 3);
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

function isLocalEndpoint(): boolean {
  const endpoint = vscode.workspace.getConfiguration("medousa").get<string>("endpoint", "http://127.0.0.1:7419");
  try {
    const host = new URL(endpoint).hostname.toLowerCase();
    return host === "127.0.0.1" || host === "localhost" || host === "::1";
  } catch {
    return false;
  }
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

function historySignature(history: SessionHistoryResponse): string {
  return history.turns
    .map((turn) => `${turn.role}\u0000${turn.timestamp}\u0000${turn.content}`)
    .join("\u0001");
}

function delay(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) {
      reject(signal.reason ?? new Error("Aborted"));
      return;
    }
    const timer = setTimeout(resolve, ms);
    signal.addEventListener("abort", () => {
      clearTimeout(timer);
      reject(signal.reason ?? new Error("Aborted"));
    }, { once: true });
  });
}

type ContextChip = { key: string; label: string; detail?: string };
type ConnectionState = "checking" | "connected" | "reconnecting" | "unavailable" | "unauthorized";
type ChatSessionSummary = { sessionId: string; displayName: string; preview: string; turns: number; lastTimestamp: string | null };

type OutboundMessage =
  | { type: "user"; text: string }
  | { type: "assistantDelta"; text: string }
  | { type: "assistantReplace"; text: string }
  | { type: "status"; text: string; working: boolean }
  | { type: "toolStarted"; runId: string; name: string; summary?: string }
  | { type: "toolFinished"; runId: string; name: string; status: string; summary?: string }
  | { type: "attention"; kind: "budget" | "permission"; requestId: string; text: string; rounds?: number }
  | { type: "runtimeState"; mode: AgentModeId; modeLabel: string; workId: string | null; workTitle: string | null; coderReady: boolean }
  | { type: "modeProposal"; proposalId: string; toMode: AgentModeId; reason: string; expiresAt: string }
  | { type: "proposalClear" }
  | { type: "error"; text: string }
  | { type: "done" }
  | { type: "busy"; value: boolean }
  | { type: "history"; sessionId: string; turns: SessionHistoryResponse["turns"] }
  | { type: "sessions"; sessions: ChatSessionSummary[]; activeSessionId: string | null }
  | { type: "sessionsOpen" }
  | { type: "sessionsLoading" }
  | { type: "sessionsError"; text: string }
  | { type: "toast"; text: string }
  | { type: "connection"; state: ConnectionState; label: string }
  | { type: "context"; chips: ContextChip[]; suggestions: string[]; canReset: boolean }
  | { type: "reset"; sessionId: string };

type InboundMessage = {
  type: "ready" | "send" | "cancel" | "configure" | "newSession" | "openSessions" | "switchSession" | "renameSession" | "deleteSession" | "copyText" | "shareText" | "saveToLibrary" | "retry" | "openHome" | "selectMode" | "selectUndertaking" | "modeProposal" | "removeContext" | "resetContext" | "insertCode" | "openLink" | "budget" | "permission";
  text?: string;
  userText?: string;
  sessionId?: string;
  key?: string;
  href?: string;
  requestId?: string;
  approve?: boolean;
  rounds?: number;
  toMode?: AgentModeId;
};

function isInboundMessage(value: unknown): value is InboundMessage {
  return Boolean(value && typeof value === "object" && "type" in value && [
    "ready", "send", "cancel", "configure", "newSession", "openSessions", "switchSession", "renameSession", "deleteSession", "copyText", "shareText", "saveToLibrary", "retry", "openHome", "selectMode", "selectUndertaking", "modeProposal", "removeContext", "resetContext", "insertCode", "openLink", "budget", "permission",
  ].includes(String(value.type)));
}

function normalizeSessionSummary(session: SessionSummary): ChatSessionSummary | null {
  const sessionId = String(session.session_id ?? session.id ?? "").trim();
  if (!sessionId) return null;
  const preview = typeof session.preview === "string" ? session.preview.trim() : "";
  const displayName = session.display_name?.trim() || firstLine(preview) || "New conversation";
  return {
    sessionId,
    displayName,
    preview,
    turns: typeof session.turns === "number" ? session.turns : 0,
    lastTimestamp: typeof session.last_timestamp === "string" ? session.last_timestamp : null,
  };
}

function firstLine(value: string): string {
  const line = value.split("\n")[0]?.trim() ?? "";
  return line.length > 52 ? `${line.slice(0, 51)}…` : line;
}

function lastUserPrompt(history: SessionHistoryResponse): string | null {
  for (let index = history.turns.length - 1; index >= 0; index -= 1) {
    const turn = history.turns[index];
    if (turn?.role === "user") return turn.content;
  }
  return null;
}

async function saveReplyToLibrary(client: MedousaClient, sessionId: string, reply: string, userPrompt?: string): Promise<string> {
  const title = chatReplyTitle(reply);
  const prompt = userPrompt?.trim();
  const quotedPrompt = prompt ? `> **You**\n${prompt.split("\n").map((line) => `> ${line}`).join("\n")}\n\n` : "";
  const content = `---\nkind: inbox\ntags: [chat-turn]\n---\n\n# ${title}\n\n${quotedPrompt}${reply.trim()}\n`;
  const slug = title.toLowerCase().normalize("NFKD").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 64) || "chat-turn";
  try {
    const response = await client.createVaultNote({ path: `inbox/${slug}.md`, content, session_id: sessionId, semantic_tags: ["chat-turn"] });
    return response.note.path;
  } catch (error) {
    if (!(error instanceof MedousaHttpError)) throw error;
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    const response = await client.createVaultNote({ path: `inbox/${slug}-${stamp}.md`, content, session_id: sessionId, semantic_tags: ["chat-turn"] });
    return response.note.path;
  }
}

function chatReplyTitle(markdown: string): string {
  const heading = markdown.match(/^#{1,6}\s+(.+)$/m)?.[1]?.trim();
  const first = heading ?? markdown.split("\n").map((line) => line.trim()).find((line) => line && !line.startsWith("```")) ?? "Chat turn";
  const clean = first.replace(/^[*_]+|[*_]+$/g, "").trim();
  return clean.length > 72 ? `${clean.slice(0, 71)}…` : clean;
}
