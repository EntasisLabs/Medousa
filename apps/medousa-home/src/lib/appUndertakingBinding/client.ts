/**
 * Thin renderer adapter for the Rust undertaking binding application core.
 *
 * Production path: Tauri host (`app_undertaking_binding_dispatch`).
 * Tests may install an in-process backend.
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  UndertakingBindingCommand,
  UndertakingBindingSnapshot,
  UndertakingBindingUpdate,
  UndertakingContextFact,
} from "$lib/types/generated/app_api";

export type AppUndertakingBindingBackend = (
  command: UndertakingBindingCommand,
) => Promise<UndertakingBindingUpdate>;

const TEST_BACKEND_KEY = "__medousaAppUndertakingBindingTestBackend";

type GlobalBackendHost = typeof globalThis & {
  [TEST_BACKEND_KEY]?: AppUndertakingBindingBackend | null;
};

function readTestBackend(): AppUndertakingBindingBackend | null {
  return (globalThis as GlobalBackendHost)[TEST_BACKEND_KEY] ?? null;
}

export function installAppUndertakingBindingTestBackend(
  backend: AppUndertakingBindingBackend | null,
) {
  (globalThis as GlobalBackendHost)[TEST_BACKEND_KEY] = backend;
}

export async function dispatchAppUndertakingBinding(
  command: UndertakingBindingCommand,
): Promise<UndertakingBindingUpdate> {
  const testBackend = readTestBackend();
  if (testBackend) return testBackend(command);
  return invoke<UndertakingBindingUpdate>("app_undertaking_binding_dispatch", {
    command,
  });
}

function expectSnapshot(update: UndertakingBindingUpdate): UndertakingBindingSnapshot {
  if (update.type !== "snapshot") {
    throw new Error(`expected undertaking binding snapshot, got ${update.type}`);
  }
  return update.snapshot;
}

/**
 * Fire-and-forget dispatch. Prefer commit* helpers in `./dualWrite` that paint
 * from the returned snapshot — core owns the binding map after paint.
 */
export function dualWriteUndertakingBinding(command: UndertakingBindingCommand): void {
  void dispatchAppUndertakingBinding(command).catch(() => {
    /* Core may be unavailable outside Tauri / without the stdio backend. */
  });
}

export async function resetUndertakingBindings(
  workshopScopeId: string,
): Promise<UndertakingBindingSnapshot> {
  return expectSnapshot(
    await dispatchAppUndertakingBinding({
      type: "reset",
      workshop_scope_id: workshopScopeId,
    }),
  );
}

export async function setActiveUndertakingBinding(options: {
  groupId: string;
  context: UndertakingContextFact;
}): Promise<UndertakingBindingSnapshot> {
  return expectSnapshot(
    await dispatchAppUndertakingBinding({
      type: "set_active",
      group_id: options.groupId,
      context: options.context,
    }),
  );
}

export async function clearActiveUndertakingBinding(
  groupId: string,
): Promise<UndertakingBindingSnapshot> {
  return expectSnapshot(
    await dispatchAppUndertakingBinding({
      type: "clear_active",
      group_id: groupId,
    }),
  );
}

export async function bindUndertakingChat(options: {
  sessionId: string;
  groupId: string;
}): Promise<UndertakingBindingSnapshot> {
  return expectSnapshot(
    await dispatchAppUndertakingBinding({
      type: "bind_chat",
      session_id: options.sessionId,
      group_id: options.groupId,
    }),
  );
}

export async function detachUndertakingChat(
  sessionId: string,
): Promise<UndertakingBindingSnapshot> {
  return expectSnapshot(
    await dispatchAppUndertakingBinding({
      type: "detach_chat",
      session_id: sessionId,
    }),
  );
}

export async function bindUndertakingTerminal(options: {
  sessionId: string;
  groupId: string;
}): Promise<UndertakingBindingSnapshot> {
  return expectSnapshot(
    await dispatchAppUndertakingBinding({
      type: "bind_terminal",
      session_id: options.sessionId,
      group_id: options.groupId,
    }),
  );
}

export async function readUndertakingBindingSnapshot(): Promise<UndertakingBindingSnapshot> {
  return expectSnapshot(await dispatchAppUndertakingBinding({ type: "snapshot" }));
}

export type {
  UndertakingBindingCommand,
  UndertakingBindingSnapshot,
  UndertakingBindingUpdate,
  UndertakingContextFact,
};
