import {
  App,
  Editor,
  ItemView,
  MarkdownFileInfo,
  MarkdownView,
  Modal,
  Notice,
  Plugin,
  PluginSettingTab,
  Setting,
  WorkspaceLeaf,
} from "obsidian";
import {
  MedousaClient,
  MedousaHttpError,
  type InteractiveTurnRequest,
  type SessionHistoryResponse,
  type SessionSummary,
  type VaultBacklinksResponse,
  type VaultNoteContentResponse,
  type VaultSearchResponse,
} from "@medousa/client";
import { captureObsidianContext, stripContextSupplement, type ObsidianContextSnapshot } from "./context";
import {
  createProjectionState,
  projectStreamEvent,
  type ProjectedEvent,
} from "./streamProjection";

export const VIEW_TYPE_MEDOUSA = "medousa-chat";

interface MedousaSettings {
  endpoint: string;
}

interface PersistedData extends Partial<MedousaSettings> {
  activeSessionId?: string;
  activeSessionName?: string | null;
}

const DEFAULT_SETTINGS: MedousaSettings = {
  endpoint: "http://127.0.0.1:7419",
};

type AttentionEvent = Extract<ProjectedEvent, { kind: "budget_request" | "permission_request" }>;

interface TurnCallbacks {
  onProjected: (event: ProjectedEvent) => void;
  onAttention: (event: AttentionEvent) => Promise<void>;
}

export default class MedousaPlugin extends Plugin {
  settings!: MedousaSettings;
  private client: MedousaClient | null = null;
  private bearerToken: string | undefined;
  private activeSessionId: string | null = null;
  private activeSessionName: string | null = null;
  private activeAbort: AbortController | null = null;
  private lastContext: ObsidianContextSnapshot | null = null;

  async onload(): Promise<void> {
    const data = ((await this.loadData()) as PersistedData | null | undefined) ?? {};
    this.settings = {
      endpoint: data.endpoint?.trim() || DEFAULT_SETTINGS.endpoint,
    };
    this.activeSessionId = data.activeSessionId ?? null;
    this.activeSessionName = data.activeSessionName ?? null;

    this.registerView(VIEW_TYPE_MEDOUSA, (leaf) => new MedousaChatView(leaf, this));
    this.addRibbonIcon("message-circle", "Open Medousa chat", () => void this.openChat());
    this.addCommand({
      id: "open-chat",
      name: "Open chat",
      callback: () => void this.openChat(),
    });
    this.addCommand({
      id: "ask-current-note",
      name: "Ask about current note",
      callback: () => void this.askCurrentNote(),
    });
    this.addCommand({
      id: "ask-selection",
      name: "Ask about selection",
      editorCallback: (_editor: Editor, _ctx: MarkdownView | MarkdownFileInfo) => void this.askSelection(),
    });
    this.addCommand({
      id: "new-conversation",
      name: "New conversation",
      callback: () => void this.newConversation(),
    });
    this.addCommand({
      id: "configure-connection",
      name: "Configure connection",
      callback: () => new ConnectionModal(this.app, this).open(),
    });
    this.addCommand({
      id: "open-home",
      name: "Open in Medousa Home",
      callback: () => this.openHome(),
    });
    this.addCommand({
      id: "search-vault",
      name: "Search Medousa vault",
      callback: () => new VaultSearchModal(this.app, this).open(),
    });
    this.addCommand({
      id: "show-backlinks",
      name: "Show backlinks for current note",
      callback: () => void this.openBacklinks(),
    });
    this.addCommand({
      id: "save-last-answer",
      name: "Save last answer as note",
      callback: () => void this.openAnswerAction("create"),
    });
    this.addCommand({
      id: "append-last-answer",
      name: "Append last answer to current note",
      callback: () => void this.openAnswerAction("append"),
    });
    this.addCommand({
      id: "daily-synthesis",
      name: "Generate daily synthesis",
      callback: () => void this.generateSynthesis("daily"),
    });
    this.addCommand({
      id: "weekly-synthesis",
      name: "Generate weekly synthesis",
      callback: () => void this.generateSynthesis("weekly"),
    });
    this.addSettingTab(new MedousaSettingTab(this.app, this));
    this.registerEvent(
      this.app.workspace.on("active-leaf-change", (leaf) => {
        if (leaf?.view instanceof MarkdownView) void this.rememberCurrentContext(leaf.view);
        void this.chatView()?.refreshContext();
      }),
    );
  }

  onunload(): void {
    this.activeAbort?.abort();
  }

  async openChat(prompt?: string, snapshot?: ObsidianContextSnapshot): Promise<MedousaChatView | null> {
    const context = snapshot ?? (await this.currentContext());
    const existing = this.app.workspace.getLeavesOfType(VIEW_TYPE_MEDOUSA)[0];
    const leaf = existing ?? this.app.workspace.getRightLeaf(false);
    if (!leaf) {
      new Notice("Medousa could not open a workspace pane.");
      return null;
    }

    await leaf.setViewState({ type: VIEW_TYPE_MEDOUSA, active: true });
    await this.app.workspace.revealLeaf(leaf);
    const view = leaf.view as MedousaChatView;
    await view.ready;
    view.setContext(context);
    if (prompt) await view.sendPrompt(prompt, context);
    return view;
  }

  async currentContext(): Promise<ObsidianContextSnapshot> {
    const activeView = this.app.workspace.getActiveViewOfType(MarkdownView);
    const captured = await captureObsidianContext(this.app, activeView);
    if (captured.file) this.lastContext = captured;
    return captured.file || !this.lastContext ? captured : this.lastContext;
  }

