/**
 * Paint NoteBuffer host maps from a VaultBufferCore snapshot.
 * Daemon remains filesystem authority; CodeMirror paint stays in the renderer.
 */

import type { NoteBufferFact, VaultBufferSnapshot } from "$lib/types/generated/app_api";
import type { NoteBuffer } from "$lib/stores/noteBuffer";

export function noteBufferFromFact(fact: NoteBufferFact): NoteBuffer {
  return {
    path: fact.path,
    content: fact.content,
    baselineContent: fact.baselineContent,
    contentHash: fact.contentHash ?? null,
    title: fact.title,
    dirty: fact.dirty,
    contentRevision: fact.contentRevision,
  };
}

export type VaultBufferPaintTarget = {
  buffers: Map<string, NoteBuffer>;
  bumpNoteBuffers(): void;
  /** When set, sync focused editor fields from the matching core buffer. */
  syncFocusedFromBuffer?(buffer: NoteBuffer): void;
  focusedPath?: string | null;
};

export function paintVaultBuffersFromSnapshot(
  host: VaultBufferPaintTarget,
  snapshot: VaultBufferSnapshot,
) {
  const next = new Map<string, NoteBuffer>();
  for (const fact of snapshot.buffers ?? []) {
    const buffer = noteBufferFromFact(fact);
    next.set(buffer.path, buffer);
  }
  host.buffers.clear();
  for (const [path, buffer] of next) {
    host.buffers.set(path, buffer);
  }
  host.bumpNoteBuffers();

  const focused = host.focusedPath?.trim();
  if (!focused || !host.syncFocusedFromBuffer) return;
  const buffer = next.get(focused);
  if (buffer) host.syncFocusedFromBuffer(buffer);
}
