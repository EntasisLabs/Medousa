import type { ChatStoreHost } from "./chatStoreHost";
import type { ChatMessage } from "$lib/types/chat";
import { getSessionHistory } from "$lib/daemon";
import { mapTurns, TRANSCRIPT_PAGE_SIZE, reloadCurrentSession, warmBackgroundSession, reconcileOnResume } from "./sessionController";
import { emptySessionRuntime } from "./chatSessionRuntime";
import { loadDraftForSession, persistDraftForSession } from "./draftPersistence";
import { dedupeMessagesById, mergeTranscript } from "$lib/utils/mergeTranscript";
import { chatScenes } from "$lib/liquid/surfaces/chat/chatScenes.svelte";
import { boundedLiveTranscript, TranscriptWindow } from "./transcriptWindow";

/** Viewport history owns old pages; stream reducers own the bounded live tail. */
export class ChatHistoryController {
  private windows = new Map<string, TranscriptWindow>();
  private released = new Set<string>();
  private readers = new Map<string, number>();
  private revision = $state(0);

  constructor(private host: ChatStoreHost) {}

  isReleased(id: string) { return this.released.has(id); }
  open(id: string) { this.released.delete(id); }
  reset() {
    for (const window of this.windows.values()) { window.pages = []; window.newer = []; }
    this.windows.clear(); this.released.clear(); this.readers.clear(); this.revision += 1;
  }
  private key(id: string, viewId = "default") { return `${id}\0${viewId}`; }
  dropViewport(id: string, viewId?: string) {
    for (const [key, window] of this.windows) {
      if (viewId === undefined ? key.startsWith(`${id}\0`) : key === this.key(id, viewId)) {
        window.pages = []; window.newer = [];
        this.windows.delete(key);
      }
    }
    this.revision += 1;
  }

  retain(id: string, viewId = "default"): () => void {
    const epoch = this.host.workshopEpoch;
    this.open(id);
    this.readers.set(id, (this.readers.get(id) ?? 0) + 1);
    if (id === this.host.sessionId) {
      if (!this.host.messages.length && !this.host.historyLoading && !this.host.sessionPristine) void reloadCurrentSession(this.host, { notice: false });
    } else void warmBackgroundSession(this.host, id);
    return () => {
      if (epoch !== this.host.workshopEpoch) return;
      const readers = (this.readers.get(id) ?? 1) - 1;
      if (readers > 0) this.readers.set(id, readers);
      else this.readers.delete(id);
      this.dropViewport(id, viewId);
    };
  }

  messagesFor(id: string, viewId = "default"): ChatMessage[] {
    void this.revision;
    const window = this.windows.get(this.key(id, viewId));
    if (window && !window.atLatest) return window.messages([]);
    void this.host.runtimeRevision;
    const live = id === this.host.sessionId ? this.host.messages : this.host.sessionRuntimes.get(id)?.messages ?? [];
    return this.windows.get(this.key(id, viewId))?.messages(live) ?? live;
  }

  olderCursorFor(id: string, viewId = "default"): string | null {
    void this.revision;
    const window = this.windows.get(this.key(id, viewId));
    if (window?.pages[0]?.cursor != null) return window.olderCursor;
    return id === this.host.sessionId ? this.host.historyCursor : this.host.sessionRuntimes.get(id)?.historyCursor ?? null;
  }

  canLoadNewer(id: string, viewId = "default") { void this.revision; return Boolean(this.windows.get(this.key(id, viewId))?.newer.length); }
  loadingFor(id: string, viewId = "default") { void this.revision; return this.windows.get(this.key(id, viewId))?.loading ?? false; }