  async ensureSession(): Promise<SessionHistoryResponse> {
    const client = this.getClient();
    if (this.activeSessionId) {
      try {
        return await client.sessionHistory(this.activeSessionId);
      } catch (error) {
        if (!(error instanceof MedousaHttpError) || error.status !== 404) throw error;
        this.activeSessionId = null;
      }
    }

    const created = await client.createSession({ catalog: "single" });
    this.activeSessionId = created.session_id;
    this.activeSessionName = created.display_name ?? null;
    await this.saveState();
    return { session_id: created.session_id, turns: [] };
  }

  async newConversation(): Promise<void> {
    if (this.activeAbort) {
      new Notice("Stop the current response before starting a new conversation.");
      return;
    }
    const created = await this.getClient().createSession({ catalog: "single" });
    this.activeSessionId = created.session_id;
    this.activeSessionName = created.display_name ?? null;
    await this.saveState();
    this.chatView()?.showHistory({ session_id: created.session_id, turns: [] });
  }

  async listSessions(): Promise<SessionSummary[]> {
    return this.getClient().sessions(100);
  }

  async switchSession(sessionId: string): Promise<SessionHistoryResponse> {
    if (this.activeAbort) throw new Error("Stop the current response before switching conversations.");
    const history = await this.getClient().sessionHistory(sessionId);
    this.activeSessionId = sessionId;
    this.activeSessionName = null;
    for (const session of await this.listSessions()) {
      if (sessionIdOf(session) === sessionId) {
        this.activeSessionName = session.display_name ?? null;
        break;
      }
    }
    await this.saveState();
    return history;
  }

  async renameSession(sessionId: string, displayName: string): Promise<void> {
    const trimmed = displayName.trim();
    if (!trimmed) throw new Error("Conversation name must not be empty.");
    await this.getClient().renameSession(sessionId, trimmed);
    if (sessionId === this.activeSessionId) this.activeSessionName = trimmed;
    await this.saveState();
  }

  async deleteSession(sessionId: string): Promise<SessionHistoryResponse | null> {
    if (this.activeAbort) throw new Error("Stop the current response before deleting a conversation.");
    await this.getClient().deleteSession(sessionId, true);
    if (sessionId !== this.activeSessionId) return null;
    const created = await this.getClient().createSession({ catalog: "single" });
    this.activeSessionId = created.session_id;
    this.activeSessionName = created.display_name ?? null;
    await this.saveState();
    return { session_id: created.session_id, turns: [] };
  }

  sessionTitle(): string {
    return this.activeSessionName?.trim() || "Medousa";
  }

  sessionId(): string | null {
    return this.activeSessionId;
  }

  openHome(): void {
    window.open("medousa://chat", "_blank");
    new Notice("Opening Medousa Home…");
  }

  async searchVault(query: string, limit = 20): Promise<VaultSearchResponse> {
    return this.getClient().searchVault(query, limit);
  }

  async vaultBacklinks(path: string): Promise<VaultBacklinksResponse> {
    return this.getClient().vaultBacklinks(path);
  }

  async getVaultNote(path: string): Promise<VaultNoteContentResponse> {
    return this.getClient().getVaultNote(path);
  }

  async createVaultNote(request: Parameters<MedousaClient["createVaultNote"]>[0]): Promise<void> {
    await this.getClient().createVaultNote(request);
  }

  async updateVaultNote(path: string, content: string, ifMatch?: string): Promise<void> {
    await this.getClient().updateVaultNote(path, content, ifMatch);
  }

  openAnswerAction(mode: "create" | "append"): void {
    const view = this.chatView();
    if (!view) {
      void this.openChat().then((opened) => opened?.openAnswerAction(mode));
      return;
    }
    view.openAnswerAction(mode);
  }

  async openBacklinks(): Promise<void> {
    const snapshot = await this.currentContext();
    if (!snapshot.file) {
      new Notice("Open a note before viewing backlinks.");
      return;
    }
    new BacklinksModal(this.app, this, snapshot.file.path).open();
  }

  insertLinkIntoCurrentNote(path: string): void {
    const target = path.replace(/\.md$/i, "");
    const preferredPath = this.lastContext?.file?.path;
    const views = this.app.workspace.getLeavesOfType("markdown");
    const view = views
      .map((leaf) => leaf.view)
      .filter((candidate): candidate is MarkdownView => candidate instanceof MarkdownView)
      .find((candidate) => !preferredPath || candidate.file?.path === preferredPath)
      ?? this.app.workspace.getActiveViewOfType(MarkdownView);
    if (!view) {
      new Notice("Open a Markdown note before inserting a link.");
      return;
    }
    view.editor.replaceSelection(`[[${target}]]`);
    new Notice(`Inserted link to ${target}.`);
  }

  async sendTurn(
    prompt: string,
    snapshot: ObsidianContextSnapshot,
    callbacks: TurnCallbacks,
  ): Promise<void> {
    if (this.activeAbort) throw new Error("Medousa is already working on a response.");
    const client = this.getClient();
    const history = await this.ensureSession();
    const defaults = await client.runtimeDefaults();
    const controller = new AbortController();
    this.activeAbort = controller;

    const request: InteractiveTurnRequest = {
      model: defaults.model,
      persist_user_turn: true,
      prompt: `${prompt.trim()}\n\n${snapshot.supplement}`,
      provider: defaults.provider,
      response_depth_mode: defaults.response_depth_mode,
      reasoning_effort: defaults.reasoning_effort,
      session_id: history.session_id,
      stage_routing: defaults.stage_routing as unknown as InteractiveTurnRequest["stage_routing"],
      media_refs: [],
      surface: {
        channel_surface: "obsidian",
        supports_browser_host: false,
        supports_ui_artifacts: false,
      },
    };

    try {
      const accepted = await client.startTurn(request, { signal: controller.signal });
      const projection = createProjectionState();
      for await (const event of client.streamTurn(accepted, { signal: controller.signal })) {
        for (const projected of projectStreamEvent(event, projection)) {
          if (projected.kind === "budget_request" || projected.kind === "permission_request") {
            await callbacks.onAttention(projected);
          } else {
            callbacks.onProjected(projected);
          }
        }
      }
    } finally {
      this.activeAbort = null;
    }
  }

