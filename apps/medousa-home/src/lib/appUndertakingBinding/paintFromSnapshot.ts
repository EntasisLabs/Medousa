/**
 * Convert UndertakingBindingCore facts ↔ Home ActiveUndertakingContext.
 * Selection chrome (path/line/text) stays renderer-local and is preserved on paint.
 */

import type {
  UndertakingBindingSnapshot,
  UndertakingContextFact,
} from "$lib/types/generated/app_api";
import type { ActiveUndertakingContext } from "$lib/stores/undertakings.svelte";

const EMPTY_SELECTION = {
  selectedEntityId: null as string | null,
  selectedPath: null as string | null,
  selectedLine: null as number | null,
  selectionStartLine: null as number | null,
  selectionEndLine: null as number | null,
  selectedText: null as string | null,
};

export function toUndertakingContextFact(
  context: ActiveUndertakingContext,
): UndertakingContextFact {
  return {
    workId: context.workId,
    executionRuntimeId: context.executionRuntimeId,
    executionTransportRuntimeId: context.executionTransportRuntimeId,
    repoId: context.repoId,
    title: context.title,
    humanPhase: String(context.humanPhase),
    forgeState: context.forgeState,
    worktree: context.worktree,
    baselineOid: context.baselineOid,
    sealedOid: context.sealedOid,
    leaseId: context.leaseId,
    leaseGeneration: context.leaseGeneration,
    executorKind: context.executorKind,
    attemptSeq: context.attemptSeq,
    boundChatSessionIds: [...context.boundChatSessionIds],
    boundTerminalSessionIds: [...context.boundTerminalSessionIds],
  };
}

export function contextFromFact(
  fact: UndertakingContextFact,
  previous?: ActiveUndertakingContext | null,
): ActiveUndertakingContext {
  return {
    workId: fact.workId,
    executionRuntimeId: fact.executionRuntimeId ?? null,
    executionTransportRuntimeId: fact.executionTransportRuntimeId ?? null,
    repoId: fact.repoId ?? null,
    title: fact.title,
    humanPhase: fact.humanPhase,
    forgeState: fact.forgeState,
    worktree: fact.worktree ?? null,
    baselineOid: fact.baselineOid ?? null,
    sealedOid: fact.sealedOid ?? null,
    leaseId: fact.leaseId ?? null,
    leaseGeneration: fact.leaseGeneration ?? null,
    executorKind: fact.executorKind ?? null,
    attemptSeq: fact.attemptSeq ?? null,
    boundChatSessionIds: [...(fact.boundChatSessionIds ?? [])],
    boundTerminalSessionIds: [...(fact.boundTerminalSessionIds ?? [])],
    selectedEntityId: previous?.selectedEntityId ?? EMPTY_SELECTION.selectedEntityId,
    selectedPath: previous?.selectedPath ?? EMPTY_SELECTION.selectedPath,
    selectedLine: previous?.selectedLine ?? EMPTY_SELECTION.selectedLine,
    selectionStartLine: previous?.selectionStartLine ?? EMPTY_SELECTION.selectionStartLine,
    selectionEndLine: previous?.selectionEndLine ?? EMPTY_SELECTION.selectionEndLine,
    selectedText: previous?.selectedText ?? EMPTY_SELECTION.selectedText,
  };
}

/**
 * Paint the host group→context map from a core snapshot.
 * Groups present in `previous` but absent from the snapshot keep only when
 * `preserveMissing` is true (default false — snapshot is full authority).
 */
export function paintUndertakingContextsFromSnapshot(
  snapshot: UndertakingBindingSnapshot,
  previous: Readonly<Record<string, ActiveUndertakingContext | null>> = {},
): Record<string, ActiveUndertakingContext | null> {
  const next: Record<string, ActiveUndertakingContext | null> = {};
  for (const group of snapshot.contexts ?? []) {
    const groupId = group.groupId?.trim();
    if (!groupId) continue;
    const fact = group.context ?? null;
    next[groupId] = fact ? contextFromFact(fact, previous[groupId]) : null;
  }
  return next;
}
