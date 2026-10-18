import { terminalCreate } from "$lib/terminal";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { layout } from "$lib/runtime/layout.svelte";
import { activeWorkshopId } from "$lib/utils/workshopLocality";
import { sshAction, type SshTarget } from "$lib/utils/sshApi";

function requireWorkshop(scope: string) {
  if (scope !== activeWorkshopId()) throw new Error("The connected workshop changed. Open the terminal from the current workshop.");
}

/** Human terminals follow Connection, independently of Code's selected worker. */
export async function openWorkshopTerminal(scope = activeWorkshopId()): Promise<void> {
  requireWorkshop(scope);
  const created = await terminalCreate({ work_id: null, cwd: null }, null);
  requireWorkshop(scope);
  const sessionId = created.session_id?.trim();
  if (!sessionId) throw new Error("The workshop did not return a terminal session.");
  layout.navigateDesktop("chat");
  shellTabs.openTerminal(sessionId, { title: "Terminal", executionRuntimeId: null });
}

export async function openSshTerminal(target: SshTarget, scope = activeWorkshopId()): Promise<void> {
  requireWorkshop(scope);
  const receipt = await sshAction<{ session_id: string | null; error: string | null }>("terminal", {
    target_id: target.target_id, request_key: crypto.randomUUID(),
  });
  requireWorkshop(scope);
  if (!receipt.session_id?.trim()) throw new Error(receipt.error || "SSH terminal could not start");
  layout.navigateDesktop("chat");
  shellTabs.openTerminal(receipt.session_id, { title: `SSH · ${target.name}`, executionRuntimeId: null });
}