  async cancelTurn(): Promise<void> {
    this.activeAbort?.abort();
    if (!this.activeSessionId || !this.client) return;
    try {
      await this.client.cancelTurn(this.activeSessionId);
    } catch {
      // Local stream cancellation still succeeds when the daemon is terminal.
    }
  }

  async respondToAttention(event: AttentionEvent, approved: boolean): Promise<void> {
    const client = this.getClient();
    if (event.kind === "budget_request") {
      if (approved) await client.approveBudget(event.requestId, event.rounds, "obsidian");
      else await client.denyBudget(event.requestId, "obsidian");
    } else {
      await client.resolvePermission(event.requestId, approved, "obsidian");
    }
  }

  async setEndpoint(endpoint: string): Promise<void> {
    const trimmed = endpoint.trim();
    if (!trimmed) throw new Error("Workshop endpoint must not be empty.");
    new URL(trimmed);
    this.settings.endpoint = trimmed;
    this.client = null;
    this.activeSessionId = null;
    await this.saveState();
  }

  setBearerToken(token: string): void {
    this.bearerToken = token.trim() || undefined;
    this.client = null;
  }

  getBearerToken(): string {
    return this.bearerToken ?? "";
  }

  chatView(): MedousaChatView | null {
    const view = this.app.workspace.getLeavesOfType(VIEW_TYPE_MEDOUSA)[0]?.view;
    return view instanceof MedousaChatView ? view : null;
  }

  private async askCurrentNote(): Promise<void> {
    const snapshot = await this.currentContext();
    await this.openChat("Help me understand and work with this note.", snapshot);
  }

  private async askSelection(): Promise<void> {
    const snapshot = await this.currentContext();
    if (!snapshot.context.selection) {
      new Notice("Select some note text first.");
      return;
    }
    await this.openChat("Explain this selection and suggest the next useful change.", snapshot);
  }

  private async generateSynthesis(period: "daily" | "weekly"): Promise<void> {
    const snapshot = await this.currentContext();
    const prompt = period === "daily"
      ? "Create a concise daily synthesis from the relevant notes in my vault. Highlight what changed, decisions, open loops, and the most useful next actions."
      : "Create a useful weekly synthesis from the relevant notes in my vault. Group themes, decisions, progress, unresolved threads, and next actions. Keep it ready to save as a Markdown note.";
    await this.openChat(prompt, snapshot);
  }

  private async rememberCurrentContext(view: MarkdownView): Promise<void> {
    this.lastContext = await captureObsidianContext(this.app, view);
  }

  private getClient(): MedousaClient {
    return (this.client ??= new MedousaClient({
      baseUrl: this.settings.endpoint,
      bearerToken: this.bearerToken,
    }));
  }

  private async saveState(): Promise<void> {
    await this.saveData({
      ...this.settings,
      activeSessionId: this.activeSessionId,
      activeSessionName: this.activeSessionName,
    });
  }
}

function sessionIdOf(session: SessionSummary): string | null {
  return session.id ?? session.session_id ?? null;
}

class MedousaChatView extends ItemView {
  readonly ready: Promise<void>;
  private resolveReady!: () => void;
  private root: HTMLElement | null = null;
  private sessionTitleEl!: HTMLButtonElement;
  private contextEl!: HTMLElement;
  private statusEl!: HTMLElement;
  private transcriptEl!: HTMLElement;
  private inputEl!: HTMLTextAreaElement;
  private sendButton!: HTMLButtonElement;
  private assistantBody: HTMLElement | null = null;
  private assistantActionsEl: HTMLElement | null = null;
  private assistantText = "";
  private lastAnswerText = "";
  private busy = false;
  private cancelRequested = false;
  private currentContext: ObsidianContextSnapshot | null = null;

  constructor(leaf: WorkspaceLeaf, private readonly plugin: MedousaPlugin) {
    super(leaf);
    this.ready = new Promise((resolve) => { this.resolveReady = resolve; });
  }

  getViewType(): string {
    return VIEW_TYPE_MEDOUSA;
  }

  getDisplayText(): string {
    return "Medousa";
  }

  getIcon(): string {
    return "message-circle";
  }

  async onOpen(): Promise<void> {
    this.renderShell();
    this.resolveReady();
    await this.refresh();
  }

  async onClose(): Promise<void> {
    this.root?.replaceChildren();
    this.root = null;
  }

  setContext(snapshot: ObsidianContextSnapshot): void {
    this.currentContext = snapshot;
    if (this.contextEl) this.contextEl.setText(`Context · ${snapshot.label}`);
  }

  async refresh(): Promise<void> {
    if (!this.root) return;
    this.setStatus("Connecting to Medousa…");
    try {
      const history = await this.plugin.ensureSession();
      this.showHistory(history);
      await this.refreshContext();
      this.setStatus("Connected");
    } catch (error) {
      this.setStatus(friendlyError(error));
      this.showError("Medousa is unavailable. Check the workshop connection and try again.");
    }
  }

  async refreshContext(): Promise<void> {
    if (!this.root) return;
    try {
      this.setContext(await this.plugin.currentContext());
    } catch (error) {
      this.contextEl?.setText(`Context unavailable · ${friendlyError(error)}`);
    }
  }

