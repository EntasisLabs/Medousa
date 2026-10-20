import { isRuntimeStartupText, visibleChatStatusLine } from "$lib/utils/chatStreamDisplay";
import { createSession, getActiveSessionTurn, getSessionHistory, listAgentRuntimes, type AgentRuntimeInfo } from "$lib/daemon";
import { chat } from "$lib/stores/chat.svelte";
import { bots } from "$lib/stores/bots.svelte";
import { connection } from "$lib/stores/connection.svelte";
import { mapTurns } from "$lib/chat/sessionController";
import { submitChatTurn } from "$lib/chat/submitTurnController";
import { prepareAgentSession } from "$lib/chat/prepareAgentSession";
import { refreshRuntimeBotTurn, runtimeBotJob, runtimeBotJobPending } from "$lib/chat/runtimeBotTurns.svelte";
import { agentRuntimeLabel, isExternalAgentRuntime, getSessionAgentRuntime, setSessionAgentRuntime } from "$lib/utils/sessionAgentRuntime";
import type { ChatMessage } from "$lib/types/chat";
import type { ConversationTarget } from "./conversationTargets";

export function transcriptExcerpt(messages: readonly ChatMessage[]): ChatMessage[] {
  return messages.filter(message => message.role !== "system").map(message => ({
    id: message.id, role: message.role,
    content: message.role === "assistant" && isRuntimeStartupText(message.content) ? "" : message.content.slice(-4000),
    createdAt: message.createdAt, streaming: message.streaming,
    statusLine: visibleChatStatusLine(message.statusLine, false),
  })).filter(message => message.content.trim() || message.streaming || message.statusLine).slice(-6);
}

/** Surface-independent destination and delivery. Native streams still belong to ChatStore. */
export class AddressedConversation {
  target = $state<ConversationTarget | null>(null);
  sessionId = $state<string | null>(null);
  loading = $state(false);
  sending = $state(false);
  error = $state<string | null>(null);
  runtimes = $state<AgentRuntimeInfo[]>([]);
  private preview = $state<ChatMessage[]>([]);
  private ownedSessions = $state<string[]>([]);
  private ownedEpoch = chat.workshopEpoch;
  private generation = 0;
  private previewRequest = 0;
  private submission: Promise<void> | null = null;

