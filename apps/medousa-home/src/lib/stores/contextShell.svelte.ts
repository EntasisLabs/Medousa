import type { ContextTabId } from "$lib/types/context";
import { contextThreads } from "$lib/stores/contextThreads.svelte";

/** Desktop shell rail ↔ ContextPanel bridge (list + mode live in the master rail). */
export class ContextShellStore {
  activeTab = $state<ContextTabId>("recall");
  search = $state("");
  threadSessionFilter = $state<string | null>(null);
  selectedRecallId = $state<string | null>(null);
  selectedThreadId = $state<string | null>(null);
  selectedPostureId = $state<string | null>(null);
  selectedMapNodeId = $state<string | null>(null);

  setTab(tab: ContextTabId) {
    if (this.activeTab === tab) return;
    this.activeTab = tab;
    this.search = "";
    this.selectedRecallId = null;
    this.selectedThreadId = null;
    this.selectedPostureId = null;
    this.selectedMapNodeId = null;
    if (tab !== "threads") {
      this.threadSessionFilter = null;
    }
    contextThreads.clearDetail();
  }

  selectRecall(id: string) {
    this.selectedRecallId = id;
  }

  selectThread(id: string) {
    this.selectedThreadId = id;
    void contextThreads.loadDetail(id);
  }

  selectPosture(id: string) {
    this.selectedPostureId = id;
  }

  selectMapNode(id: string | null) {
    this.selectedMapNodeId = id;
  }

  clearThreadSessionFilter() {
    this.threadSessionFilter = null;
    this.selectedThreadId = null;
    contextThreads.clearDetail();
  }

  openThreadsForSession(sessionId: string, syncKey?: string) {
    this.activeTab = "threads";
    this.search = "";
    this.threadSessionFilter = sessionId;
    this.selectedRecallId = null;
    this.selectedPostureId = null;
    this.selectedMapNodeId = null;
    this.selectedThreadId = syncKey?.trim() || null;
    if (syncKey?.trim()) {
      void contextThreads.loadDetail(syncKey.trim());
    } else {
      contextThreads.clearDetail();
    }
  }

  openPostureForSession(sessionId: string) {
    this.activeTab = "posture";
    this.search = "";
    this.threadSessionFilter = null;
    this.selectedRecallId = null;
    this.selectedThreadId = null;
    this.selectedMapNodeId = null;
    this.selectedPostureId = `posture:${sessionId}`;
    contextThreads.clearDetail();
  }
}

export const contextShell = new ContextShellStore();