  showHistory(history: SessionHistoryResponse): void {
    if (!this.transcriptEl) return;
    this.setSessionTitle(this.plugin.sessionTitle());
    this.transcriptEl.replaceChildren();
    this.assistantBody = null;
    this.assistantActionsEl = null;
    this.assistantText = "";
    this.lastAnswerText = "";
    let renderedTurns = 0;
    for (const turn of history.turns) {
      const content = stripContextSupplement(turn.content);
      if (!content) continue;
      if (turn.role === "user" || turn.role === "assistant") {
        this.appendMessage(turn.role, content);
        renderedTurns += 1;
        if (turn.role === "assistant") this.lastAnswerText = content;
      }
    }
    if (renderedTurns === 0) this.renderEmptyState();
    else this.renderAssistantActions();
    this.scrollToLatest();
  }

  async sendPrompt(prompt: string, snapshot?: ObsidianContextSnapshot): Promise<void> {
    if (!this.root || this.busy) return;
    const context = snapshot ?? (await this.plugin.currentContext());
    this.setContext(context);
    const text = prompt.trim();
    if (!text) return;

    this.busy = true;
    this.cancelRequested = false;
    this.inputEl.value = "";
    this.inputEl.disabled = true;
    this.sendButton.setText("Stop");
    this.appendMessage("user", text);
    this.startAssistant();
    this.setStatus("Medousa is thinking…");

    try {
      await this.plugin.sendTurn(text, context, {
        onProjected: (event) => this.handleProjected(event),
        onAttention: (event) => this.handleAttention(event),
      });
    } catch (error) {
      if (!this.cancelRequested) this.showError(friendlyError(error));
    } finally {
      this.busy = false;
      this.inputEl.disabled = false;
      this.sendButton.setText("Send");
      if (!this.cancelRequested) this.setStatus("Connected");
      this.inputEl.focus();
      this.scrollToLatest();
    }
  }

  private renderShell(): void {
    this.containerEl.replaceChildren();
    this.containerEl.classList.add("medousa-chat-view");
    this.root = this.containerEl;

    const header = this.el("header", "medousa-header");
    const identity = this.el("div", "medousa-identity");
    this.sessionTitleEl = this.button(this.plugin.sessionTitle(), "Open conversation history", "medousa-title-button");
    this.sessionTitleEl.addEventListener("click", () => new ConversationModal(this.app, this.plugin, this).open());
    identity.appendChild(this.sessionTitleEl);
    this.statusEl = identity.createEl("div", { text: "Checking workshop…", cls: "medousa-status" });
    header.appendChild(identity);
    const newButton = this.button("+", "New conversation");
    newButton.addEventListener("click", () => void this.startNewConversation());
    header.appendChild(newButton);
    const configureButton = this.button("⚙", "Configure connection");
    configureButton.addEventListener("click", () => new ConnectionModal(this.app, this.plugin).open());
    header.appendChild(configureButton);
    const homeButton = this.button("⌂", "Open in Medousa Home");
    homeButton.addEventListener("click", () => this.plugin.openHome());
    header.appendChild(homeButton);
    const searchButton = this.button("⌕", "Search vault");
    searchButton.addEventListener("click", () => new VaultSearchModal(this.app, this.plugin).open());
    header.appendChild(searchButton);
    const backlinksButton = this.button("↗", "Show backlinks");
    backlinksButton.addEventListener("click", () => void this.plugin.openBacklinks());
    header.appendChild(backlinksButton);
    this.root.appendChild(header);

    this.contextEl = this.el("div", "medousa-context");
    this.contextEl.setText("Context · current vault");
    this.root.appendChild(this.contextEl);

    this.transcriptEl = this.el("main", "medousa-transcript");
    this.root.appendChild(this.transcriptEl);

    const composer = this.el("div", "medousa-composer");
    this.inputEl = document.createElement("textarea");
    this.inputEl.className = "medousa-input";
    this.inputEl.placeholder = "Ask about this note…";
    this.inputEl.rows = 2;
    this.inputEl.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
        event.preventDefault();
        void this.submitComposer();
      }
    });
    composer.appendChild(this.inputEl);
    this.sendButton = this.button("Send", "Send message", "medousa-send");
    this.sendButton.addEventListener("click", () => {
      if (this.busy) void this.stopResponse();
      else void this.submitComposer();
    });
    composer.appendChild(this.sendButton);
    this.root.appendChild(composer);
  }

  private async submitComposer(): Promise<void> {
    const text = this.inputEl.value.trim();
    if (text) await this.sendPrompt(text);
  }

  private async stopResponse(): Promise<void> {
    this.cancelRequested = true;
    this.setStatus("Stopping response…");
    await this.plugin.cancelTurn();
    this.setStatus("Response stopped");
  }

  private async startNewConversation(): Promise<void> {
    if (this.busy) {
      new Notice("Stop the current response before starting a new conversation.");
      return;
    }
    await this.plugin.newConversation();
    await this.refreshContext();
    this.inputEl.focus();
  }

  private async handleAttention(event: AttentionEvent): Promise<void> {
    const approved = await new ApprovalModal(this.app, event).waitForDecision();
    try {
      await this.plugin.respondToAttention(event, approved);
      this.setStatus(approved ? "Approved · Medousa is continuing…" : "Denied · Medousa is continuing…");
    } catch (error) {
      this.showError(friendlyError(error));
    }
  }

  private handleProjected(event: ProjectedEvent): void {
    switch (event.kind) {
      case "answer_delta":
        this.appendAssistant(event.text);
        break;
      case "answer_replace":
        this.replaceAssistant(event.text);
        break;
      case "status":
        this.setStatus(event.text);
        break;
      case "tool_started":
        this.setStatus(`Using ${event.name}…`);
        break;
      case "tool_finished":
        this.setStatus(`${event.name} · ${event.status}`);
        break;
      case "terminal":
        if (event.error && event.text) this.showError(event.text);
        else {
          this.setStatus("Connected");
          this.renderAssistantActions();
        }
        break;
      case "budget_request":
      case "permission_request":
        break;
    }
    this.scrollToLatest();
  }

  private appendMessage(role: "user" | "assistant", content: string): void {
    this.transcriptEl.querySelector(".medousa-empty")?.remove();
    const message = this.el("article", `medousa-message medousa-message-${role}`);
    message.createEl("div", { text: role === "user" ? "You" : "Medousa", cls: "medousa-message-label" });
    const body = message.createEl("div", { text: content });
    if (role === "assistant") this.assistantBody = body;
    this.transcriptEl.appendChild(message);
  }

  private startAssistant(): void {
    this.transcriptEl.querySelector(".medousa-empty")?.remove();
    const message = this.el("article", "medousa-message medousa-message-assistant");
    message.createEl("div", { text: "Medousa", cls: "medousa-message-label" });
    this.assistantBody = message.createEl("div");
    this.assistantActionsEl = null;
    this.assistantText = "";
    this.lastAnswerText = "";
    this.transcriptEl.appendChild(message);
  }

  private appendAssistant(text: string): void {
    if (!this.assistantBody) this.startAssistant();
    this.assistantText += text;
    this.lastAnswerText = this.assistantText;
    this.assistantBody?.setText(this.assistantText);
  }

  private replaceAssistant(text: string): void {
    if (!this.assistantBody) this.startAssistant();
    this.assistantText = text;
    this.lastAnswerText = text;
    this.assistantBody?.setText(text);
  }

  private showError(message: string): void {
    const error = this.el("article", "medousa-message medousa-message-error");
    error.setText(message);
    this.transcriptEl.appendChild(error);
    this.setStatus("Needs attention");
  }

  openAnswerAction(mode: "create" | "append"): void {
    const answer = this.lastAnswerText.trim();
    if (!answer) {
      new Notice("There is no settled answer to save yet.");
      return;
    }
    new NoteActionModal(this.app, this.plugin, answer, mode).open();
  }

  private renderAssistantActions(): void {
    if (!this.assistantBody || this.assistantActionsEl || !this.lastAnswerText.trim()) return;
    const actions = this.assistantBody.parentElement?.createDiv("medousa-answer-actions");
    if (!actions) return;
    this.assistantActionsEl = actions;

    const copy = document.createElement("button");
    copy.type = "button";
    copy.textContent = "Copy";
    copy.addEventListener("click", () => {
      if (!navigator.clipboard) {
        new Notice("Clipboard is unavailable in this workspace.");
        return;
      }
      void navigator.clipboard.writeText(this.lastAnswerText)
        .then(() => new Notice("Answer copied."))
        .catch(() => new Notice("Could not copy the answer."));
    });
    actions.appendChild(copy);

    const save = document.createElement("button");
    save.type = "button";
    save.textContent = "Save as note";
    save.addEventListener("click", () => this.openAnswerAction("create"));
    actions.appendChild(save);

    const append = document.createElement("button");
    append.type = "button";
    append.textContent = "Append to note";
    append.addEventListener("click", () => this.openAnswerAction("append"));
    actions.appendChild(append);
  }

  private renderEmptyState(): void {
    const empty = this.el("div", "medousa-empty");
    empty.createEl("strong", { text: "What are we working on?" });
    empty.createEl("span", { text: "Ask about this note, a selection, or the links around it." });
    this.transcriptEl.appendChild(empty);
  }

  private setStatus(text: string): void {
    this.statusEl?.setText(text);
  }

  setSessionTitle(text: string): void {
    if (this.sessionTitleEl) this.sessionTitleEl.setText(text);
  }

  private scrollToLatest(): void {
    window.requestAnimationFrame(() => {
      if (this.transcriptEl) this.transcriptEl.scrollTop = this.transcriptEl.scrollHeight;
    });
  }

  private el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string): HTMLElementTagNameMap[K] {
    const element = document.createElement(tag);
    if (className) element.className = className;
    return element;
  }

  private button(text: string, ariaLabel: string, className?: string): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = text;
    button.ariaLabel = ariaLabel;
    if (className) button.className = className;
    return button;
  }
}

