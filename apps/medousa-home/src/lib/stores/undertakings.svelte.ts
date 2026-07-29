/**
 * ActiveUndertakingContext — per Shell tab-group undertaking binding.
 */

import type { ItemProjection, HumanPhase, ReviewProjection } from "$lib/forge";
import {
  listUndertakings,
  getUndertaking,
  createUndertaking,
  provisionUndertaking,
  getReview,
} from "$lib/forge";
import { shellTabs } from "$lib/stores/shellTabs.svelte";

export type ActiveUndertakingContext = {
  workId: string;
  title: string;
  humanPhase: HumanPhase | string;
  forgeState: string;
  worktree: string | null;
  baselineOid: string | null;
  sealedOid: string | null;
  leaseId: string | null;
  leaseGeneration: number | null;
  boundChatSessionIds: string[];
  boundTerminalSessionIds: string[];
  selectedEntityId: string | null;
  selectedPath: string | null;
};

function groupKey(): string {
  return shellTabs.activeGroupId || "default";
}

function createUndertakingsStore() {
  let items = $state<ItemProjection[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let selectedId = $state<string | null>(null);
  let detail = $state<ItemProjection | null>(null);
  let review = $state<ReviewProjection | null>(null);
  /** Map shell group id → active context */
  let contexts = $state<Record<string, ActiveUndertakingContext | null>>({});
  let workTab = $state<"activity" | "undertakings">("activity");
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  const active = $derived(contexts[groupKey()] ?? null);

  function setActiveFromItem(item: ItemProjection, merge?: Partial<ActiveUndertakingContext>) {
    const lease = item.attempts
      ?.find((a) => a.id === item.active_attempt)
      ?.lease;
    const next: ActiveUndertakingContext = {
      workId: item.id,
      title: item.title,
      humanPhase: item.human_phase,
      forgeState: item.state,
      worktree: item.environment?.worktree ?? null,
      baselineOid: item.environment?.baseline_oid ?? null,
      sealedOid: null,
      leaseId: lease?.lease_id ?? null,
      leaseGeneration: lease?.generation ?? null,
      boundChatSessionIds: [],
      boundTerminalSessionIds: [],
      selectedEntityId: null,
      selectedPath: null,
      ...merge,
    };
    const prev = contexts[groupKey()];
    if (prev?.workId === next.workId) {
      next.boundChatSessionIds = prev.boundChatSessionIds;
      next.boundTerminalSessionIds = prev.boundTerminalSessionIds;
      next.selectedEntityId = prev.selectedEntityId;
      next.selectedPath = prev.selectedPath;
      next.sealedOid = prev.sealedOid;
    }
    contexts = { ...contexts, [groupKey()]: next };
  }

  function clearActive() {
    contexts = { ...contexts, [groupKey()]: null };
  }

  function bindChat(sessionId: string) {
    const cur = contexts[groupKey()];
    if (!cur) return;
    if (cur.boundChatSessionIds.includes(sessionId)) return;
    contexts = {
      ...contexts,
      [groupKey()]: {
        ...cur,
        boundChatSessionIds: [...cur.boundChatSessionIds, sessionId],
      },
    };
  }

  function detachChat(sessionId: string) {
    const cur = contexts[groupKey()];
    if (!cur) return;
    contexts = {
      ...contexts,
      [groupKey()]: {
        ...cur,
        boundChatSessionIds: cur.boundChatSessionIds.filter((id) => id !== sessionId),
      },
    };
  }

  function bindTerminal(sessionId: string) {
    const cur = contexts[groupKey()];
    if (!cur) return;
    if (cur.boundTerminalSessionIds.includes(sessionId)) return;
    contexts = {
      ...contexts,
      [groupKey()]: {
        ...cur,
        boundTerminalSessionIds: [...cur.boundTerminalSessionIds, sessionId],
      },
    };
  }

  async function refreshList() {
    loading = true;
    error = null;
    try {
      items = await listUndertakings();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  async function select(workId: string) {
    const trimmed = workId.trim();
    if (!trimmed) {
      selectedId = null;
      detail = null;
      review = null;
      return;
    }
    selectedId = trimmed;
    try {
      detail = await getUndertaking(trimmed);
      setActiveFromItem(detail);
      if (
        detail.human_phase === "review" ||
        detail.state === "awaiting_review" ||
        detail.state === "applying_decision"
      ) {
        review = await getReview(trimmed);
        if (review.sealed_head_oid) {
          const cur = contexts[groupKey()];
          if (cur) {
            contexts = {
              ...contexts,
              [groupKey()]: { ...cur, sealedOid: review.sealed_head_oid },
            };
          }
        }
      } else {
        review = null;
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function create(input: {
    title: string;
    brief: string;
    repo_path: string;
    base_ref?: string;
  }) {
    const item = await createUndertaking(input);
    await refreshList();
    await select(item.id);
    return item;
  }

  async function provision(workId: string) {
    try {
      detail = await provisionUndertaking(workId);
      setActiveFromItem(detail);
      await refreshList();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      if ((err as { status?: number }).status === 409) {
        await select(workId);
      }
      throw err;
    }
  }

  async function refreshDetail() {
    if (!selectedId) return;
    await select(selectedId);
  }

  function startPolling() {
    stopPolling();
    pollTimer = setInterval(() => {
      if (selectedId) void refreshDetail();
      else void refreshList();
    }, 2500);
  }

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  return {
    get items() {
      return items;
    },
    get loading() {
      return loading;
    },
    get error() {
      return error;
    },
    get selectedId() {
      return selectedId;
    },
    get detail() {
      return detail;
    },
    get review() {
      return review;
    },
    get active() {
      return active;
    },
    get workTab() {
      return workTab;
    },
    setWorkTab(tab: "activity" | "undertakings") {
      workTab = tab;
    },
    refreshList,
    select,
    create,
    provision,
    refreshDetail,
    clearActive,
    setActiveFromItem,
    bindChat,
    detachChat,
    bindTerminal,
    startPolling,
    stopPolling,
  };
}

export const undertakings = createUndertakingsStore();
