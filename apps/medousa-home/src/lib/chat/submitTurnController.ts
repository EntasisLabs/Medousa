/**
 * Interactive/background turn start from ChatPanel.
 * Uses ChatStore.beginTurn + startTurnStream — no second stream apply path.
 */

import { createTurnTicket, promptAgentSession } from "$lib/daemon";
import type { TurnTicketResponse } from "$lib/types/session";
import { buildInteractiveTurnOptions } from "$lib/interactiveTurnOptions";
import { chat } from "$lib/stores/chat.svelte";
import { userProfiles } from "$lib/stores/userProfiles.svelte";
import { voicePresets } from "$lib/stores/voicePresets.svelte";
import { activeCodeContext } from "$lib/utils/undertakingWorkspace";
import {
  agentSessionStreamUrl,
  clearSessionAgentSessionId,
  getSessionAgentRuntime,
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
    runtime: Exclude<ReturnType<typeof getSessionAgentRuntime>, "medousa">,
    options?: { openChooserWhenMissing?: boolean },
  ) => Promise<PreparedAgentSession | null>;
  onAgentSessionLost: () => void;
  scrollToLatest: (force: boolean) => void;
}): Promise<void> {
  const codeProjectSetupAuthorized = input.codeProjectSetupAuthorized ?? false;
  const runtime = getSessionAgentRuntime(chat.sessionId);
  if (runtime !== "medousa" && input.mode === "interactive" && !codeProjectSetupAuthorized) {
    const prepared = await input.synchronizeAgentSession(chat.sessionId, runtime, {
      openChooserWhenMissing: true,
    });
    if (!prepared) throw new Error("Choose a project before starting a coding agent.");
    const { agentSessionId, streamUrl, streamReady, acceptedAt } = prepared;

    const ticket: TurnTicketResponse = {
      turn_id: agentSessionId,
      session_id: chat.sessionId,
      mode: "interactive",
      phase: "accepted" as TurnTicketResponse["phase"],
      accepted_at_utc: acceptedAt,
      stream_url: streamUrl || agentSessionStreamUrl(agentSessionId),
      stream_ready: streamReady,
    };
    chat.beginTurn(input.userContent, ticket, [], userProfiles.activeProfileId);
    chat.clearPendingMedia();
    input.scrollToLatest(true);
    await chat.startTurnStream(ticket.turn_id, ticket.session_id, ticket.stream_url);
    try {
      await promptAgentSession(agentSessionId, input.prompt, activeCodeContext(chat.sessionId));
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (/unknown agent session|not found|404/i.test(message)) {
        clearSessionAgentSessionId(chat.sessionId);
        setSessionAgentConfigOptions(chat.sessionId, []);
        input.onAgentSessionLost();
      }
      throw err;
    }
    return;
  }

  const opts = buildInteractiveTurnOptions();
  const mediaRefs = [...chat.pendingMediaRefs];
  const voice = voicePresets.turnVoiceFields();
  const codeContext = activeCodeContext(chat.sessionId);
  const accepted = await createTurnTicket({
    sessionId: chat.sessionId,
    prompt: input.prompt,
    mode: input.mode,
    codeContext,
    codeProjectSetupAuthorized,
    provider: opts.provider,
    model: opts.model,
    responseDepthMode: opts.responseDepthMode,
    reasoningEffort: opts.reasoningEffort,
    stageRouting: opts.stageRouting,
    channelSurface: opts.channelSurface,
    mediaRefs,
    voicePresetId: voice.voicePresetId,
    voiceAppendix: voice.voiceAppendix,
    identityUserId: opts.identityUserId,
  });
  chat.beginTurn(
    input.userContent,
    accepted,
    mediaRefs,
    opts.identityUserId ?? userProfiles.activeProfileId,
  );
  chat.clearPendingMedia();
  input.scrollToLatest(true);
  await chat.startTurnStream(accepted.turn_id, accepted.session_id, accepted.stream_url);
}