class MedousaSettingTab extends PluginSettingTab {
  constructor(app: App, private readonly plugin: MedousaPlugin) {
    super(app, plugin);
  }

  display(): void {
    const { containerEl } = this;
    containerEl.replaceChildren();
    containerEl.createEl("h2", { text: "Medousa" });
    new Setting(containerEl)
      .setName("Workshop endpoint")
      .setDesc("The Medousa workshop daemon used by this vault.")
      .addText((text) => text.setValue(this.plugin.settings.endpoint).onChange(async (value) => {
        try {
          await this.plugin.setEndpoint(value);
        } catch (error) {
          new Notice(friendlyError(error));
        }
      }));
    new Setting(containerEl)
      .setName("Bearer token")
      .setDesc("Tokens are held in memory only. Use Configure connection to set one.")
      .addButton((button) => button.setButtonText("Configure connection").onClick(() => {
        new ConnectionModal(this.app, this.plugin).open();
      }));
  }
}

class ConnectionModal extends Modal {
  private endpoint = "";
  private token = "";

  constructor(app: App, private readonly plugin: MedousaPlugin) {
    super(app);
    this.endpoint = plugin.settings.endpoint;
    this.token = plugin.getBearerToken();
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.replaceChildren();
    contentEl.createEl("h2", { text: "Connect to Medousa" });
    new Setting(contentEl)
      .setName("Workshop endpoint")
      .addText((text) => text.setValue(this.endpoint).onChange((value) => { this.endpoint = value; }));
    new Setting(contentEl)
      .setName("Bearer token")
      .setDesc("Optional. Held in memory only for this Obsidian session.")
      .addText((text) => {
        text.inputEl.type = "password";
        text.setValue(this.token).onChange((value) => { this.token = value; });
      });
    new Setting(contentEl).addButton((button) => button
      .setButtonText("Save")
      .setCta()
      .onClick(() => void this.save()));
  }

