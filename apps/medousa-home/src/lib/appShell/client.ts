/**
 * Thin renderer adapter for the Rust application shell layout core.
 *
 * Production path: Tauri host (`app_shell_dispatch`).
 * Tests may install an in-process backend.
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  ShellCommand,
  ShellDesktop,
  ShellSnapshot,
  ShellTab,
  ShellUpdate,
  SplitEdge,
} from "$lib/types/generated/app_api";

export type AppShellBackend = (command: ShellCommand) => Promise<ShellUpdate>;

const TEST_BACKEND_KEY = "__medousaAppShellTestBackend";

type GlobalBackendHost = typeof globalThis & {
  [TEST_BACKEND_KEY]?: AppShellBackend | null;
};

function readTestBackend(): AppShellBackend | null {
  return (globalThis as GlobalBackendHost)[TEST_BACKEND_KEY] ?? null;
}

export function installAppShellTestBackend(backend: AppShellBackend | null) {
  (globalThis as GlobalBackendHost)[TEST_BACKEND_KEY] = backend;
}

export async function dispatchAppShell(command: ShellCommand): Promise<ShellUpdate> {
  const testBackend = readTestBackend();
  if (testBackend) return testBackend(command);
  return invoke<ShellUpdate>("app_shell_dispatch", { command });
}

function expectSnapshot(update: ShellUpdate): ShellSnapshot {
  if (update.type !== "snapshot") {
    throw new Error(`expected shell snapshot, got ${update.type}`);
  }
  return update.snapshot;
}

/** Fire-and-forget dispatch — prefer commit* helpers that paint from snapshots. */
export function dualWriteShell(command: ShellCommand): void {
  void dispatchAppShell(command).catch(() => {
    /* Core may be unavailable in pure-DOM vitest without the stdio backend. */
  });
}

export async function hydrateShellLayout(options: {
  workspaceScopeId: string;
  desktops: ShellDesktop[];
  activeDesktopId: string;
}): Promise<ShellSnapshot> {
  return expectSnapshot(
    await dispatchAppShell({
      type: "hydrate",
      workspace_scope_id: options.workspaceScopeId,
      desktops: options.desktops,
      active_desktop_id: options.activeDesktopId,
    }),
  );
}

export async function resetShellLayout(workspaceScopeId: string): Promise<ShellSnapshot> {
  return expectSnapshot(
    await dispatchAppShell({
      type: "reset",
      workspace_scope_id: workspaceScopeId,
    }),
  );
}

export async function openShellTab(options: {
  tab: ShellTab;
  groupId?: string | null;
  activate?: boolean;
}): Promise<ShellSnapshot> {
  return expectSnapshot(
    await dispatchAppShell({
      type: "open_tab",
      tab: options.tab,
      group_id: options.groupId ?? null,
      activate: options.activate ?? true,
    }),
  );
}

export async function closeShellTab(tabId: string): Promise<ShellSnapshot> {
  return expectSnapshot(
    await dispatchAppShell({ type: "close_tab", tab_id: tabId }),
  );
}

export async function focusShellTab(tabId: string): Promise<ShellSnapshot> {
  return expectSnapshot(
    await dispatchAppShell({ type: "focus_tab", tab_id: tabId }),
  );
}

export async function focusShellGroup(groupId: string): Promise<ShellSnapshot> {
  return expectSnapshot(
    await dispatchAppShell({ type: "focus_group", group_id: groupId }),
  );
}

export async function splitShellPane(options: {
  groupId: string;
  edge: SplitEdge;
  newGroupId: string;
  moveTabId?: string | null;
}): Promise<ShellSnapshot> {
  return expectSnapshot(
    await dispatchAppShell({
      type: "split_pane",
      group_id: options.groupId,
      edge: options.edge,
      new_group_id: options.newGroupId,
      move_tab_id: options.moveTabId ?? null,
    }),
  );
}

export async function switchShellDesktop(desktopId: string): Promise<ShellSnapshot> {
  return expectSnapshot(
    await dispatchAppShell({ type: "switch_desktop", desktop_id: desktopId }),
  );
}

export async function readShellSnapshot(): Promise<ShellSnapshot> {
  return expectSnapshot(await dispatchAppShell({ type: "snapshot" }));
}

export type { ShellCommand, ShellDesktop, ShellSnapshot, ShellTab, ShellUpdate, SplitEdge };
