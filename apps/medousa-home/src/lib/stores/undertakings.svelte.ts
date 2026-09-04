/**
 * ActiveUndertakingContext — per Shell tab-group undertaking binding.
 */

import type {
  ForgeWorkspaceMode,
  ItemProjection,
  HumanPhase,
  ReviewProjection,
} from "$lib/forge";
import {
  listUndertakings,
  getUndertaking,
  createUndertaking,
  startUndertaking,
  provisionUndertaking,
  getReview,
  forgeStreamUrl,
} from "$lib/forge";
import {
  getCoderExecutionTransport,
  setCoderExecutionTransport,
} from "$lib/executionAuthority";
import { currentUndertakingGroupId } from "$lib/runtime/undertakingGroupPort";
import {
  openDaemonEventStream,
  type DaemonEventConnection,
} from "$lib/daemon/daemonEventStream";

export type ActiveUndertakingContext = {
  workId: string;
  /** Daemon identity that owns the undertaking and all of its execution. */
  executionRuntimeId: string | null;
  /** Tauri transport override; null means the current parent daemon. */
  executionTransportRuntimeId: string | null;
  /** Stable destination-authored repository identity. */
  repoId: string | null;
  title: string;
  humanPhase: HumanPhase | string;
  forgeState: string;
  worktree: string | null;
  baselineOid: string | null;
  sealedOid: string | null;
  leaseId: string | null;
  leaseGeneration: number | null;
  executorKind: string | null;
  attemptSeq: number | null;
  boundChatSessionIds: string[];
  boundTerminalSessionIds: string[];
  selectedEntityId: string | null;
  selectedPath: string | null;
  selectedLine: number | null;
  selectionStartLine: number | null;
  selectionEndLine: number | null;
  selectedText: string | null;
};

function groupKey(): string {
  return currentUndertakingGroupId();
}