  onClose(): void {
    this.contentEl.replaceChildren();
  }

  private async save(): Promise<void> {
    try {
      await this.plugin.setEndpoint(this.endpoint);
      this.plugin.setBearerToken(this.token);
      this.plugin.chatView()?.refresh();
      new Notice("Medousa connection updated.");
      this.close();
    } catch (error) {
      new Notice(friendlyError(error));
    }
  }
}

class ApprovalModal extends Modal {
  private settled = false;
  private resolve: ((approved: boolean) => void) | null = null;

  constructor(app: App, private readonly event: AttentionEvent) {
    super(app);
  }

  waitForDecision(): Promise<boolean> {
    return new Promise((resolve) => {
      this.resolve = resolve;
      this.open();
    });
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.replaceChildren();
    const title = this.event.kind === "budget_request" ? "Medousa needs more room" : "Medousa needs permission";
    contentEl.createEl("h2", { text: title });
    contentEl.createEl("p", {
      text: this.event.kind === "budget_request"
        ? `Allow ${this.event.rounds} more tool round${this.event.rounds === 1 ? "" : "s"}?`
        : this.event.message,
    });
    new Setting(contentEl)
      .addButton((button) => button.setButtonText("Deny").onClick(() => this.finish(false)))
      .addButton((button) => button.setButtonText("Approve").setCta().onClick(() => this.finish(true)));
  }

  onClose(): void {
    if (!this.settled) {
      this.settled = true;
      this.resolve?.(false);
      this.resolve = null;
    }
    this.contentEl.replaceChildren();
  }

  private finish(approved: boolean): void {
    if (this.settled) return;
    this.settled = true;
    this.resolve?.(approved);
    this.resolve = null;
    this.close();
  }
}

class ConversationModal extends Modal {
  private searchInput!: HTMLInputElement;
  private listEl!: HTMLElement;
  private sessions: SessionSummary[] = [];

  constructor(
    app: App,
    private readonly plugin: MedousaPlugin,
    private readonly view: MedousaChatView,
  ) {
    super(app);
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.replaceChildren();
    contentEl.createEl("h2", { text: "Conversations" });
    const toolbar = contentEl.createDiv("medousa-modal-toolbar");
    this.searchInput = document.createElement("input");
    this.searchInput.type = "search";
    this.searchInput.placeholder = "Search conversations…";
    this.searchInput.addEventListener("input", () => this.renderList());
    toolbar.appendChild(this.searchInput);
    const newButton = document.createElement("button");
    newButton.type = "button";
    newButton.textContent = "New";
    newButton.addEventListener("click", () => void this.startNew());
    toolbar.appendChild(newButton);
    this.listEl = contentEl.createDiv("medousa-session-list");
    this.listEl.setText("Loading conversations…");
    void this.load();
    window.setTimeout(() => this.searchInput.focus(), 0);
  }

  onClose(): void {
    this.contentEl.replaceChildren();
  }

  private async load(): Promise<void> {
    try {
      this.sessions = await this.plugin.listSessions();
      this.renderList();
    } catch (error) {
      this.listEl.setText(friendlyError(error));
    }
  }

  private renderList(): void {
    if (!this.listEl) return;
    this.listEl.replaceChildren();
    const query = this.searchInput.value.trim().toLowerCase();
    const sessions = this.sessions.filter((session) => {
      const haystack = `${sessionDisplayName(session)} ${session.preview ?? ""}`.toLowerCase();
      return !query || haystack.includes(query);
    });
    if (sessions.length === 0) {
      this.listEl.setText(query ? "No matching conversations." : "No conversations yet.");
      return;
    }

    for (const session of sessions) {
      const id = sessionIdOf(session);
      if (!id) continue;
      const row = this.listEl.createDiv("medousa-session-row");
      const select = document.createElement("button");
      select.type = "button";
      select.className = "medousa-session-select";
      select.innerHTML = `<strong></strong><span></span>`;
      const title = select.querySelector("strong");
      const preview = select.querySelector("span");
      title?.setText(sessionDisplayName(session));
      preview?.setText(session.preview?.trim() || "No messages yet");
      select.addEventListener("click", () => void this.openSession(id));
      row.appendChild(select);

      const actions = row.createDiv("medousa-session-actions");
      const rename = document.createElement("button");
      rename.type = "button";
      rename.textContent = "Rename";
      rename.addEventListener("click", () => void this.renameSession(id, sessionDisplayName(session)));
      actions.appendChild(rename);
      const remove = document.createElement("button");
      remove.type = "button";
      remove.textContent = "Delete";
      remove.addEventListener("click", () => void this.deleteSession(id, sessionDisplayName(session)));
      actions.appendChild(remove);
    }
  }

  private async openSession(sessionId: string): Promise<void> {
    try {
      const history = await this.plugin.switchSession(sessionId);
      this.view.showHistory(history);
      await this.view.refreshContext();
      this.close();
    } catch (error) {
      new Notice(friendlyError(error));
    }
  }

  private async startNew(): Promise<void> {
    try {
      await this.plugin.newConversation();
      this.close();
      await this.view.refreshContext();
    } catch (error) {
      new Notice(friendlyError(error));
    }
  }

  private async renameSession(sessionId: string, currentName: string): Promise<void> {
    const name = await new RenameModal(this.app, currentName).waitForName();
    if (!name) return;
    try {
      await this.plugin.renameSession(sessionId, name);
      this.sessions = await this.plugin.listSessions();
      this.renderList();
      this.view.setSessionTitle(this.plugin.sessionTitle());
    } catch (error) {
      new Notice(friendlyError(error));
    }
  }

