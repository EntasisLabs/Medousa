import type { ChatAgentRuntime } from "$lib/utils/sessionAgentRuntime";
import type { AgentModeId } from "$lib/types/session";

export type AgentWorkspaceAction = "keep" | "start" | "restart" | "stop" | "wait_for_project";

export function planAgentWorkspace(input: {
  runtime: ChatAgentRuntime;
  mode: AgentModeId;
  bindingWorkId: string | null;
  agentSessionId: string | null;
  agentWorkId: string | null | undefined;
}): AgentWorkspaceAction {
  if (input.runtime === "medousa") return input.agentSessionId ? "stop" : "keep";

  if (input.mode === "coder" && !input.bindingWorkId) {
    return input.agentSessionId ? "stop" : "wait_for_project";
  }

  if (!input.agentSessionId) return "start";
  if (input.agentWorkId === input.bindingWorkId) return "keep";
  return "restart";
}
