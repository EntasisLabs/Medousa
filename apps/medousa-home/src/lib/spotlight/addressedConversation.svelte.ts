import { uploadChatFiles } from "$lib/utils/chatMediaUpload";
import { resourceContext } from "$lib/utils/turnContext";
import { MAX_MEDIA_REFS_PER_TURN, MAX_MEDIA_UPLOAD_MB } from "$lib/utils/normieErrors";
import type { MediaRef } from "$lib/types/media";
import type { TurnResourceReference } from "$lib/types/generated/daemon_api";
import { isRuntimeStartupText, visibleChatStatusLine } from "$lib/utils/chatStreamDisplay";
import { createSession, getActiveSessionTurn, getSessionHistory, listAgentRuntimes, setSessionCodeBinding, type AgentRuntimeInfo } from "$lib/daemon";
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
import type { TurnTicketResponse } from "$lib/types/session";

export function transcriptExcerpt(messages: readonly ChatMessage[]): ChatMessage[] {
  return messages.filter(message => message.role !== "system").map(message => ({
    id: message.id, role: message.role,
    content: message.role === "assistant" && isRuntimeStartupText(message.content) ? "" : message.content,
    createdAt: message.createdAt, streaming: message.streaming,
    turnId: message.turnId, failed: message.failed, errorLine: message.errorLine,
    hostContext: message.hostContext, mediaAttachments: message.mediaAttachments,
    statusLine: message.streaming ? visibleChatStatusLine(message.statusLine, false) : null,
  })).filter(message => message.content.trim() || message.mediaAttachments?.length || message.streaming || message.failed).slice(-6);
}