  private async deleteSession(sessionId: string, name: string): Promise<void> {
    const confirmed = await new ConfirmModal(
      this.app,
      "Delete conversation?",
      `Delete “${name}” and its Medousa memory? This cannot be undone.`,
    ).waitForDecision();
    if (!confirmed) return;
    try {
      const history = await this.plugin.deleteSession(sessionId);
      if (history) this.view.showHistory(history);
      this.sessions = await this.plugin.listSessions();
      this.renderList();
      this.view.setSessionTitle(this.plugin.sessionTitle());
    } catch (error) {
      new Notice(friendlyError(error));
    }
  }
}

class RenameModal extends Modal {
  private name = "";
  private settled = false;
  private resolve: ((name: string | null) => void) | null = null;

  constructor(app: App, currentName: string) {
    super(app);
    this.name = currentName === "Untitled conversation" ? "" : currentName;
  }

  waitForName(): Promise<string | null> {
    return new Promise((resolve) => {
      this.resolve = resolve;
      this.open();
    });
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.replaceChildren();
    contentEl.createEl("h2", { text: "Name conversation" });
    const input = document.createElement("input");
    input.type = "text";
    input.value = this.name;
    input.placeholder = "e.g. Research notes";
    input.addEventListener("input", () => { this.name = input.value; });
    contentEl.appendChild(input);
    new Setting(contentEl).addButton((button) => button
      .setButtonText("Cancel")
      .onClick(() => this.finish(null)))
      .addButton((button) => button
        .setButtonText("Save")
        .setCta()
        .onClick(() => this.finish(this.name.trim() || null)));
    window.setTimeout(() => { input.focus(); input.select(); }, 0);
  }

  onClose(): void {
    if (!this.settled) {
      this.settled = true;
      this.resolve?.(null);
      this.resolve = null;
    }
    this.contentEl.replaceChildren();
  }

  private finish(name: string | null): void {
    if (this.settled) return;
    this.settled = true;
    this.resolve?.(name);
    this.resolve = null;
    this.close();
  }
}

class ConfirmModal extends Modal {
  private settled = false;
  private resolve: ((confirmed: boolean) => void) | null = null;

  constructor(app: App, private readonly title: string, private readonly message: string) {
    super(app);
  }

  waitForDecision(): Promise<boolean> {
    return new Promise((resolve) => {
      this.resolve = resolve;
      this.open();
    });
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.replaceChildren();
    contentEl.createEl("h2", { text: this.title });
    contentEl.createEl("p", { text: this.message });
    new Setting(contentEl)
      .addButton((button) => button.setButtonText("Cancel").onClick(() => this.finish(false)))
      .addButton((button) => button.setButtonText("Delete").setWarning().onClick(() => this.finish(true)));
  }

  onClose(): void {
    if (!this.settled) {
      this.settled = true;
      this.resolve?.(false);
      this.resolve = null;
    }
    this.contentEl.replaceChildren();
  }

  private finish(confirmed: boolean): void {
    if (this.settled) return;
    this.settled = true;
    this.resolve?.(confirmed);
    this.resolve = null;
    this.close();
  }
}

class VaultSearchModal extends Modal {
  private queryInput!: HTMLInputElement;
  private resultEl!: HTMLElement;
  private searchTimer: number | null = null;
  private requestId = 0;

  constructor(app: App, private readonly plugin: MedousaPlugin) {
    super(app);
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.replaceChildren();
    contentEl.createEl("h2", { text: "Search vault" });
    this.queryInput = document.createElement("input");
    this.queryInput.type = "search";
    this.queryInput.placeholder = "Search note titles and content…";
    this.queryInput.addEventListener("input", () => this.scheduleSearch());
    this.queryInput.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        void this.search();
      }
    });
    contentEl.appendChild(this.queryInput);
    this.resultEl = contentEl.createDiv("medousa-vault-results");
    this.resultEl.setText("Search the Medousa vault for a note, idea, or phrase.");
    window.setTimeout(() => this.queryInput.focus(), 0);
  }

  onClose(): void {
    if (this.searchTimer !== null) window.clearTimeout(this.searchTimer);
    this.searchTimer = null;
    this.contentEl.replaceChildren();
  }

  private scheduleSearch(): void {
    if (this.searchTimer !== null) window.clearTimeout(this.searchTimer);
    const query = this.queryInput.value.trim();
    if (query.length < 2) {
      this.resultEl.setText("Type at least two characters to search.");
      return;
    }
    this.searchTimer = window.setTimeout(() => {
      this.searchTimer = null;
      void this.search();
    }, 260);
  }

  private async search(): Promise<void> {
    const query = this.queryInput.value.trim();
    if (query.length < 2) return;
    const requestId = ++this.requestId;
    this.resultEl.setText("Searching…");
    try {
      const response = await this.plugin.searchVault(query);
      if (requestId !== this.requestId) return;
      this.renderResults(response);
    } catch (error) {
      if (requestId === this.requestId) this.resultEl.setText(friendlyError(error));
    }
  }

  private renderResults(response: VaultSearchResponse): void {
    this.resultEl.replaceChildren();
    if (response.hits.length === 0) {
      this.resultEl.setText("No notes matched that search.");
      return;
    }
    for (const hit of response.hits) {
      const row = this.resultEl.createDiv("medousa-vault-result");
      const open = document.createElement("button");
      open.type = "button";
      open.className = "medousa-vault-result-main";
      const title = document.createElement("strong");
      title.textContent = hit.note.title || hit.note.path;
      const path = document.createElement("span");
      path.textContent = hit.note.path;
      const snippet = document.createElement("small");
      snippet.textContent = hit.snippet?.trim() || "";
      open.append(title, path, snippet);
      open.addEventListener("click", () => {
        void this.app.workspace.openLinkText(hit.note.path, "", true);
        this.close();
      });
      row.appendChild(open);
      const link = document.createElement("button");
      link.type = "button";
      link.textContent = "Insert link";
      link.addEventListener("click", () => {
        this.plugin.insertLinkIntoCurrentNote(hit.note.path);
        this.close();
      });
      row.appendChild(link);
    }
  }
}

