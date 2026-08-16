/**
 * ACP / coding-agent session lifecycle for ChatPanel.
 * ChatStore still owns turn start (`beginTurn` / `startTurnStream`).
 */

import {
  cancelAgentSession,
  createAgentSession,
  getSessionAgentMode,
  getSessionCodeBinding,
  setAgentSessionConfigOption,
  type AgentSessionConfigOption,
} from "$lib/daemon";
import { chat } from "$lib/stores/chat.svelte";
import { planAgentWorkspace } from "$lib/utils/agentWorkspacePlan";
import {
  agentSessionStreamUrl,
  clearSessionAgentSessionId,
  getSessionAgentRuntime,
  getSessionAgentSessionId,
  getSessionAgentConfigOptions,
  getSessionAgentWorkId,
  setSessionAgentRuntime,
  setSessionAgentSessionId,
  setSessionAgentConfigOptions,
  setSessionAgentWorkId,
  type ChatAgentRuntime,
} from "$lib/utils/sessionAgentRuntime";

export type PreparedAgentSession = {
  agentSessionId: string;
  streamUrl: string;
  streamReady: boolean;
  acceptedAt: string;
};

export function createAgentSessionController() {
  let sessionRuntime = $state<ChatAgentRuntime>(getSessionAgentRuntime(chat.sessionId));
  let agentConfigOptions = $state<AgentSessionConfigOption[]>(
    getSessionAgentConfigOptions(chat.sessionId) as AgentSessionConfigOption[],
  );
  let agentLifecyclePending = $state(0);
  const preparingAgent = $derived(agentLifecyclePending > 0);
  let agentLifecycleQueue: Promise<void> = Promise.resolve();

  function queueAgentLifecycle<T>(operation: () => Promise<T>): Promise<T> {
    agentLifecyclePending += 1;
    const queued = agentLifecycleQueue.catch(() => undefined).then(operation);
    agentLifecycleQueue = queued.then(
      () => undefined,
      () => undefined,
    );
    return queued.finally(() => {
      agentLifecyclePending = Math.max(0, agentLifecyclePending - 1);
    });
  }

  async function cancelKnownAgent(sessionId: string, agentSessionId: string) {
    try {
      await cancelAgentSession(agentSessionId);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (!/unknown agent session|not found|404/i.test(message)) throw err;
    }
    clearSessionAgentSessionId(sessionId);
    setSessionAgentConfigOptions(sessionId, []);
    if (chat.sessionId === sessionId) agentConfigOptions = [];
  }

  function synchronizeAgentSession(
    sessionId: string,
    runtimeChoice: Exclude<ChatAgentRuntime, "medousa">,
    options?: { openChooserWhenMissing?: boolean; stopWhenUnbound?: boolean },
  ): Promise<PreparedAgentSession | null> {
    return queueAgentLifecycle(async () => {
      const [binding, mode] = await Promise.all([
        getSessionCodeBinding(sessionId),
        getSessionAgentMode(sessionId),
      ]);
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
        await cancelKnownAgent(sessionId, currentAgentId);
      }
      if (action === "stop" || action === "wait_for_project") {
        if (options?.openChooserWhenMissing && chat.sessionId === sessionId) {
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
      const latestBinding = await getSessionCodeBinding(sessionId);
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
      if (chat.sessionId === sessionId) agentConfigOptions = configOptions;
      return {
        agentSessionId: accepted.agent_session_id,
        streamUrl: accepted.stream_url,
        streamReady: accepted.stream_ready,
        acceptedAt: accepted.accepted_at_utc ?? new Date().toISOString(),
      };
    });
  }

  function syncFromFocusedSession() {
    const sessionId = chat.sessionId;
    const runtimeChoice = getSessionAgentRuntime(sessionId);
    sessionRuntime = runtimeChoice;
    agentConfigOptions = getSessionAgentConfigOptions(
      sessionId,
    ) as AgentSessionConfigOption[];
    return { sessionId, runtimeChoice };
  }

  function onRuntimeChange(value: ChatAgentRuntime) {
    const sessionId = chat.sessionId;
    const previousRuntime = getSessionAgentRuntime(sessionId);
    const previousId = getSessionAgentSessionId(sessionId);
    const previousWorkId = getSessionAgentWorkId(sessionId);
    const previousConfigOptions = getSessionAgentConfigOptions(sessionId);
    sessionRuntime = value;
    setSessionAgentRuntime(sessionId, value);
    agentConfigOptions = [];
    void (async () => {
      if (previousId) {
        try {
          await queueAgentLifecycle(() => cancelKnownAgent(sessionId, previousId));
        } catch (err) {
          if (getSessionAgentRuntime(sessionId) === value) {
            setSessionAgentRuntime(sessionId, previousRuntime);
            setSessionAgentSessionId(sessionId, previousId);
            if (previousWorkId !== undefined) {
              setSessionAgentWorkId(sessionId, previousWorkId);
            }
            setSessionAgentConfigOptions(sessionId, previousConfigOptions);
            if (chat.sessionId === sessionId) {
              sessionRuntime = previousRuntime;
              agentConfigOptions = previousConfigOptions as AgentSessionConfigOption[];
              chat.setError(err instanceof Error ? err.message : String(err));
            }
          }
          return;
        }
      }
      if (value !== "medousa") {
        await synchronizeAgentSession(sessionId, value, { openChooserWhenMissing: true }).catch(
          () => {
            /* Sending the first message retries and surfaces provider errors. */
          },
        );
      }
    })();
  }

  function onCodeProjectBindingChanged(
    event: Event & { detail?: { sessionId?: string; workId?: string | null } },
  ) {
    const sessionId = event.detail?.sessionId?.trim();
    if (!sessionId || sessionId !== chat.sessionId) return;
    const runtimeChoice = getSessionAgentRuntime(sessionId);
    if (runtimeChoice === "medousa") return;
    void synchronizeAgentSession(sessionId, runtimeChoice, {
      stopWhenUnbound: !event.detail?.workId,
    }).catch((err) => {
      chat.setError(err instanceof Error ? err.message : String(err));
    });
  }

  async function updateAgentConfig(configId: string, value: unknown) {
    const agentSessionId = getSessionAgentSessionId(chat.sessionId);
    if (!agentSessionId) return;
    const response = await setAgentSessionConfigOption(agentSessionId, configId, value);
    agentConfigOptions = response.config_options;
    setSessionAgentConfigOptions(chat.sessionId, agentConfigOptions);
  }

  return {
    get sessionRuntime() {
      return sessionRuntime;
    },
    get agentConfigOptions() {
      return agentConfigOptions;
    },
    set agentConfigOptions(next: AgentSessionConfigOption[]) {
      agentConfigOptions = next;
    },
    get preparingAgent() {
      return preparingAgent;
    },
    synchronizeAgentSession,
    syncFromFocusedSession,
    onRuntimeChange,
    onCodeProjectBindingChanged,
    updateAgentConfig,
  };
}
