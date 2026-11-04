/**
 * Thin renderer adapter for the Rust vault note buffer application core.
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  NoteBufferFact,
  VaultBufferCommand,
  VaultBufferSnapshot,
  VaultBufferUpdate,
} from "$lib/types/generated/app_api";
import type { NoteBuffer } from "$lib/stores/noteBuffer";

export type AppVaultBufferBackend = (
  command: VaultBufferCommand,
) => Promise<VaultBufferUpdate>;

const TEST_BACKEND_KEY = "__medousaAppVaultBufferTestBackend";

type GlobalBackendHost = typeof globalThis & {
  [TEST_BACKEND_KEY]?: AppVaultBufferBackend | null;
};

function readTestBackend(): AppVaultBufferBackend | null {
  return (globalThis as GlobalBackendHost)[TEST_BACKEND_KEY] ?? null;
}

export function installAppVaultBufferTestBackend(backend: AppVaultBufferBackend | null) {
  (globalThis as GlobalBackendHost)[TEST_BACKEND_KEY] = backend;
}

export async function dispatchAppVaultBuffer(
  command: VaultBufferCommand,
): Promise<VaultBufferUpdate> {
  const testBackend = readTestBackend();
  if (testBackend) return testBackend(command);
  return invoke<VaultBufferUpdate>("app_vault_buffer_dispatch", { command });
}

function expectSnapshot(update: VaultBufferUpdate): VaultBufferSnapshot {
  if (update.type !== "snapshot") {
    throw new Error(`expected vault buffer snapshot, got ${update.type}`);
  }
  return update.snapshot;
}

export function toNoteBufferFact(
  buffer: NoteBuffer,
  options?: { conflict?: boolean },
): NoteBufferFact {
  return {
    path: buffer.path,
    content: buffer.content,
    baselineContent: buffer.baselineContent,
    contentHash: buffer.contentHash,
    title: buffer.title,
    dirty: buffer.dirty,
    contentRevision: buffer.contentRevision,
    conflict: options?.conflict ?? false,
  };
}

export async function resetVaultBuffers(
  workshopScopeId: string,
): Promise<VaultBufferSnapshot> {
  return expectSnapshot(
    await dispatchAppVaultBuffer({
      type: "reset",
      workshop_scope_id: workshopScopeId,
    }),
  );
}

export async function seedVaultBuffer(buffer: NoteBuffer): Promise<VaultBufferSnapshot> {
  return expectSnapshot(
    await dispatchAppVaultBuffer({
      type: "seed_buffer",
      buffer: toNoteBufferFact(buffer),
    }),
  );
}

export async function applyVaultBufferEdit(options: {
  path: string;
  content: string;
  title?: string | null;
}): Promise<VaultBufferSnapshot> {
  return expectSnapshot(
    await dispatchAppVaultBuffer({
      type: "apply_edit",
      path: options.path,
      content: options.content,
      title: options.title ?? null,
    }),
  );
}

export async function markVaultBufferSaved(options: {
  path: string;
  content: string;
  contentHash?: string | null;
  title?: string | null;
}): Promise<VaultBufferSnapshot> {
  return expectSnapshot(
    await dispatchAppVaultBuffer({
      type: "mark_saved",
      path: options.path,
      content: options.content,
      content_hash: options.contentHash ?? null,
      title: options.title ?? null,
    }),
  );
}

export async function reconcileVaultBufferFromDisk(options: {
  path: string;
  content: string;
  contentHash?: string | null;
  title?: string | null;
}): Promise<VaultBufferSnapshot> {
  return expectSnapshot(
    await dispatchAppVaultBuffer({
      type: "reconcile_from_disk",
      path: options.path,
      content: options.content,
      content_hash: options.contentHash ?? null,
      title: options.title ?? null,
    }),
  );
}

export async function disposeVaultBuffer(path: string): Promise<VaultBufferSnapshot> {
  return expectSnapshot(
    await dispatchAppVaultBuffer({ type: "dispose_buffer", path }),
  );
}

export async function readVaultBufferSnapshot(
  path?: string | null,
): Promise<VaultBufferSnapshot> {
  return expectSnapshot(
    await dispatchAppVaultBuffer({
      type: "snapshot",
      path: path ?? null,
    }),
  );
}

export type {
  NoteBufferFact,
  VaultBufferCommand,
  VaultBufferSnapshot,
  VaultBufferUpdate,
};