class BacklinksModal extends Modal {
  constructor(
    app: App,
    private readonly plugin: MedousaPlugin,
    private readonly path: string,
  ) {
    super(app);
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.replaceChildren();
    contentEl.createEl("h2", { text: "Backlinks" });
    contentEl.createEl("p", { text: this.path, cls: "medousa-muted" });
    const list = contentEl.createDiv("medousa-vault-results");
    list.setText("Finding notes that link here…");
    void this.load(list);
  }

  onClose(): void {
    this.contentEl.replaceChildren();
  }

  private async load(list: HTMLElement): Promise<void> {
    try {
      const response = await this.plugin.vaultBacklinks(this.path);
      list.replaceChildren();
      if (response.backlinks.length === 0) {
        list.setText("No backlinks found.");
        return;
      }
      for (const backlink of response.backlinks) {
        const button = document.createElement("button");
        button.type = "button";
        button.className = "medousa-vault-result-main";
        button.textContent = backlink;
        button.addEventListener("click", () => {
          void this.app.workspace.openLinkText(backlink, "", true);
          this.close();
        });
        list.appendChild(button);
      }
    } catch (error) {
      list.setText(friendlyError(error));
    }
  }
}

class NoteActionModal extends Modal {
  private path = "";
  private pathInput!: HTMLInputElement;
  private previewEl!: HTMLElement;
  private saving = false;

  constructor(
    app: App,
    private readonly plugin: MedousaPlugin,
    private readonly answer: string,
    private readonly mode: "create" | "append",
  ) {
    super(app);
    this.path = mode === "append" ? "" : `inbox/${slugifyNoteTitle(answer)}.md`;
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.replaceChildren();
    contentEl.createEl("h2", { text: this.mode === "create" ? "Save answer as note" : "Append answer to note" });
    const description = this.mode === "create"
      ? "Choose a Markdown path. Medousa will create it after you approve the preview."
      : "Medousa will re-read the note and use its content hash before applying this append.";
    contentEl.createEl("p", { text: description });
    new Setting(contentEl)
      .setName("Note path")
      .addText((text) => {
        this.pathInput = text.inputEl;
        text.setValue(this.path).onChange((value) => { this.path = value; });
      });
    this.previewEl = contentEl.createEl("pre", { cls: "medousa-note-preview" });
    this.previewEl.setText(this.answer);
    new Setting(contentEl)
      .addButton((button) => button.setButtonText("Cancel").onClick(() => this.close()))
      .addButton((button) => button
        .setButtonText(this.mode === "create" ? "Create note" : "Append")
        .setCta()
        .onClick(() => void this.apply()));
    if (this.mode === "append") void this.loadCurrentPath();
    window.setTimeout(() => this.pathInput.focus(), 0);
  }

  onClose(): void {
    this.contentEl.replaceChildren();
  }

  private async loadCurrentPath(): Promise<void> {
    const snapshot = await this.plugin.currentContext();
    if (!this.path && snapshot.file) {
      this.path = snapshot.file.path;
      if (this.pathInput) this.pathInput.value = this.path;
    }
  }

  private async apply(): Promise<void> {
    if (this.saving) return;
    let selectedPath = this.path;
    if (this.mode === "append" && !selectedPath) {
      selectedPath = (await this.plugin.currentContext()).file?.path ?? "";
    }
    const path = normalizeNotePath(selectedPath);
    if (!path) {
      new Notice("Choose a valid Markdown note path.");
      return;
    }
    this.saving = true;
    try {
      if (this.mode === "create") {
        await this.plugin.createVaultNote({
          path,
          content: this.answer,
          session_id: this.plugin.sessionId() ?? undefined,
          auto_workshop_tags: false,
        });
        new Notice(`Created ${path}.`);
      } else {
        const current = await this.plugin.getVaultNote(path);
        const separator = current.content.endsWith("\n") ? "\n" : "\n\n";
        await this.plugin.updateVaultNote(path, `${current.content}${separator}${this.answer}\n`, current.note.content_hash);
        new Notice(`Appended to ${path}.`);
      }
      this.close();
    } catch (error) {
      new Notice(friendlyError(error));
    } finally {
      this.saving = false;
    }
  }
}

function normalizeNotePath(path: string): string | null {
  const trimmed = path.trim().replace(/^\/+/, "");
  if (!trimmed || trimmed.split("/").some((part) => part === ".." || part === ".")) return null;
  return trimmed.toLowerCase().endsWith(".md") ? trimmed : `${trimmed}.md`;
}

function slugifyNoteTitle(content: string): string {
  const firstLine = content.split(/\r?\n/).find((line) => line.trim()) ?? "medousa-answer";
  const title = firstLine.replace(/^#+\s*/, "").trim().toLowerCase();
  return title.replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 60) || "medousa-answer";
}

function sessionDisplayName(session: SessionSummary): string {
  return session.display_name?.trim() || "Untitled conversation";
}

function friendlyError(error: unknown): string {
  if (error instanceof MedousaHttpError) {
    if (error.status === 401 || error.status === 403) return "Authorization required. Configure a current workshop token.";
    if (error.status === 404) return "This Medousa session is no longer available.";
    if (error.status === 412) return "This note changed before the preview was applied. Refresh it and try again.";
    return `Medousa request failed (${error.status}).`;
  }
  if (error instanceof Error && error.message.trim()) {
    if (error.message.toLowerCase().includes("failed to fetch")) return "Workshop unavailable. Is Medousa running?";
    return error.message;
  }
  return "Something went wrong. Try again in a moment.";
}
