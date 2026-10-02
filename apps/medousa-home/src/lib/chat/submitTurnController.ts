/**
 * Interactive/background turn start from ChatPanel.
 * Uses ChatStore.beginTurn + startTurnStream — no second stream apply path.
 */

import { sendRuntimeBotTurn } from "./runtimeBotTurns.svelte";
import { createTurnTicket, promptAgentSession } from "$lib/daemon";
import type { TurnTicketResponse } from "$lib/types/session";
import { prepareInteractiveTurnOptions } from "$lib/interactiveTurnOptions";
import { chat } from "$lib/stores/chat.svelte";
import { bots } from "$lib/stores/bots.svelte";
import { getSessionBot } from "$lib/daemon/bot";
import { executionTargets } from "$lib/stores/executionTargets.svelte";
import { chatInteractions } from "$lib/liquid/surfaces/chat/chatInteractions";
import { recordLiquidMetric, recordLiquidPresentationOpportunity } from "$lib/liquid/observability";
import { userProfiles } from "$lib/stores/userProfiles.svelte";
import { voicePresets } from "$lib/stores/voicePresets.svelte";
import { activeCodeContext } from "$lib/utils/undertakingWorkspace";
import { promptWithConversationContext } from "$lib/utils/agentConversationContext";
import {
  agentConversationContextSeeded,
  markAgentConversationContextSeeded,
  agentSessionStreamUrl,
  clearSessionAgentSessionId,
  getSessionAgentRuntime,
  isExternalAgentRuntime,
  isProviderConversationRuntime,
  setSessionAgentConfigOptions,
} from "$lib/utils/sessionAgentRuntime";
import type { PreparedAgentSession } from "./agentSessionController.svelte";

export async function submitChatTurn(input: {
  userContent: string;
  prompt: string;
  mode: "interactive" | "background";
  codeProjectSetupAuthorized?: boolean;
  synchronizeAgentSession: (
    sessionId: string,
    runtime: "cursor" | "codex" | "hermes",
    options?: { openChooserWhenMissing?: boolean },
  ) => Promise<PreparedAgentSession | null>;
  onAgentSessionLost: () => void;
  scrollToLatest: (force: boolean) => void;
  onAccepted?: (ticket: TurnTicketResponse) => void;
  responseVoiceAppendix?: string;
}): Promise<void> {
  const workshopEpoch = chat.workshopEpoch;
  const sessionId = chat.focusedSessionId || chat.sessionId;
  const current = () => chat.workshopEpoch === workshopEpoch && (chat.focusedSessionId || chat.sessionId) === sessionId;
  if (!chat.workshopScopeId) {
    throw new Error("Workshop is switching; wait for it to reconnect");
  }
  const identityUserId = userProfiles.turnIdentityUserId();
  const codeProjectSetupAuthorized = input.codeProjectSetupAuthorized ?? false;
  let bot = bots.forSession(sessionId);
  let runtime = bot ? "medousa" : getSessionAgentRuntime(sessionId);
  if (!bot && !isProviderConversationRuntime(runtime)) {
    const result = await getSessionBot(sessionId);
    if (!current()) throw new Error("Conversation changed while checking the session identity");
    if (result.binding) { runtime = "medousa"; bot = result.bot ?? null; }
  }
  if (bot?.external_agent) {
    if (chat.pendingMediaRefs.length > 0) throw new Error("Runtime Bots currently accept text only.");
    const admitted = await sendRuntimeBotTurn(bot, sessionId, input.prompt);
    if (!current() || !admitted) return;
    input.onAccepted?.({turn_id: admitted.ticket.turnId,session_id:sessionId,mode:input.mode,phase:"accepted",accepted_at_utc:new Date().toISOString(),stream_url:"",stream_ready:false});
    input.scrollToLatest(true);
    return;
  }
  if (isProviderConversationRuntime(runtime)) {
    throw new Error("Provider conversations must use the selected session or bot.");
  }
  if (isExternalAgentRuntime(runtime) && input.mode === "interactive" && !codeProjectSetupAuthorized) {
    const prepared = await input.synchronizeAgentSession(sessionId, runtime, {
      openChooserWhenMissing: true,
    });
    if (!prepared) throw new Error("Choose a project before starting a coding agent.");
    const { agentSessionId, streamUrl, streamReady, acceptedAt } = prepared;
    const prompt = agentConversationContextSeeded(sessionId, agentSessionId)
      ? input.prompt
      : promptWithConversationContext(input.prompt, chat.messagesFor(sessionId));

    const ticket: TurnTicketResponse = {
      turn_id: agentSessionId,
      session_id: sessionId,
      mode: "interactive",
      phase: "accepted" as TurnTicketResponse["phase"],
      accepted_at_utc: acceptedAt,
      stream_url: streamUrl || agentSessionStreamUrl(agentSessionId),
      stream_ready: streamReady,
    };
    if (!current()) {
      throw new Error("Conversation changed while the turn was being admitted");
    }
    chat.beginTurn(input.userContent, ticket, [], identityUserId);
    input.onAccepted?.(ticket);
    chat.clearPendingMedia();
    input.scrollToLatest(true);
    await chat.startTurnStream(ticket.turn_id, ticket.session_id, ticket.stream_url);
    if (!current()) {
      throw new Error("Conversation changed before the agent prompt was sent");
    }
    try {
      await promptAgentSession(agentSessionId, prompt, activeCodeContext(sessionId));
      if (chat.workshopEpoch === workshopEpoch) markAgentConversationContextSeeded(sessionId, agentSessionId);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (/unknown agent session|not found|404/i.test(message)) {
        clearSessionAgentSessionId(sessionId);
        setSessionAgentConfigOptions(sessionId, []);
        input.onAgentSessionLost();
      }
      throw err;
    }
    return;
  }

  const opts = await prepareInteractiveTurnOptions(chat);
  if (!current()) throw new Error("Conversation changed while preparing the turn");
  const mediaRefs = [...chat.pendingMediaRefs];
  const voice = voicePresets.turnVoiceFields();
  const codeContext = activeCodeContext(sessionId);
  const liquidInteractions = chatInteractions.envelopes(sessionId);
  recordLiquidPresentationOpportunity(input.prompt);
  const accepted = await createTurnTicket({
    sessionId,
    prompt: input.prompt,
    mode: input.mode,
    codeContext,
    codeProjectSetupAuthorized,
    workerExecutionTarget: executionTargets.turnSelection(sessionId),
    provider: opts.provider,
    model: opts.model,
    responseDepthMode: opts.responseDepthMode,
    reasoningEffort: opts.reasoningEffort,
    stageRouting: opts.stageRouting,
    channelSurface: opts.channelSurface,
    browserDriverId: opts.browserDriverId,
    selectedWorlds: opts.selectedWorlds,
    mediaRefs,
    liquidInteractions,
    voicePresetId: voice.voicePresetId,
    voiceAppendix: [voice.voiceAppendix, input.responseVoiceAppendix].filter(Boolean).join("\n") || undefined,
    identityUserId: opts.identityUserId,
  });
  if (!current()) {
    throw new Error("Workshop changed while the turn was being admitted");
  }
  chatInteractions.ack(sessionId, liquidInteractions.length);
  recordLiquidMetric("interactionsDelivered", liquidInteractions.length);
  chat.beginTurn(
    input.userContent,
    accepted,
    mediaRefs,
    opts.identityUserId ?? identityUserId,
  );
  chat.clearPendingMedia();
  input.scrollToLatest(true);
  input.onAccepted?.(accepted);
  await chat.startTurnStream(accepted.turn_id, accepted.session_id, accepted.stream_url);
}
