/**
 * Code-map (world) load, index, find, and impact for an undertaking.
 * UndertakingWorldPanel owns UI state; this module talks to
 * undertakingCommandController (not $lib/forge).
 */

import {
  getWorldAtLocation,
  getWorldBinding,
  getWorldCodeAvec,
  getWorldFiles,
  getWorldFind,
  getWorldImpact,
  queueWorldIndex,
  type WorldAvecResult,
  type WorldBindingStatus,
  type WorldFilesResult,
  type WorldFindResult,
  type WorldImpactResult,
  type WorldSnapshotRef,
} from "$lib/code/undertakingCommandController";
import { undertakings } from "$lib/stores/undertakings.svelte";
import { openUndertakingLocation } from "$lib/utils/undertakingLocation";

export type WorldSnapshotKind = "baseline" | "sealed";

export type WorldLocationIntent = {
  path: string;
  line?: number | null;
  entityId?: string | null;
};

export type WorldOverview = {
  binding: WorldBindingStatus | null;
  files: WorldFilesResult | null;
  insight: WorldAvecResult | null;
  error: string | null;
  /** True when the selected snapshot is not ready; callers should drop find/impact. */
  resetSearch: boolean;
};

export type {
  WorldAvecResult,
  WorldBindingStatus,
  WorldFilesResult,
  WorldFindResult,
  WorldImpactResult,
  WorldSnapshotRef,
};

export function selectedWorldSnapshot(
  binding: WorldBindingStatus | null,
  kind: WorldSnapshotKind,
): WorldSnapshotRef | null {
  return binding?.[kind] ?? null;
}

export function worldSlotState(
  binding: WorldBindingStatus | null,
  kind: WorldSnapshotKind,
): string {
  return (binding?.[kind]?.state ?? "").toLowerCase();
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export async function loadWorldOverview(
  workId: string,
  kind: WorldSnapshotKind,
): Promise<WorldOverview> {
  try {
    const binding = await getWorldBinding(workId);
    const slot = binding?.[kind];
    const state = (slot?.state ?? "").toLowerCase();
    if (state && state !== "ready") {
      return {
        binding,
        files: null,
        insight: null,
        error:
          state === "failed"
            ? slot?.error?.trim() || "Code map indexing failed."
            : null,
        resetSearch: true,
      };
    }
    const snapshot = selectedWorldSnapshot(binding, kind);
    try {
      const files = await getWorldFiles(workId, undefined, snapshot);
      const insight = await getWorldCodeAvec(workId, snapshot);
      return { binding, files, insight, error: null, resetSearch: false };
    } catch (err) {
      return {
        binding,
        files: null,
        insight: null,
        error: errorMessage(err),
        resetSearch: false,
      };
    }
  } catch (err) {
    return {
      binding: null,
      files: null,
      insight: null,
      error: errorMessage(err),
      resetSearch: false,
    };
  }
}

export async function rebuildWorldMap(
  workId: string,
  kind: WorldSnapshotKind,
): Promise<WorldOverview> {
  await queueWorldIndex(workId, kind);
  return loadWorldOverview(workId, kind);
}

export async function findWorldEntities(
  workId: string,
  nameContains: string,
  snapshot: WorldSnapshotRef | null,
): Promise<WorldFindResult> {
  return getWorldFind(workId, {
    name_contains: nameContains.trim() || undefined,
    snapshot,
  });
}

export async function loadWorldImpact(
  workId: string,
  entityId: string,
  snapshot: WorldSnapshotRef | null,
): Promise<WorldImpactResult> {
  return getWorldImpact(workId, entityId.trim(), snapshot);
}

export async function revealWorldLocation(
  workId: string,
  input: WorldLocationIntent,
  snapshot: WorldSnapshotRef | null,
): Promise<{ entityId: string; impact?: WorldImpactResult } | null> {
  await openUndertakingLocation({ workId, ...input });
  if (input.entityId) {
    const impact = await getWorldImpact(workId, input.entityId, snapshot);
    return { entityId: input.entityId, impact };
  }
  if (input.line) {
    const located = await getWorldAtLocation(
      workId,
      input.path,
      input.line,
      snapshot,
    );
    const entity = located.entity;
    if (entity) {
      undertakings.setSelection({ entityId: entity.id });
      return { entityId: entity.id };
    }
  }
  return null;
}