/** Surface-independent destination and delivery. Native streams still belong to ChatStore. */
export class AddressedConversation {
  constructor(private onAdmitted?: (ticket: TurnTicketResponse, label: string) => void) {}
  target = $state<ConversationTarget | null>(null);
  files = $state<File[]>([]);
  references = $state<TurnResourceReference[]>([]);
  uploading = $state(false);
  private uploaded: { sessionId: string; files: File[]; refs: MediaRef[] } | null = null;
  private draftEpoch = chat.workshopEpoch;
  addFiles(files: File[]) {
    if (this.sending) { this.error = "Wait for this message to finish sending before attaching another image."; return; }
    if (this.draftEpoch !== chat.workshopEpoch) { this.files = []; this.references = []; this.uploaded = null; }
    this.draftEpoch = chat.workshopEpoch;
    if (files.some(file => !file.type.startsWith("image/"))) { this.error = "Attach an image or screenshot."; return; }
    if (this.files.length + files.length > MAX_MEDIA_REFS_PER_TURN) { this.error = `Attach up to ${MAX_MEDIA_REFS_PER_TURN} images.`; return; }
    if (files.some(file => file.size > MAX_MEDIA_UPLOAD_MB * 1024 * 1024)) { this.error = `Images must be under ${MAX_MEDIA_UPLOAD_MB} MB.`; return; }
    this.files = [...this.files, ...files]; this.error = null;
  }
  removeFile(index: number) { if (!this.sending) this.files = this.files.filter((_, i) => i !== index); }

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
    // A persisted reply settles the same turn even if this window missed its
    // terminal stream event. Keep newer live turns intact while history catches up.
    const live = this.sessionId ? chat.messagesFor(this.sessionId) : [];
    const pending = live.findLast(message => message.role === "assistant" && message.streaming);
    const submittedAt = live.findLast(message => message.role === "user")?.createdAt ?? "";
    const committed = pending?.turnId && this.preview.some(message => message.role === "assistant"
      && message.turnId === pending.turnId && (message.createdAt ?? "") >= submittedAt);
    if (committed) return transcriptExcerpt(this.preview);
    const liveAt = live.reduce((latest, message) => message.createdAt || latest, "");
    const previewAt = this.preview.at(-1)?.createdAt ?? "";
    return transcriptExcerpt(live.some(message => message.streaming) || (live.length && liveAt >= previewAt) ? live : this.preview);
  }
  get working(): boolean {
    return Boolean(this.sessionId && (runtimeBotJobPending(this.sessionId) || this.messages.some(message => message.streaming)));
  }
  get failure(): string | null {
    if (!this.sessionId) return null;
    const job = runtimeBotJob(this.sessionId);
    if (job?.status === "failed") return job.error || "This request couldn’t finish.";
    const last = this.messages.at(-1);
    return last?.failed ? last.errorLine || "This request couldn’t finish." : null;
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
    this.files = []; this.references = []; this.uploaded = null; this.draftEpoch = chat.workshopEpoch;
    this.loading = false;
    this.error = null;
    // Dismissing a surface never cancels an admitted turn or its stream.
  }
  select(target: ConversationTarget, resumedSessionId: string | null = null) {
    const files = this.draftEpoch === chat.workshopEpoch ? this.files : [];
    const references = this.draftEpoch === chat.workshopEpoch ? this.references : [];
    this.clear();
    this.files = files; this.references = references;
    this.target = target;
    this.sessionId = target.kind === "session" ? target.sessionId : target.kind === "bot" ? target.bot.primary_session_id || null : resumedSessionId;
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
        this.preview = mapTurns(history.turns, { sessionId, authorityId: history.authority_id });
      }
    } catch {
      // Optional preview refreshes retain the last answer. Delivery failures
      // belong to send(), not to a background history read.
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
  async bindProject(workId: string): Promise<void> {
    const target = this.target;
    if (!target || this.sending) throw new Error("Choose a conversation before binding a project.");
    const epoch = chat.workshopEpoch, generation = this.generation;
    const current = () => epoch === chat.workshopEpoch && generation === this.generation && target.scope === chat.workshopScopeId;
    if (!current()) throw new Error("The workshop changed. Select the project again.");
    let sessionId = this.sessionId;
    if (!sessionId) {
      if (target.kind === "bot") sessionId = (await bots.open(target.bot)).binding.session_id;
      else if (target.kind === "new") {
        sessionId = (await createSession()).session_id;
        if (current()) setSessionAgentRuntime(sessionId, target.runtime);
      }
    }
    if (!sessionId || !current()) throw new Error("The conversation changed before binding.");
    this.sessionId = sessionId;
    await setSessionCodeBinding(sessionId, workId);
  }
  send(text: string, onAccepted?: () => void): Promise<void> {
    if (this.submission) return Promise.reject(new Error("A message is still sending."));
    const target = this.target;
    if (!target || (!text.trim() && !this.files.length)) return Promise.resolve();
    const assistantLabel = this.assistantLabel;
    const generation = this.generation;
    const epoch = chat.workshopEpoch;
    const current = () => epoch === chat.workshopEpoch && target.scope === chat.workshopScopeId;
    const files = [...this.files];
    const references = structuredClone($state.snapshot(this.references));
    this.sending = true;
    this.error = null;
    const request = (async () => {
      if (!current()) throw new Error("Workshop changed. Select a destination again.");
      if (connection.offline) throw new Error("Reconnect to your workshop to send.");
      const targetRuntime = target.kind === "new" ? target.runtime : getSessionAgentRuntime(this.sessionId ?? "");
      if (files.length && (isExternalAgentRuntime(targetRuntime) || (target.kind === "bot" && target.bot.external_agent))) throw new Error("This agent accepts text only. Select Medousa to use screenshots.");
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
      let mediaRefs: MediaRef[] = [];
      if (files.length) {
        const cached = this.uploaded;
        if (cached && cached.sessionId === sessionId && cached.files.length === files.length && cached.files.every((file, i) => file === files[i])) mediaRefs = cached.refs;
        else {
          this.uploading = true;
          try { mediaRefs = await uploadChatFiles(sessionId, files, current); }
          finally { this.uploading = false; }
          if (!current() || generation !== this.generation) throw new Error("The conversation changed while uploading. Attach the image again.");
          this.uploaded = { sessionId, files, refs: mediaRefs };
        }
      }
      if (generation !== this.generation) throw new Error("The conversation changed before sending.");
      await submitChatTurn({
        sessionId, userContent: text.trim(), prompt: text.trim(), mediaRefs, hostContext: resourceContext("spotlight", references), mode: active.active ? "background" : "interactive",
        synchronizeAgentSession: (id, runtime) => prepareAgentSession(id, runtime),
        onAgentSessionLost: () => {}, scrollToLatest: () => {}, onAccepted: ticket => {
          this.onAdmitted?.(ticket, assistantLabel);
          if (generation === this.generation) { this.files = []; this.references = []; this.uploaded = null; }
          onAccepted?.();
        },
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
