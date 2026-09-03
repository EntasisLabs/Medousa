/**
 * Code workspace orchestration boundary.
 *
 * The LME store owns presentation state (mode, tabs, and focus). This module
 * owns the cross-system transition required before code can load: selecting
 * the undertaking and landing its first source buffer.
 */

import { humanizeForgeMessage, type ForgeSourceTree } from "$lib/forge";
import { undertakings } from "$lib/stores/undertakings.svelte";
import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";
import {
  getUndertakingSourceTreeShared,
  invalidateUndertakingSourceTree,
} from "$lib/utils/forgeSourceTreeCache";
import { traceCodeWorkspaceEnd, traceCodeWorkspaceStart } from "$lib/utils/codeWorkspaceTrace";

const openingByWorkId = new Map<string, Promise<LandCodeResult>>();
const landingByWorkId = new Map<string, Promise<LandCodeResult>>();
const treeByWorkId = new Map<string, ForgeSourceTree>();
const treeRequestsByWorkId = new Map<string, Promise<ForgeSourceTree>>();

const LANDING_CANDIDATES = [
  "README.md", "README", "readme.md", "src/main.ts", "src/main.rs",
  "src/lib.rs", "src/index.ts", "src/index.js", "main.go", "Cargo.toml", "package.json",
];

export type LandCodeResult =
  | { ok: true; path: string }
  | { ok: false; error: string };

export function ensureCodeWorkspaceTree(
  workId: string,
  options?: { force?: boolean },
): Promise<ForgeSourceTree> {
  const id = workId.trim();
  if (!id) return Promise.reject(new Error("No project selected."));

  if (!options?.force) {
    const current = treeByWorkId.get(id);
    if (current) return Promise.resolve(current);
    const pending = treeRequestsByWorkId.get(id);
    if (pending) return pending;
  }

  if (options?.force) {
    treeByWorkId.delete(id);
    invalidateUndertakingSourceTree(id);
  }

  const trace = traceCodeWorkspaceStart("tree-fetch", id);
  const request = getUndertakingSourceTreeShared(id, { force: options?.force })
    .then((tree) => {
      treeByWorkId.set(id, tree);
      traceCodeWorkspaceEnd(trace, `${tree.files.length} files${tree.truncated ? ", truncated" : ""}`);
      return tree;
    })
    .catch((error) => {
      traceCodeWorkspaceEnd(trace, "failed");
      throw error;
    })
    .finally(() => {
      if (treeRequestsByWorkId.get(id) === request) {
        treeRequestsByWorkId.delete(id);
      }
    });
  treeRequestsByWorkId.set(id, request);
  return request;
}

export function invalidateCodeWorkspaceTree(workId: string) {
  const id = workId.trim();
  if (!id) return;
  treeByWorkId.delete(id);
  invalidateUndertakingSourceTree(id);
}

export function landCodeWorkingSet(workId: string): Promise<LandCodeResult> {
  const id = workId.trim();
  if (!id) return landCodeWorkingSetImpl(id);
  const existing = landingByWorkId.get(id);
  if (existing) return existing;
  const pending = landCodeWorkingSetImpl(id).finally(() => {
    if (landingByWorkId.get(id) === pending) landingByWorkId.delete(id);
  });
  landingByWorkId.set(id, pending);
  return pending;
}

async function landCodeWorkingSetImpl(workId: string): Promise<LandCodeResult> {
  const id = workId.trim();
  if (!id) return { ok: false, error: "No project selected." };
  let detail = undertakings.detail?.id === id ? undertakings.detail : null;
  if (detail && !detail.environment && detail.allowed_actions.provision.allowed) {
    try {
      await undertakings.provision(id);
      detail = undertakings.detail?.id === id ? undertakings.detail : null;
    } catch (err) {
      const message = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
      codeWorkspace.workspaceErrorByWorkId = { ...codeWorkspace.workspaceErrorByWorkId, [id]: message };
      return { ok: false, error: message };
    }
  }
  if (detail && !detail.environment) {
    const message = detail.allowed_actions.provision.allowed
      ? "Set up this project to open its working copy and files."
      : detail.allowed_actions.provision.reason || "This project has no working copy yet.";
    codeWorkspace.workspaceErrorByWorkId = { ...codeWorkspace.workspaceErrorByWorkId, [id]: message };
    return { ok: false, error: message };
  }
  try {
    await codeWorkspace.hydrate(id);
    const existing = codeWorkspace.activeFor(id);
    if (existing && !existing.loading && existing.digest) {
      undertakings.setSelection({ path: existing.path, line: existing.line ?? 1, entityId: null });
      codeWorkspace.workspaceErrorByWorkId = { ...codeWorkspace.workspaceErrorByWorkId, [id]: null };
      return { ok: true, path: existing.path };
    }
    const tree = await ensureCodeWorkspaceTree(id);
    const paths = tree.files.map((file) => file.path);
    if (paths.length === 0) {
      const message = "This working copy has no files to open yet.";
      codeWorkspace.workspaceErrorByWorkId = { ...codeWorkspace.workspaceErrorByWorkId, [id]: message };
      return { ok: false, error: message };
    }
    const preferred = LANDING_CANDIDATES.find((candidate) => paths.includes(candidate)) ??
      paths.find((path) => /\.(ts|tsx|js|jsx|rs|go|py|svelte|md)$/i.test(path)) ?? paths[0];
    if (!preferred) return { ok: false, error: "Could not pick a landing file in this working copy." };
    await codeWorkspace.open(id, preferred, 1);
    undertakings.setSelection({ path: preferred, line: 1, entityId: null });
    codeWorkspace.workspaceErrorByWorkId = { ...codeWorkspace.workspaceErrorByWorkId, [id]: null };
    return { ok: true, path: preferred };
  } catch (err) {
    const message = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    codeWorkspace.workspaceErrorByWorkId = { ...codeWorkspace.workspaceErrorByWorkId, [id]: message };
    return { ok: false, error: message };
  }
}

export function openCodeWorkspaceSession(workId: string): Promise<LandCodeResult> {
  const id = workId.trim();
  if (!id) return Promise.resolve({ ok: false, error: "No project selected." });

  const existing = openingByWorkId.get(id);
  if (existing) return existing;

  const trace = traceCodeWorkspaceStart("open", id);
  const pending = (async () => {
    const selectTrace = traceCodeWorkspaceStart("select", id);
    if (undertakings.detail?.id !== id) {
      await undertakings.select(id);
    }
    traceCodeWorkspaceEnd(selectTrace);
    const result = await landCodeWorkingSet(id);
    traceCodeWorkspaceEnd(trace, result.ok ? `opened ${result.path}` : "failed");
    return result;
  })().catch((error) => {
    traceCodeWorkspaceEnd(trace, "failed");
    throw error;
  }).finally(() => {
    if (openingByWorkId.get(id) === pending) openingByWorkId.delete(id);
  });

  openingByWorkId.set(id, pending);
  return pending;
}