  sanitize() {
    const host = this.host;
    const messages = dedupeMessagesById(host.messages);
    const activeIds = new Set([...host.turns.values()].filter(turn => !turn.terminal).map(turn => turn.messageId).filter((id): id is string => Boolean(id)));
    const next = boundedLiveTranscript(messages, activeIds);
    if (next.length === host.messages.length) return;
    const retained = new Set(next.map(message => message.id));
    for (const message of host.messages) if (!retained.has(message.id)) chatScenes.clear?.(message.id);
    host.messages = next;
    host.messageIndexes.delete(host.sessionId);
    const oldest = next.find(message => message.transcript?.sessionId === host.sessionId)?.transcript?.entrySeq;
    if (oldest && messages.length > next.length) host.historyCursor = String(oldest);
    host.stashFocusedRuntime();
  }

  release(id: string) {
    if (this.readers.has(id)) return;
    this.released.add(id);
    this.dropViewport(id);
    const host = this.host;
    const runtime = id === host.sessionId ? undefined : host.sessionRuntimes.get(id);
    const messages = runtime?.messages ?? (id === host.sessionId ? host.messages : []);
    persistDraftForSession(id, runtime?.draft ?? (id === host.sessionId ? host.draft : loadDraftForSession(id, host.workshopScopeId)), host.workshopScopeId);
    for (const message of messages) {
      chatScenes.clear?.(message.id);
      const timer = host.contentRevealTimers.get(message.id);
      if (timer) clearTimeout(timer);
      host.contentRevealTimers.delete(message.id);
      if (message.turnId) {
        const reconcile = host.terminalReconcileTimers.get(message.turnId);
        if (reconcile) clearTimeout(reconcile);
        host.terminalReconcileTimers.delete(message.turnId);
      }
    }
    for (const owner of host.streamOwners.values()) if (owner.sessionId === id) void host.detachStreamOwner(owner.turnId);
    host.messageIndexes.delete(id);
    if (id === host.sessionId) {
      host.transcriptEpoch += 1;
      const empty = emptySessionRuntime(id, host.draft);
      empty.transcriptEpoch = host.transcriptEpoch;
      host.loadRuntimeIntoFocused(empty);
    }
    host.sessionRuntimes.delete(id);
    host.bumpRuntimeRevision();
  }

  async loadOlder(id: string, viewId = "default"): Promise<number> { return this.load(id, "older", viewId); }
  async loadNewer(id: string, viewId = "default"): Promise<number> { return this.load(id, "newer", viewId); }

  async latest(id: string, viewId = "default"): Promise<void> {
    this.dropViewport(id, viewId);
    if (id === this.host.sessionId) await reconcileOnResume(this.host, { notice: false });
    this.revision += 1;
  }

  private async load(id: string, direction: "older" | "newer", viewId: string): Promise<number> {
    const host = this.host;
    if (!id || this.isReleased(id)) return 0;
    let window = this.windows.get(this.key(id, viewId));
    if (!window) {
      window = new TranscriptWindow(this.olderCursorFor(id, viewId));
      this.windows.set(this.key(id, viewId), window);
    }
    if (window.loading) return 0;
    const cursor = direction === "older" ? this.olderCursorFor(id, viewId) : window.newer.at(-1);
    if (direction === "older" ? !cursor : !window.newer.length) return 0;
    const epoch = host.workshopEpoch;
    window.loading = true;
    this.revision += 1;
    try {
      const history = await getSessionHistory(id, { limit: TRANSCRIPT_PAGE_SIZE, ...(cursor ? { cursor } : {}) });
      if (epoch !== host.workshopEpoch || this.windows.get(this.key(id, viewId)) !== window || this.isReleased(id)) return 0;
      const messages = mapTurns(history.turns, { sessionId: id, authorityId: history.authority_id });
      const page = { cursor: cursor ?? null, olderCursor: history.next_cursor?.trim() || null, messages };
      if (direction === "older") {
        if (page.olderCursor === cursor) page.olderCursor = null;
        window.prepend(page);
      } else {
        if (cursor == null) {
          host.withSessionFields(id, () => {
            host.messages = mergeTranscript(host.messages, messages);
            host.historyCursor = page.olderCursor;
            this.sanitize();
          });
          page.messages = [];
        }
        window.append(page);
      }
      return messages.length;
    } finally {
      window.loading = false;
      this.revision += 1;
    }
  }
}
