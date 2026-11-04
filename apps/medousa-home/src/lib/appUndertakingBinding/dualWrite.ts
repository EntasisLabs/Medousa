/**
 * UndertakingBindingCore commit helpers: await the mutation, then paint the
 * returned snapshot onto the host contexts map. Selection chrome is preserved
 * by paintUndertakingContextsFromSnapshot.
 */

import type {
  UndertakingBindingSnapshot,
  UndertakingContextFact,
} from "$lib/types/generated/app_api";
import {
  bindUndertakingChat,
  bindUndertakingTerminal,
  clearActiveUndertakingBinding,
  detachUndertakingChat,
  resetUndertakingBindings,
  setActiveUndertakingBinding,
} from "./client";

export type UndertakingBindingSnapshotPainter = (
  snapshot: UndertakingBindingSnapshot,
) => void;

function commitUndertaking(
  paint: UndertakingBindingSnapshotPainter,
  run: () => Promise<UndertakingBindingSnapshot>,
) {
  void run()
    .then((snapshot) => paint(snapshot))
    .catch(() => {
      /* Core may be unavailable outside Tauri / without the stdio backend. */
    });
}

export function commitSetActiveUndertaking(
  paint: UndertakingBindingSnapshotPainter,
  options: { groupId: string; context: UndertakingContextFact },
) {
  commitUndertaking(paint, () => setActiveUndertakingBinding(options));
}

export function commitClearActiveUndertaking(
  paint: UndertakingBindingSnapshotPainter,
  groupId: string,
) {
  commitUndertaking(paint, () => clearActiveUndertakingBinding(groupId));
}

export function commitBindUndertakingChat(
  paint: UndertakingBindingSnapshotPainter,
  options: { sessionId: string; groupId: string },
) {
  commitUndertaking(paint, () => bindUndertakingChat(options));
}

export function commitDetachUndertakingChat(
  paint: UndertakingBindingSnapshotPainter,
  sessionId: string,
) {
  commitUndertaking(paint, () => detachUndertakingChat(sessionId));
}

export function commitBindUndertakingTerminal(
  paint: UndertakingBindingSnapshotPainter,
  options: { sessionId: string; groupId: string },
) {
  commitUndertaking(paint, () => bindUndertakingTerminal(options));
}

export function commitResetUndertakingBindings(
  paint: UndertakingBindingSnapshotPainter,
  workshopScopeId: string,
) {
  commitUndertaking(paint, () => resetUndertakingBindings(workshopScopeId));
}