  get assistantLabel(): string {
    if (this.target?.kind === "bot") return this.target.label;
    const runtime = this.target?.kind === "new" ? this.target.runtime : getSessionAgentRuntime(this.sessionId ?? "");
    return agentRuntimeLabel(runtime);
  }
  get messages() {
    const live = this.sessionId ? chat.messagesFor(this.sessionId) : [];
    const liveAt = live.reduce((latest, message) => message.createdAt || latest, "");
    const previewAt = this.preview.at(-1)?.createdAt ?? "";
    return transcriptExcerpt(live.some(message => message.streaming) || (live.length && liveAt >= previewAt) ? live : this.preview);
  }
  get status(): string | null {
    if (!this.sessionId) return null;
    const job = runtimeBotJob(this.sessionId);
    if (job?.error) return job.error;
    if (runtimeBotJobPending(this.sessionId)) return "Working…";
    return chat.streamErrorFor(this.sessionId);
  }
  async loadRuntimes() {
    const epoch = chat.workshopEpoch;
    try {
      const result = await listAgentRuntimes();
      if (chat.workshopEpoch === epoch) this.runtimes = result.runtimes;
    } catch { /* Medousa and saved conversations remain available. */ }
  }
  clear() {
    this.generation += 1;
    this.previewRequest += 1;
    this.target = null;
    this.sessionId = null;
    this.preview = [];
    this.loading = false;
    this.error = null;
    // Dismissing a surface never cancels an admitted turn or its stream.
  }
  select(target: ConversationTarget) {
    this.clear();
    this.target = target;
    this.sessionId = target.kind === "session" ? target.sessionId : target.kind === "bot" ? target.bot.primary_session_id || null : null;
    void this.refresh();
  }
  async refresh() {
    const sessionId = this.sessionId;
    if (!sessionId || this.target?.scope !== chat.workshopScopeId || connection.offline) return;
    const generation = this.generation;
    const request = ++this.previewRequest;
    const epoch = chat.workshopEpoch;
    this.loading = !this.preview.length && !chat.messagesFor(sessionId).length;
    try {
      if (runtimeBotJobPending(sessionId)) await refreshRuntimeBotTurn(sessionId);
      if (epoch !== chat.workshopEpoch) return;
      const history = await getSessionHistory(sessionId, { limit: 6 });
      if (generation === this.generation && request === this.previewRequest && epoch === chat.workshopEpoch) {
        this.preview = transcriptExcerpt(mapTurns(history.turns, { sessionId, authorityId: history.authority_id }));
      }
    } catch (cause) {
      if (generation === this.generation && request === this.previewRequest && epoch === chat.workshopEpoch) this.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      if (generation === this.generation && request === this.previewRequest && epoch === chat.workshopEpoch) this.loading = false;
    }
  }
  releaseInactive() {
    if (this.ownedEpoch !== chat.workshopEpoch) { this.ownedSessions = []; this.ownedEpoch = chat.workshopEpoch; return; }
    const retained = this.ownedSessions.filter(id => {
      const messages = chat.messagesFor(id);
      if (this.sending || id === this.sessionId || messages.some(message => message.streaming) || [...chat.streamOwners.values()].some(owner => owner.sessionId === id)) return true;
      if (id !== chat.sessionId) chat.history.release(id);
      return false;
    });
    if (retained.length !== this.ownedSessions.length) this.ownedSessions = retained;
  }
  send(text: string, onAccepted?: () => void): Promise<void> {
    if (this.submission) return Promise.reject(new Error("A message is still sending."));
    const target = this.target;
    if (!target || !text.trim()) return Promise.resolve();
    const generation = this.generation;
    const epoch = chat.workshopEpoch;
    const current = () => epoch === chat.workshopEpoch && target.scope === chat.workshopScopeId;
    this.sending = true;
    this.error = null;
    const request = (async () => {
      if (!current()) throw new Error("Workshop changed. Select a destination again.");
      if (connection.offline) throw new Error("Reconnect to your workshop to send.");
      let sessionId = this.sessionId;
      if (!sessionId) {
        if (target.kind === "bot") sessionId = (await bots.open(target.bot)).binding.session_id;
        else if (target.kind === "new") {
          sessionId = (await createSession()).session_id;
          if (current()) setSessionAgentRuntime(sessionId, target.runtime);
        }
      }
      if (!current()) throw new Error("Workshop changed while preparing the conversation.");
      if (!sessionId) throw new Error("This destination has no conversation.");
      if (generation === this.generation) this.sessionId = sessionId;
      if (this.ownedEpoch !== epoch) { this.ownedSessions = []; this.ownedEpoch = epoch; }
      if (sessionId !== chat.sessionId && !chat.sessionRuntimes.has(sessionId)) this.ownedSessions = [...this.ownedSessions, sessionId];
      chat.history.open(sessionId);
      await chat.warmBackgroundSession(sessionId);
      if (!current()) throw new Error("Workshop changed while loading the conversation.");
      const active = await getActiveSessionTurn(sessionId);
      if (!current()) throw new Error("Workshop changed before sending.");
      const external = isExternalAgentRuntime(getSessionAgentRuntime(sessionId));
      if (active.active && external) throw new Error("This agent is still working. Wait for its reply before sending another message.");
      await submitChatTurn({
        sessionId, userContent: text.trim(), prompt: text.trim(), mode: active.active ? "background" : "interactive",
        synchronizeAgentSession: (id, runtime) => prepareAgentSession(id, runtime),
        onAgentSessionLost: () => {}, scrollToLatest: () => {}, onAccepted,
      });
      if (current()) void chat.refreshSessions();
    })().catch(cause => {
      if (generation === this.generation && current()) this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }).finally(() => { this.sending = false; this.submission = null; });
    this.submission = request;
    return request;
  }
}
