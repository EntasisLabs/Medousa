/**
 * ACP / coding-agent session lifecycle for ChatPanel.
 * ChatStore still owns turn start (`beginTurn` / `startTurnStream`).
 */

import { untrack } from "svelte";
import {
  cancelAgentSession,
  setAgentSessionConfigOption,
  type AgentSessionConfigOption,
} from "$lib/daemon";
import { bots } from "$lib/stores/bots.svelte";
import { chat } from "$lib/stores/chat.svelte";
import { prepareAgentSession } from "./prepareAgentSession";
import {
  clearSessionAgentSessionId,
  getSessionAgentRuntime,
  getSessionAgentSessionId,
  getSessionAgentConfigOptions,
  getSessionAgentWorkId,
  isExternalAgentRuntime,
  isProviderConversationRuntime,
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
  let sessionRuntime = $state<ChatAgentRuntime>(getSessionAgentRuntime(chat.focusedSessionId));
  let agentConfigOptions = $state<AgentSessionConfigOption[]>(
    getSessionAgentConfigOptions(chat.focusedSessionId) as AgentSessionConfigOption[],
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
    if (chat.focusedSessionId === sessionId) agentConfigOptions = [];
  }

  function synchronizeAgentSession(
    sessionId: string,
    runtimeChoice: "cursor" | "codex" | "hermes",
    options?: { openChooserWhenMissing?: boolean; stopWhenUnbound?: boolean },
  ): Promise<PreparedAgentSession | null> {
    return queueAgentLifecycle(async () => {
      const prepared = await prepareAgentSession(sessionId, runtimeChoice, options);
      if (chat.focusedSessionId === sessionId) agentConfigOptions = getSessionAgentConfigOptions(sessionId) as AgentSessionConfigOption[];
      return prepared;
    });
  }

  function syncFromFocusedSession() {
    const sessionId = chat.focusedSessionId;
    const runtimeChoice = bots.forSession(sessionId) ? "medousa" : getSessionAgentRuntime(sessionId);
    sessionRuntime = runtimeChoice;
    agentConfigOptions = getSessionAgentConfigOptions(
      sessionId,
    ) as AgentSessionConfigOption[];
    return { sessionId, runtimeChoice };
  }

  function onRuntimeChange(value: ChatAgentRuntime) {
    const sessionId = chat.focusedSessionId;
    if (bots.forSession(sessionId) || isProviderConversationRuntime(getSessionAgentRuntime(sessionId)) || isProviderConversationRuntime(value)) return;
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
            if (chat.focusedSessionId === sessionId) {
              sessionRuntime = previousRuntime;
              agentConfigOptions = previousConfigOptions as AgentSessionConfigOption[];
              chat.setError(err instanceof Error ? err.message : String(err));
            }
          }
          return;
        }
      }
      if (isExternalAgentRuntime(value)) {
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
    if (!sessionId || sessionId !== chat.focusedSessionId) return;
    const runtimeChoice = getSessionAgentRuntime(sessionId);
    if (!isExternalAgentRuntime(runtimeChoice)) return;
    void synchronizeAgentSession(sessionId, runtimeChoice, {
      stopWhenUnbound: !event.detail?.workId,
    }).catch((err) => {
      chat.setError(err instanceof Error ? err.message : String(err));
    });
  }

  async function updateAgentConfig(configId: string, value: unknown) {
    const agentSessionId = getSessionAgentSessionId(chat.focusedSessionId);
    if (!agentSessionId) return;
    const response = await setAgentSessionConfigOption(agentSessionId, configId, value);
    agentConfigOptions = response.config_options;
    setSessionAgentConfigOptions(chat.focusedSessionId, agentConfigOptions);
  }

  $effect(() => {
    chat.workshopScopeId;
    const { sessionId, runtimeChoice } = syncFromFocusedSession();
    if (!isProviderConversationRuntime(runtimeChoice)) {
      void untrack(() => bots.refreshSessionBinding(sessionId)).catch(() => undefined);
    }
    if (isExternalAgentRuntime(runtimeChoice)) {
      // Busy counters belong outside this effect's dependency graph.
      void untrack(() => synchronizeAgentSession(sessionId, runtimeChoice)).catch(() => {
        // First send retries and surfaces connection/provider errors.
      });
    }
  });

  $effect(() => {
    window.addEventListener("medousa-code-project-binding-changed", onCodeProjectBindingChanged as EventListener);
    return () => window.removeEventListener("medousa-code-project-binding-changed", onCodeProjectBindingChanged as EventListener);
  });

  return {
    get sessionRuntime() {
      return bots.forSession(chat.focusedSessionId) ? "medousa" : sessionRuntime;
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
