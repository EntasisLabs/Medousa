import { cancelAgentSession, createAgentSession, getSessionAgentMode, getSessionCodeBinding } from "$lib/daemon";
import { getSessionBot } from "$lib/daemon/bot";
import { chat } from "$lib/stores/chat.svelte";
import { planAgentWorkspace } from "$lib/utils/agentWorkspacePlan";
import { agentSessionStreamUrl, clearSessionAgentSessionId, getSessionAgentRuntime, getSessionAgentSessionId, getSessionAgentWorkId, setSessionAgentSessionId, setSessionAgentWorkId, setSessionAgentConfigOptions } from "$lib/utils/sessionAgentRuntime";
import type { PreparedAgentSession } from "./agentSessionController.svelte";

const pending = new Map<string, Promise<PreparedAgentSession | null>>();

async function cancelPreparedAgent(sessionId: string, id: string) {
  try { await cancelAgentSession(id); }
  catch (cause) { if (!/unknown agent session|not found|404/i.test(String(cause))) throw cause; }
  clearSessionAgentSessionId(sessionId);
  setSessionAgentConfigOptions(sessionId, []);
}

/** Same lifecycle for Chat and addressed surfaces, serialized per workshop/session. */
export function prepareAgentSession(sessionId: string, runtimeChoice: "cursor" | "codex" | "hermes", options?: { openChooserWhenMissing?: boolean; stopWhenUnbound?: boolean }): Promise<PreparedAgentSession | null> {
  const epoch = chat.workshopEpoch;
  const key = `${epoch}:${sessionId}`;
  const previous = pending.get(key) ?? Promise.resolve();
  const request = previous.catch(() => undefined).then(async () => {
    const assertCurrent = () => { if (chat.workshopEpoch !== epoch) throw new Error("Workshop changed while preparing the agent."); };
    assertCurrent();
    const botResponse = await getSessionBot(sessionId);
    assertCurrent();
    if (botResponse.binding) throw new Error("A Bot’s conversation uses its configured runtime.");
    const [binding, mode] = await Promise.all([
      getSessionCodeBinding(sessionId),
      getSessionAgentMode(sessionId),
    ]);
    assertCurrent();
    if (getSessionAgentRuntime(sessionId) !== runtimeChoice) return null;

    const bindingWorkId = binding.work_id?.trim() || null;
    const currentAgentId = getSessionAgentSessionId(sessionId);
    const action =
      options?.stopWhenUnbound && !bindingWorkId
        ? currentAgentId
          ? "stop"
          : "keep"
        : planAgentWorkspace({
            runtime: runtimeChoice,
            mode: mode.effective_mode,
            bindingWorkId,
            agentSessionId: currentAgentId,
            agentWorkId: getSessionAgentWorkId(sessionId),
          });

    if ((action === "stop" || action === "restart") && currentAgentId) {
      await cancelPreparedAgent(sessionId, currentAgentId);
      assertCurrent();
    }
    if (action === "stop" || action === "wait_for_project") {
      if (options?.openChooserWhenMissing && chat.focusedSessionId === sessionId) {
        window.dispatchEvent(new CustomEvent("medousa-open-code-project-chooser"));
      }
      return null;
    }

    const retainedAgentId = getSessionAgentSessionId(sessionId);
    if (action === "keep" && retainedAgentId) {
      return {
        agentSessionId: retainedAgentId,
        streamUrl: agentSessionStreamUrl(retainedAgentId),
        streamReady: true,
        acceptedAt: new Date().toISOString(),
      };
    }

    const accepted = await createAgentSession({
      session_id: sessionId,
      runtime: runtimeChoice,
      work_id: bindingWorkId,
    });
    assertCurrent();
    const latestBinding = await getSessionCodeBinding(sessionId);
    assertCurrent();
    const latestWorkId = latestBinding.work_id?.trim() || null;
    if (
      getSessionAgentRuntime(sessionId) !== runtimeChoice ||
      latestWorkId !== bindingWorkId
    ) {
      await cancelAgentSession(accepted.agent_session_id).catch(() => undefined);
      return null;
    }

    setSessionAgentSessionId(sessionId, accepted.agent_session_id);
    setSessionAgentWorkId(sessionId, bindingWorkId);
    const configOptions = accepted.config_options ?? [];
    setSessionAgentConfigOptions(sessionId, configOptions);

    return {
      agentSessionId: accepted.agent_session_id,
      streamUrl: accepted.stream_url,
      streamReady: accepted.stream_ready,
      acceptedAt: accepted.accepted_at_utc ?? new Date().toISOString(),
    };
  }).finally(() => { if (pending.get(key) === request) pending.delete(key); });
  pending.set(key, request);
  return request;
}
