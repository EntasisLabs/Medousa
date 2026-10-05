/** Derived request identity, never a substitute for Forge execution authority. */
import { getCoderExecutionTransport } from "$lib/executionAuthority";
import { activeWorkshopId } from "$lib/utils/workshopLocality";
import type { ForgeEnvironment } from "$lib/forgeWorkspace";

let workshopEpoch = $state(0);

/** Also invalidates work from an earlier visit to the same workshop. */
export function invalidateCodeWorkshopContext() {
  workshopEpoch += 1;
}

export function codeExecutionScopeKey(): string {
  return JSON.stringify([activeWorkshopId(), getCoderExecutionTransport(), workshopEpoch]);
}

export function codeWorkspaceScopeKey(input: {
  workId: string;
  workspaceRoot: string | null;
  environment?: ForgeEnvironment | null;
}): string {
  const environment = input.environment;
  return JSON.stringify([
    codeExecutionScopeKey(), input.workId, input.workspaceRoot,
    environment?.kind ?? null, environment?.generation ?? null,
    environment?.branch ?? null, environment?.baseline_oid ?? null,
    environment?.attached_index_oid ?? null,
  ]);
}

/** Captures an operation's scope and checks it again after asynchronous work. */
export function captureCodeScope(getScopeKey: () => string): () => boolean {
  const key = getScopeKey();
  return () => getScopeKey() === key;
}
