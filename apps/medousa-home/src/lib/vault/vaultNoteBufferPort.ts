/** Injected note buffers — published by the vault store, read by features. */

import type { NoteBuffer } from "$lib/stores/noteBuffer";

let peek: ((path: string) => NoteBuffer | undefined) | null = null;

export function setVaultNoteBufferPort(
  port: ((path: string) => NoteBuffer | undefined) | null,
): void {
  peek = port;
}

export function getVaultNoteBuffer(path: string): NoteBuffer | undefined {
  return peek?.(path);
}
