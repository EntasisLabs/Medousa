import type { ExternalAgentExecutor, UpdateBotRequest } from "$lib/types/generated/daemon_api";

function signature(executor: ExternalAgentExecutor | null | undefined): string {
  return JSON.stringify(executor ? [
    executor.runtime, executor.home_workshop_id, executor.forge_work_id,
    executor.forge_repo_id, executor.session_contract,
    executor.allowed_tools ?? [], executor.allowed_capabilities ?? [],
  ] : null);
}

/** Executor changes require administration; ordinary identity edits omit them. */
export function botExecutorUpdate(
  previous: ExternalAgentExecutor | null | undefined,
  next: ExternalAgentExecutor | null,
): Pick<UpdateBotRequest, "external_agent" | "clear_external_agent"> {
  if (signature(previous) === signature(next)) return {};
  return next ? { external_agent: next } : { clear_external_agent: true };
}