function createUndertakingsStore() {
  let items = $state<ItemProjection[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let selectedId = $state<string | null>(null);
  let selectionRequest = 0;
  let detail = $state<ItemProjection | null>(null);
  let review = $state<ReviewProjection | null>(null);
  let selectedReviewAttemptId = $state<string | null>(null);
  /** Map shell group id → active context */
  let contexts = $state<Record<string, ActiveUndertakingContext | null>>({});
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let eventSource: DaemonEventConnection | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let eventStreamConnecting = false;
  let eventStreamGeneration = 0;
  let eventRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  let eventRevision = $state(0);
  const pendingEventWorkIds = new Set<string>();

  const active = $derived(contexts[groupKey()] ?? null);

  function sameContext(
    left: ActiveUndertakingContext | null | undefined,
    right: ActiveUndertakingContext,
  ): boolean {
    if (!left) return false;
    return (
      left.workId === right.workId &&
      left.executionRuntimeId === right.executionRuntimeId &&
      left.executionTransportRuntimeId === right.executionTransportRuntimeId &&
      left.repoId === right.repoId &&
      left.title === right.title &&
      left.humanPhase === right.humanPhase &&
      left.forgeState === right.forgeState &&
      left.worktree === right.worktree &&
      left.baselineOid === right.baselineOid &&
      left.sealedOid === right.sealedOid &&
      left.leaseId === right.leaseId &&
      left.leaseGeneration === right.leaseGeneration &&
      left.executorKind === right.executorKind &&
      left.attemptSeq === right.attemptSeq &&
      left.selectedEntityId === right.selectedEntityId &&
      left.selectedPath === right.selectedPath &&
      left.selectedLine === right.selectedLine &&
      left.selectionStartLine === right.selectionStartLine &&
      left.selectionEndLine === right.selectionEndLine &&
      left.selectedText === right.selectedText &&
      left.boundChatSessionIds.length === right.boundChatSessionIds.length &&
      left.boundChatSessionIds.every((id, index) => id === right.boundChatSessionIds[index]) &&
      left.boundTerminalSessionIds.length === right.boundTerminalSessionIds.length &&
      left.boundTerminalSessionIds.every((id, index) => id === right.boundTerminalSessionIds[index])
    );
  }

  function setActiveFromItem(item: ItemProjection, merge?: Partial<ActiveUndertakingContext>) {
    const attempt = item.attempts?.find((a) => a.id === item.active_attempt);
    const lease = attempt?.lease;
    const next: ActiveUndertakingContext = {
      workId: item.id,
      executionRuntimeId: null,
      executionTransportRuntimeId: null,
      repoId: null,
      title: item.title,
      humanPhase: item.human_phase,
      forgeState: item.state,
      worktree: item.environment?.worktree ?? null,
      baselineOid: item.environment?.baseline_oid ?? null,
      sealedOid: null,
      leaseId: lease?.lease_id ?? null,
      leaseGeneration: lease?.generation ?? null,
      executorKind: attempt?.executor?.kind ?? null,
      attemptSeq: attempt?.seq ?? null,
      boundChatSessionIds: [],
      boundTerminalSessionIds: [],
      selectedEntityId: null,
      selectedPath: null,
      selectedLine: null,
      selectionStartLine: null,
      selectionEndLine: null,
      selectedText: null,
      ...merge,
    };
    const prev = contexts[groupKey()];
    if (prev?.workId === next.workId) {
      if (!next.executionRuntimeId) next.executionRuntimeId = prev.executionRuntimeId;
      if (!next.executionTransportRuntimeId) {
        next.executionTransportRuntimeId = prev.executionTransportRuntimeId;
      }
      if (!next.repoId) next.repoId = prev.repoId;
      next.boundChatSessionIds = prev.boundChatSessionIds;
      next.boundTerminalSessionIds = prev.boundTerminalSessionIds;
      next.selectedEntityId = prev.selectedEntityId;
      next.selectedPath = prev.selectedPath;
      next.selectedLine = prev.selectedLine;
      next.selectionStartLine = prev.selectionStartLine;
      next.selectionEndLine = prev.selectionEndLine;
      next.selectedText = prev.selectedText;
      next.sealedOid = prev.sealedOid;
      // Only sticky-hold agent kind across a projection gap while a lease is
      // still live. After seal, active_attempt/lease clear and sticky must not
      // keep the Code editor locked in "agent owns this" forever.
      if (
        !next.executorKind &&
        next.leaseId &&
        (prev.executorKind === "codex" || prev.executorKind === "cursor")
      ) {
        next.executorKind = prev.executorKind;
      }
    }
    if (!sameContext(prev, next)) {
      contexts = { ...contexts, [groupKey()]: next };
    }
    setCoderExecutionTransport(next.executionTransportRuntimeId);
    void ensureEventStream();
  }

  function clearActive() {
    contexts = { ...contexts, [groupKey()]: null };
    setCoderExecutionTransport(null);
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

  function setSelection(selection: {
    entityId?: string | null;
    path?: string | null;
    line?: number | null;
    selectionStartLine?: number | null;
    selectionEndLine?: number | null;
    selectedText?: string | null;
  }) {
    const cur = contexts[groupKey()];
    if (!cur) return;
    const movedWithoutSelection =
      (selection.path !== undefined && selection.path !== cur.selectedPath) ||
      (selection.line !== undefined && selection.selectedText === undefined);
    const selectedEntityId =
      selection.entityId === undefined ? cur.selectedEntityId : selection.entityId;
    const selectedPath = selection.path === undefined ? cur.selectedPath : selection.path;
    const selectedLine = selection.line === undefined ? cur.selectedLine : selection.line;
    const selectionStartLine =
      selection.selectionStartLine === undefined
        ? movedWithoutSelection ? null : cur.selectionStartLine
        : selection.selectionStartLine;
    const selectionEndLine =
      selection.selectionEndLine === undefined
        ? movedWithoutSelection ? null : cur.selectionEndLine
        : selection.selectionEndLine;
    const selectedText =
      selection.selectedText === undefined
        ? movedWithoutSelection ? null : cur.selectedText
        : selection.selectedText;
    if (
      selectedEntityId === cur.selectedEntityId &&
      selectedPath === cur.selectedPath &&
      selectedLine === cur.selectedLine &&
      selectionStartLine === cur.selectionStartLine &&
      selectionEndLine === cur.selectionEndLine &&
      selectedText === cur.selectedText
    ) return;
    contexts = {
      ...contexts,
      [groupKey()]: {
        ...cur,
        selectedEntityId,
        selectedPath,
        selectedLine,
        selectionStartLine,
        selectionEndLine,
        selectedText,
      },
    };
  }

  async function refreshList(quiet = false) {
    if (!quiet) loading = true;
    error = null;
    try {
      const next = await listUndertakings();
      if (JSON.stringify(next) !== JSON.stringify(items)) items = next;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      if (!quiet) loading = false;
    }
  }

  async function select(
    workId: string,
    authority?: Pick<
      ActiveUndertakingContext,
      "executionRuntimeId" | "executionTransportRuntimeId" | "repoId"
    >,
  ) {
    const trimmed = workId.trim();
    if (!trimmed) {
      selectionRequest += 1;
      selectedId = null;
      detail = null;
      review = null;
      selectedReviewAttemptId = null;
      return;
    }
    const request = ++selectionRequest;
    if (selectedId !== trimmed) selectedReviewAttemptId = null;
    selectedId = trimmed;
    try {
      const nextDetail = await getUndertaking(trimmed);
      if (request !== selectionRequest) return;
      if (JSON.stringify(nextDetail) !== JSON.stringify(detail)) detail = nextDetail;
      setActiveFromItem(nextDetail, authority);
      if (
        nextDetail.human_phase === "review" ||
        nextDetail.state === "awaiting_review" ||
        nextDetail.state === "applying_decision"
      ) {
        const nextReview = await getReview(trimmed, selectedReviewAttemptId ?? undefined);
        if (request !== selectionRequest) return;
        if (JSON.stringify(nextReview) !== JSON.stringify(review)) review = nextReview;
        if (nextReview.sealed_head_oid) {
          const cur = contexts[groupKey()];
          if (cur) {
            contexts = {
              ...contexts,
              [groupKey()]: { ...cur, sealedOid: nextReview.sealed_head_oid },
            };
          }
        }
      } else {
        review = null;
      }
    } catch (err) {
      if (request === selectionRequest) {
        error = err instanceof Error ? err.message : String(err);
      }
    }
  }

  async function selectReviewAttempt(attemptId: string) {
    if (!selectedId) return;
    selectedReviewAttemptId = attemptId;
    const request = ++selectionRequest;
    const nextReview = await getReview(selectedId, attemptId);
    if (request === selectionRequest) review = nextReview;
  }

  async function create(input: {
    title: string;
    brief: string;
    repo_path: string;
    base_ref?: string;
    workspace_mode?: ForgeWorkspaceMode;
  }, authority?: Pick<
    ActiveUndertakingContext,
    "executionRuntimeId" | "executionTransportRuntimeId" | "repoId"
  >) {
    const item = await createUndertaking(input);
    await refreshList();
    await select(item.id, authority);
    return item;
  }

  async function start(input: {
    title: string;
    brief: string;
    repo_path: string;
    base_ref?: string;
    workspace_mode?: ForgeWorkspaceMode;
  }, authority?: Pick<
    ActiveUndertakingContext,
    "executionRuntimeId" | "executionTransportRuntimeId" | "repoId"
  >) {
    try {
      const item = await startUndertaking(input);
      await refreshList();
      selectedId = item.id;
      detail = item;
      setActiveFromItem(item, authority);
      return item;
    } catch (err) {
      await refreshList();
      throw err;
    }
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

  function startFallbackPolling() {
    if (pollTimer) return;
    pollTimer = setInterval(() => {
      if (selectedId) void refreshDetail();
      else void refreshList(true);
    }, 10_000);
  }

  function scheduleEventRefresh(workId: string) {
    if (workId) pendingEventWorkIds.add(workId);
    if (eventRefreshTimer) return;
    eventRefreshTimer = setTimeout(() => {
      eventRefreshTimer = null;
      const changed = new Set(pendingEventWorkIds);
      pendingEventWorkIds.clear();
      eventRevision += 1;
      void refreshList(true);
      const currentActive = contexts[groupKey()];
      const currentId = selectedId || currentActive?.workId || "";
      if (currentId && (changed.size === 0 || changed.has(currentId))) {
        void select(currentId);
      }
    }, 100);
  }

  async function ensureEventStream() {
    if (eventSource || eventStreamConnecting) return;
    const generation = eventStreamGeneration;
    eventStreamConnecting = true;
    let source: DaemonEventConnection | null = null;
    try {
      source = await openDaemonEventStream<{ work_id?: string }>({
        operation: "forge.stream.get",
        executionRuntimeId: getCoderExecutionTransport(),
        browserUrl: forgeStreamUrl,
        browserEvent: "forge",
        onOpen: stopPolling,
        onEvent: (event) => scheduleEventRefresh(event.work_id ?? ""),
        onError: () => {
          if (generation !== eventStreamGeneration) return;
          if (source && eventSource === source) eventSource = null;
          startFallbackPolling();
          if (reconnectTimer) clearTimeout(reconnectTimer);
          reconnectTimer = setTimeout(() => void ensureEventStream(), 5000);
        },
      });
      if (source.closed || generation !== eventStreamGeneration) {
        source.close();
        return;
      }
      eventSource = source;
    } catch {
      source?.close();
      if (generation !== eventStreamGeneration) return;
      startFallbackPolling();
      if (reconnectTimer) clearTimeout(reconnectTimer);
      reconnectTimer = setTimeout(() => void ensureEventStream(), 5000);
    } finally {
      if (generation === eventStreamGeneration) eventStreamConnecting = false;
    }
  }

  function startPolling() {
    void ensureEventStream();
  }

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  function resetForWorkshopSwitch() {
    eventStreamGeneration += 1;
    stopPolling();
    eventSource?.close();
    eventSource = null;
    eventStreamConnecting = false;
    if (reconnectTimer) clearTimeout(reconnectTimer);
    reconnectTimer = null;
    if (eventRefreshTimer) clearTimeout(eventRefreshTimer);
    eventRefreshTimer = null;
    pendingEventWorkIds.clear();
    eventRevision = 0;
    items = [];
    loading = false;
    error = null;
    selectedId = null;
    detail = null;
    review = null;
    selectedReviewAttemptId = null;
    contexts = {};
    setCoderExecutionTransport(null);
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
    /** Increments after coalesced Forge events so live change surfaces can refresh. */
    get eventRevision() {
      return eventRevision;
    },
    refreshList,
    select,
    create,
    start,
    provision,
    refreshDetail,
    selectReviewAttempt,
    clearActive,
    setActiveFromItem,
    bindChat,
    detachChat,
    bindTerminal,
    setSelection,
    startPolling,
    stopPolling,
    resetForWorkshopSwitch,
  };
}

export const undertakings = createUndertakingsStore();
