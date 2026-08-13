/** Mobile Code open/enter helpers that must not activate desktop shell tabs. */

import { chat } from "$lib/stores/chat.svelte";
import { setSessionCodeBinding } from "$lib/daemon";
import { switchMobileTab } from "$lib/mobileNavigation";
import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";
import { mobileCodeWorkspaceState } from "$lib/stores/mobileCodeWorkspaceState.svelte";
import { undertakings } from "$lib/stores/undertakings.svelte";
import { ensureCodeWorkspaceTree } from "$lib/utils/codeWorkspaceController";
import {
  projectHasAttention,
  resolveMobileCodeLanding,
  type MobileCodeSurface,
} from "$lib/utils/mobileCodeLanding";

export async function enterMobileCodeProject(workId: string): Promise<void> {
  const id = workId.trim();
  if (!id) return;
  if (undertakings.detail?.id !== id) {
    await undertakings.select(id);
  }
  await codeWorkspace.hydrate(id);

  const detail = undertakings.detail?.id === id ? undertakings.detail : null;
  const tabs = codeWorkspace.tabsFor(id);
  const dirtyBuffers = tabs.some((tab) => codeWorkspace.isDirty(tab));
  let dirtyWorkingCopy = false;
  try {
    const tree = await ensureCodeWorkspaceTree(id);
    dirtyWorkingCopy = tree.files.some((file) => Boolean(file.status));
  } catch {
    // Tree may be unavailable before provision; landing still works from buffers.
  }

  const landing: MobileCodeSurface = resolveMobileCodeLanding({
    hasAttention: projectHasAttention({
      humanPhase: detail?.human_phase,
      forgeState: detail?.state,
      dirtyWorkingCopy,
      dirtyBuffers,
    }),
    hasOpenFile: Boolean(codeWorkspace.activeFor(id)?.path),
  });
  mobileCodeWorkspaceState.enterProject(id, landing);
}

export async function openMobileCodeFile(
  workId: string,
  path: string,
  options?: { line?: number | null; origin?: "files" | "changes" | "terminal" },
): Promise<void> {
  const id = workId.trim();
  const normalized = path.trim().replaceAll("\\", "/").replace(/^\.\//, "");
  if (
    !id ||
    !normalized ||
    normalized.startsWith("/") ||
    /^[a-z]:\//i.test(normalized) ||
    normalized.split("/").includes("..")
  ) {
    throw new Error("Invalid undertaking location");
  }
  if (undertakings.detail?.id !== id) {
    await undertakings.select(id);
  }
  await codeWorkspace.hydrate(id);
  await codeWorkspace.open(id, normalized, options?.line ?? 1);
  undertakings.setSelection({
    path: normalized,
    line: options?.line && options.line > 0 ? Math.floor(options.line) : 1,
    entityId: null,
  });
  const origin = options?.origin ?? "files";
  if (!mobileCodeWorkspaceState.selectedWorkId) {
    mobileCodeWorkspaceState.enterProject(id, "editor");
  }
  mobileCodeWorkspaceState.jumpToEditor(origin, normalized);
}

export async function openMobileCodeThread(): Promise<void> {
  const workId = mobileCodeWorkspaceState.selectedWorkId;
  if (!workId) return;
  if (undertakings.detail?.id !== workId) {
    await undertakings.select(workId);
  }
  const bound = undertakings.active?.workId === workId
    ? undertakings.active.boundChatSessionIds[0]
    : null;
  if (bound) {
    await chat.switchSession(bound);
  } else {
    await chat.newSession();
    const sessionId = chat.sessionId;
    if (!sessionId) return;
    const item = undertakings.detail?.id === workId
      ? undertakings.detail
      : undertakings.items.find((row) => row.id === workId);
    if (item) undertakings.setActiveFromItem(item);
    undertakings.bindChat(sessionId);
    try {
      await setSessionCodeBinding(sessionId, workId);
    } catch {
      // Binding is best-effort; the Chat surface still opens.
    }
  }
  switchMobileTab("chat");
}
