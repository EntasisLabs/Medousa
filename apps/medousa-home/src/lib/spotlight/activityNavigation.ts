import type { WorkshopCommandContext } from "$lib/commands/types";
import type { ActivityTarget } from "./activity";
import { activeWorkshopId } from "$lib/utils/workshopLocality";
import { openTaskDetails } from "$lib/runtime/shellUseCases";

/** Shared by the inline Activity lens and desktop-to-shell navigation. */
export async function openActivityTarget(target: ActivityTarget, title: string, ctx: WorkshopCommandContext, alive: () => boolean = () => true): Promise<boolean> {
  const scope = activeWorkshopId();
  const current = () => alive() && activeWorkshopId() === scope;
  if (target.kind === "chat" || (target.kind === "card" && target.sessionId)) {
    await ctx.chat.switchSession(target.kind === "chat" ? target.sessionId : target.sessionId!);
    if (current()) { ctx.navigate("chat"); ctx.callbacks.close(); }
  } else if (target.kind === "card" || target.kind === "job") {
    await openTaskDetails(target.kind === "card" ? target.cardId : target.jobId);
  } else if (target.kind === "project") {
    const { setCoderExecutionTransport } = await import("$lib/executionAuthority");
    const { lmeWorkspace } = await import("$lib/stores/lmeWorkspace.svelte");
    if (!current()) return true;
    setCoderExecutionTransport(null);
    if (target.review) await lmeWorkspace.openCodeReview(target.workId, title);
    else await lmeWorkspace.openCodeWorkspace(target.workId, title);
    if (current()) ctx.callbacks.close();
  } else if (target.kind === "note") {
    const { lmeWorkspace } = await import("$lib/stores/lmeWorkspace.svelte");
    if (!current()) return true;
    await lmeWorkspace.openNote(target.path);
    if (current()) ctx.callbacks.close();
  } else return false;
  return true;
}
