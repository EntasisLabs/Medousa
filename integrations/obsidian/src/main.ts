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
  private activeAbort: AbortController | null = null;
  private lastContext: ObsidianContextSnapshot | null = null;

  async onload(): Promise<void> {
    const data = ((await this.loadData()) as PersistedData | null | undefined) ?? {};
    this.settings = {
      endpoint: data.endpoint?.trim() || DEFAULT_SETTINGS.endpoint,
    };
    this.activeSessionId = data.activeSessionId ?? null;

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
    await this.saveState();
    this.chatView()?.showHistory({ session_id: created.session_id, turns: [] });
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
    await this.saveData({ ...this.settings, activeSessionId: this.activeSessionId });
  }
}

class MedousaChatView extends ItemView {
  readonly ready: Promise<void>;
  private resolveReady!: () => void;
  private root: HTMLElement | null = null;
  private contextEl!: HTMLElement;
  private statusEl!: HTMLElement;
  private transcriptEl!: HTMLElement;
  private inputEl!: HTMLTextAreaElement;
  private sendButton!: HTMLButtonElement;
  private assistantBody: HTMLElement | null = null;
  private assistantText = "";
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
    this.transcriptEl.replaceChildren();
    this.assistantBody = null;
    this.assistantText = "";
    for (const turn of history.turns) {
      const content = stripContextSupplement(turn.content);
      if (!content) continue;
      if (turn.role === "user" || turn.role === "assistant") this.appendMessage(turn.role, content);
    }
    if (history.turns.length === 0) this.renderEmptyState();
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
    identity.createEl("div", { text: "Medousa", cls: "medousa-title" });
    this.statusEl = identity.createEl("div", { text: "Checking workshop…", cls: "medousa-status" });
    header.appendChild(identity);
    const newButton = this.button("+", "New conversation");
    newButton.addEventListener("click", () => void this.startNewConversation());
    header.appendChild(newButton);
    const configureButton = this.button("⚙", "Configure connection");
    configureButton.addEventListener("click", () => new ConnectionModal(this.app, this.plugin).open());
    header.appendChild(configureButton);
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
        else this.setStatus("Connected");
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
    message.createEl("div", { text: content });
    this.transcriptEl.appendChild(message);
  }

  private startAssistant(): void {
    this.transcriptEl.querySelector(".medousa-empty")?.remove();
    const message = this.el("article", "medousa-message medousa-message-assistant");
    message.createEl("div", { text: "Medousa", cls: "medousa-message-label" });
    this.assistantBody = message.createEl("div");
    this.assistantText = "";
    this.transcriptEl.appendChild(message);
  }

  private appendAssistant(text: string): void {
    if (!this.assistantBody) this.startAssistant();
    this.assistantText += text;
    this.assistantBody?.setText(this.assistantText);
  }

  private replaceAssistant(text: string): void {
    if (!this.assistantBody) this.startAssistant();
    this.assistantText = text;
    this.assistantBody?.setText(text);
  }

  private showError(message: string): void {
    const error = this.el("article", "medousa-message medousa-message-error");
    error.setText(message);
    this.transcriptEl.appendChild(error);
    this.setStatus("Needs attention");
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

function friendlyError(error: unknown): string {
  if (error instanceof MedousaHttpError) {
    if (error.status === 401 || error.status === 403) return "Authorization required. Configure a current workshop token.";
    if (error.status === 404) return "This Medousa session is no longer available.";
    return `Medousa request failed (${error.status}).`;
  }
  if (error instanceof Error && error.message.trim()) {
    if (error.message.toLowerCase().includes("failed to fetch")) return "Workshop unavailable. Is Medousa running?";
    return error.message;
  }
  return "Something went wrong. Try again in a moment.";
}
